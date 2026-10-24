use crate::kdeconnect::packet::NetworkPacket;
use anyhow::{anyhow, Context, Result};
use russh::client::{self, AuthResult, Config, GexParams, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{decode_secret_key, PrivateKeyWithHashAlg};
use russh::{cipher, compression, kex, mac, Preferred};
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpRootEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpBrowseResponse {
    pub current_path: String,
    pub roots: Vec<SftpRootEntry>,
    pub entries: Vec<SftpFileEntry>,
    pub unconfigured_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SftpServerInfo {
    pub ip: IpAddr,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub roots: Vec<SftpRootEntry>,
}

#[derive(Debug, Clone)]
pub enum SftpHandshakeOutcome {
    Ready(SftpServerInfo),
    Unconfigured(String),
}

struct SshClientHandler;

impl client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        // Matches kdeconnect-kde `StrictHostKeyChecking=no` over local LAN after TLS pairing
        Ok(true)
    }
}

struct ActiveSftpConnection {
    info: SftpServerInfo,
    ssh_handle: Arc<Handle<SshClientHandler>>,
    sftp: Arc<SftpSession>,
}

#[derive(Default)]
pub struct SftpManager {
    active: Mutex<HashMap<String, ActiveSftpConnection>>,
    pending_handshakes: Mutex<HashMap<String, Vec<oneshot::Sender<SftpHandshakeOutcome>>>>,
    connect_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl SftpManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn device_connect_lock(&self, device_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut guard = self.connect_locks.lock().unwrap();
        guard
            .entry(device_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    pub fn clear_device(&self, device_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(device_id);
        }
        if let Ok(mut pending) = self.pending_handshakes.lock() {
            pending.remove(device_id);
        }
    }

    pub fn handle_incoming_sftp_packet(
        &self,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        packet: &NetworkPacket,
    ) {
        if packet.get_bool("serverRunning") == Some(false) || packet.get_bool("stop") == Some(true)
        {
            log::info!(
                "[sftp] Device '{}' ({}) reported SFTP server stopped",
                device_name,
                device_id
            );
            self.clear_device(device_id);
            return;
        }

        if let Some(err_msg) = packet.get_str("errorMessage") {
            log::warn!(
                "[sftp] Device '{}' ({}) returned SFTP errorMessage: {}",
                device_name,
                device_id,
                err_msg
            );
            self.clear_device(device_id);
            let waiters = self
                .pending_handshakes
                .lock()
                .ok()
                .and_then(|mut map| map.remove(device_id))
                .unwrap_or_default();
            for tx in waiters {
                let _ = tx.send(SftpHandshakeOutcome::Unconfigured(err_msg.to_string()));
            }
            return;
        }

        let Some(port) = packet.get_i64("port").and_then(|p| u16::try_from(p).ok()) else {
            log::warn!(
                "[sftp] Ignoring SFTP packet without valid port from '{}': {:?}",
                device_name,
                packet.body
            );
            return;
        };

        let user = packet
            .get_str("user")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("kdeconnect")
            .to_string();
        let password = packet.get_str("password").unwrap_or("").to_string();

        // Prefer the active TLS socket peer IP (matches kdeconnect-kde Mounter::onPacketReceived)
        let ip = if !peer_ip.is_unspecified() {
            peer_ip
        } else {
            packet
                .get_str("ip")
                .and_then(|s| s.parse::<IpAddr>().ok())
                .unwrap_or(peer_ip)
        };

        let mut roots = Vec::new();
        let multi_paths: Vec<String> = packet
            .body
            .get("multiPaths")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let path_names: Vec<String> = packet
            .body
            .get("pathNames")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if !multi_paths.is_empty() {
            for (idx, p) in multi_paths.into_iter().enumerate() {
                let normalized = if p.starts_with('/') {
                    p
                } else {
                    format!("/{}", p)
                };
                let label = path_names
                    .get(idx)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| {
                        normalized
                            .rsplit('/')
                            .find(|seg| !seg.is_empty())
                            .unwrap_or("Storage")
                            .to_string()
                    });
                roots.push(SftpRootEntry {
                    name: label,
                    path: normalized,
                });
            }
        } else {
            let single_path = packet
                .get_str("path")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("/")
                .to_string();
            let normalized = if single_path.starts_with('/') {
                single_path
            } else {
                format!("/{}", single_path)
            };
            roots.push(SftpRootEntry {
                name: "Internal storage".to_string(),
                path: normalized,
            });
        }

        let info = SftpServerInfo {
            ip,
            port,
            user,
            password,
            roots,
        };

        log::info!(
            "[sftp] Received SFTP server info from '{}' ({}): {}:{} user='{}' roots={:?}",
            device_name,
            device_id,
            info.ip,
            info.port,
            info.user,
            info.roots
        );

        let waiters = self
            .pending_handshakes
            .lock()
            .ok()
            .and_then(|mut map| map.remove(device_id))
            .unwrap_or_default();
        for tx in waiters {
            let _ = tx.send(SftpHandshakeOutcome::Ready(info.clone()));
        }
    }

    fn get_cached_session(&self, device_id: &str) -> Option<(SftpServerInfo, Arc<SftpSession>)> {
        let mut guard = self.active.lock().ok()?;
        if let Some(conn) = guard.get(device_id) {
            if !conn.ssh_handle.is_closed() {
                return Some((conn.info.clone(), Arc::clone(&conn.sftp)));
            }
        }
        guard.remove(device_id);
        None
    }

    async fn ensure_connected<F>(
        &self,
        device_id: &str,
        config_dir: &Path,
        force_reconnect: bool,
        send_start_browsing: F,
    ) -> Result<Result<(SftpServerInfo, Arc<SftpSession>), String>>
    where
        F: FnOnce() -> Result<()>,
    {
        let dev_lock = self.device_connect_lock(device_id);
        let _guard = dev_lock.lock().await;

        if !force_reconnect {
            if let Some(cached) = self.get_cached_session(device_id) {
                return Ok(Ok(cached));
            }
        } else {
            self.clear_device(device_id);
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_handshakes.lock().unwrap();
            pending.entry(device_id.to_string()).or_default().push(tx);
        }

        send_start_browsing()?;

        let outcome = tokio::time::timeout(Duration::from_secs(12), rx)
            .await
            .context("Timed out waiting for phone to start SFTP server")?
            .context("SFTP handshake cancelled")?;

        match outcome {
            SftpHandshakeOutcome::Unconfigured(msg) => Ok(Err(msg)),
            SftpHandshakeOutcome::Ready(info) => {
                let (ssh_handle, sftp) = connect_ssh_sftp(&info, config_dir).await?;
                let sftp_arc = Arc::new(sftp);
                let handle_arc = Arc::new(ssh_handle);
                {
                    let mut active = self.active.lock().unwrap();
                    active.insert(
                        device_id.to_string(),
                        ActiveSftpConnection {
                            info: info.clone(),
                            ssh_handle: handle_arc,
                            sftp: Arc::clone(&sftp_arc),
                        },
                    );
                }
                Ok(Ok((info, sftp_arc)))
            }
        }
    }

    pub async fn browse<F>(
        &self,
        device_id: &str,
        requested_path: Option<String>,
        config_dir: &Path,
        send_start_browsing: F,
    ) -> Result<SftpBrowseResponse>
    where
        F: Fn() -> Result<()>,
    {
        for attempt in 0..2 {
            let force_reconnect = attempt > 0;
            let session_res = self
                .ensure_connected(device_id, config_dir, force_reconnect, &send_start_browsing)
                .await?;

            let (mut info, sftp) = match session_res {
                Ok(pair) => pair,
                Err(unconfigured_msg) => {
                    return Ok(SftpBrowseResponse {
                        current_path: "/".to_string(),
                        roots: Vec::new(),
                        entries: Vec::new(),
                        unconfigured_message: Some(unconfigured_msg),
                    });
                }
            };

            let target_path = match requested_path
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                Some(p) => p.to_string(),
                None => {
                    if info.roots.len() == 1 {
                        info.roots[0].path.clone()
                    } else {
                        "/".to_string()
                    }
                }
            };

            // If multiple roots are configured and the user is at "/", list the storage roots cleanly
            if target_path == "/" && info.roots.len() > 1 {
                let entries = info
                    .roots
                    .iter()
                    .map(|r| SftpFileEntry {
                        name: r.name.clone(),
                        path: r.path.clone(),
                        is_dir: true,
                        size: 0,
                        modified_secs: None,
                    })
                    .collect();

                return Ok(SftpBrowseResponse {
                    current_path: "/".to_string(),
                    roots: info.roots,
                    entries,
                    unconfigured_message: None,
                });
            }

            match read_sftp_directory(&sftp, &target_path).await {
                Ok(entries) => {
                    return Ok(SftpBrowseResponse {
                        current_path: target_path,
                        roots: info.roots,
                        entries,
                        unconfigured_message: None,
                    });
                }
                Err(primary_err) => {
                    // Fallback: if single-root default path isn't found because the Android SAF SFTP server
                    // already chrooted its root to "/", try "/" automatically before reconnecting.
                    if requested_path.is_none() && target_path != "/" {
                        if let Ok(fallback_entries) = read_sftp_directory(&sftp, "/").await {
                            if let Some(first_root) = info.roots.first_mut() {
                                first_root.path = "/".to_string();
                            }
                            if let Ok(mut active) = self.active.lock() {
                                if let Some(conn) = active.get_mut(device_id) {
                                    conn.info.roots = info.roots.clone();
                                }
                            }
                            return Ok(SftpBrowseResponse {
                                current_path: "/".to_string(),
                                roots: info.roots,
                                entries: fallback_entries,
                                unconfigured_message: None,
                            });
                        }
                    }

                    log::warn!(
                        "[sftp] read_dir('{}') failed on attempt {}: {}",
                        target_path,
                        attempt + 1,
                        primary_err
                    );
                    if attempt == 1 {
                        return Err(primary_err);
                    }
                }
            }
        }

        Err(anyhow!("Failed to browse remote directory"))
    }

    pub async fn download_file<F>(
        &self,
        device_id: &str,
        remote_path: &str,
        dest_path: &Path,
        config_dir: &Path,
        send_start_browsing: F,
    ) -> Result<PathBuf>
    where
        F: Fn() -> Result<()>,
    {
        for attempt in 0..2 {
            let force_reconnect = attempt > 0;
            let session_res = self
                .ensure_connected(device_id, config_dir, force_reconnect, &send_start_browsing)
                .await?;

            let (_, sftp) = match session_res {
                Ok(pair) => pair,
                Err(unconfigured_msg) => {
                    return Err(anyhow!(unconfigured_msg));
                }
            };

            match stream_sftp_file_to_disk(&sftp, remote_path, dest_path).await {
                Ok(()) => return Ok(dest_path.to_path_buf()),
                Err(e) => {
                    let _ = tokio::fs::remove_file(dest_path).await;
                    log::warn!(
                        "[sftp] download_file('{}') failed on attempt {}: {}",
                        remote_path,
                        attempt + 1,
                        e
                    );
                    if attempt == 1 {
                        return Err(e);
                    }
                }
            }
        }

        Err(anyhow!("Failed to download file from device"))
    }

    pub async fn delete_entry<F>(
        &self,
        device_id: &str,
        remote_path: &str,
        is_dir: bool,
        config_dir: &Path,
        send_start_browsing: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<()>,
    {
        let trimmed = remote_path.trim();
        if trimmed.is_empty() || trimmed == "/" {
            return Err(anyhow!("Cannot delete root storage directory"));
        }

        for attempt in 0..2 {
            let force_reconnect = attempt > 0;
            let session_res = self
                .ensure_connected(device_id, config_dir, force_reconnect, &send_start_browsing)
                .await?;

            let (info, sftp) = match session_res {
                Ok(pair) => pair,
                Err(unconfigured_msg) => {
                    return Err(anyhow!(unconfigured_msg));
                }
            };

            if info.roots.iter().any(|r| r.path == trimmed) {
                return Err(anyhow!("Cannot delete a top-level storage location"));
            }

            let res = if is_dir {
                delete_sftp_directory_recursive(&sftp, trimmed).await
            } else {
                sftp.remove_file(trimmed)
                    .await
                    .with_context(|| format!("Failed to delete file '{}'", trimmed))
            };

            match res {
                Ok(()) => return Ok(()),
                Err(e) => {
                    log::warn!(
                        "[sftp] delete_entry('{}', is_dir={}) failed on attempt {}: {}",
                        trimmed,
                        is_dir,
                        attempt + 1,
                        e
                    );
                    if attempt == 1 {
                        return Err(e);
                    }
                }
            }
        }

        Err(anyhow!("Failed to delete item on device"))
    }
}

async fn connect_ssh_sftp(
    info: &SftpServerInfo,
    config_dir: &Path,
) -> Result<(Handle<SshClientHandler>, SftpSession)> {
    use russh::keys::ssh_key::{Algorithm, EcdsaCurve, HashAlg};

    let preferred = Preferred {
        kex: Cow::Borrowed(&[
            kex::CURVE25519,
            kex::CURVE25519_PRE_RFC_8731,
            kex::ECDH_SHA2_NISTP256,
            kex::ECDH_SHA2_NISTP384,
            kex::ECDH_SHA2_NISTP521,
            kex::DH_GEX_SHA256,
            kex::DH_G16_SHA512,
            kex::DH_G14_SHA256,
            kex::DH_G14_SHA1,
            kex::DH_GEX_SHA1,
            kex::DH_G1_SHA1,
            kex::EXTENSION_SUPPORT_AS_CLIENT,
        ]),
        key: Cow::Borrowed(&[
            Algorithm::Ed25519,
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP384,
            },
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP521,
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512),
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
            Algorithm::Rsa { hash: None },
        ]),
        cipher: Cow::Borrowed(&[
            cipher::CHACHA20_POLY1305,
            cipher::AES_256_GCM,
            cipher::AES_256_CTR,
            cipher::AES_192_CTR,
            cipher::AES_128_CTR,
        ]),
        mac: Cow::Borrowed(&[
            mac::HMAC_SHA512_ETM,
            mac::HMAC_SHA256_ETM,
            mac::HMAC_SHA512,
            mac::HMAC_SHA256,
            mac::HMAC_SHA1_ETM,
            mac::HMAC_SHA1,
        ]),
        compression: Cow::Borrowed(&[compression::NONE]),
    };

    let mut ssh_config = Config {
        preferred,
        window_size: 4 * 1024 * 1024,
        maximum_packet_size: 65535,
        inactivity_timeout: Some(Duration::from_secs(180)),
        keepalive_interval: Some(Duration::from_secs(25)),
        ..Config::default()
    };
    if let Ok(gex) = GexParams::new(1024, 2048, 8192) {
        ssh_config.gex = gex;
    }

    let addr = SocketAddr::new(info.ip, info.port);
    let mut handle = tokio::time::timeout(
        Duration::from_secs(10),
        client::connect(Arc::new(ssh_config), addr, SshClientHandler),
    )
    .await
    .context("Timed out connecting to phone SFTP port")?
    .context("Failed to establish SSH connection to phone")?;

    let mut authenticated = false;

    if !info.password.is_empty() {
        if let Ok(AuthResult::Success) = handle
            .authenticate_password(&info.user, &info.password)
            .await
        {
            authenticated = true;
        }
    }

    if !authenticated && !info.password.is_empty() {
        if let Ok(mut resp) = handle
            .authenticate_keyboard_interactive_start(&info.user, None::<String>)
            .await
        {
            for _ in 0..3 {
                match resp {
                    KeyboardInteractiveAuthResponse::Success => {
                        authenticated = true;
                        break;
                    }
                    KeyboardInteractiveAuthResponse::Failure { .. } => break,
                    KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                        let answers = vec![info.password.clone(); prompts.len()];
                        match handle
                            .authenticate_keyboard_interactive_respond(answers)
                            .await
                        {
                            Ok(next) => resp = next,
                            Err(_) => break,
                        }
                    }
                }
            }
        }
    }

    if !authenticated {
        let key_pem_path = config_dir.join("key.pem");
        if let Ok(key_pem) = std::fs::read_to_string(&key_pem_path) {
            if let Ok(secret_key) = decode_secret_key(&key_pem, None) {
                let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(secret_key), None);
                if let Ok(AuthResult::Success) = handle
                    .authenticate_publickey(&info.user, key_with_alg)
                    .await
                {
                    authenticated = true;
                }
            }
        }
    }

    if !authenticated {
        return Err(anyhow!(
            "Phone SFTP server rejected authentication for user '{}'",
            info.user
        ));
    }

    let channel = handle
        .channel_open_session()
        .await
        .context("Failed to open SSH session channel")?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .context("Failed to request SFTP subsystem on phone")?;

    let normalized_stream = wrap_sftp_stream_with_status_normalizer(channel.into_stream());
    let sftp = SftpSession::new(normalized_stream)
        .await
        .context("Failed to initialize SFTP protocol session")?;
    sftp.set_timeout(10);

    Ok((handle, sftp))
}

/// Wraps an SFTP transport stream to normalize SFTP v4–v6 status codes (`>= 9`, such as
/// Apache MINA SSHD's `18 = SSH_FX_DIR_NOT_EMPTY` or `10 = SSH_FX_NO_SUCH_PATH`) into
/// SFTP v3 status codes (`0..=8`) so `russh-sftp` never drops a status reply packet.
fn wrap_sftp_stream_with_status_normalizer<S>(remote_stream: S) -> tokio::io::DuplexStream
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (client_side, proxy_side) = tokio::io::duplex(256 * 1024);
    let (mut remote_rd, mut remote_wr) = tokio::io::split(remote_stream);
    let (mut proxy_rd, mut proxy_wr) = tokio::io::split(proxy_side);

    // Client -> Phone SFTP server (raw pass-through)
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut proxy_rd, &mut remote_wr).await;
        let _ = remote_wr.shutdown().await;
    });

    // Phone SFTP server -> Client (rewrites SSH_FXP_STATUS codes > 8 to SFTP v3 codes)
    tokio::spawn(async move {
        const SSH_FXP_STATUS: u8 = 101;
        let mut frame = Vec::with_capacity(16 * 1024);

        while let Ok(len) = remote_rd.read_u32().await {
            if len > 16 * 1024 * 1024 {
                break;
            }
            frame.resize(len as usize, 0);
            if remote_rd.read_exact(&mut frame).await.is_err() {
                break;
            }

            // SSH_FXP_STATUS layout inside frame: [type: u8 (1)][req_id: u32 (4)][status_code: u32 (4)]...
            if frame.len() >= 9 && frame[0] == SSH_FXP_STATUS {
                let raw_code = u32::from_be_bytes([frame[5], frame[6], frame[7], frame[8]]);
                if raw_code > 8 {
                    let mapped_code: u32 = match raw_code {
                        10 => 2, // SSH_FX_NO_SUCH_PATH -> SSH_FX_NO_SUCH_FILE (2)
                        12 => 3, // SSH_FX_WRITE_PROTECT -> SSH_FX_PERMISSION_DENIED (3)
                        _ => 4,  // e.g. 18 (SSH_FX_DIR_NOT_EMPTY) -> SSH_FX_FAILURE (4)
                    };
                    frame[5..9].copy_from_slice(&mapped_code.to_be_bytes());
                }
            }

            if proxy_wr.write_u32(len).await.is_err() {
                break;
            }
            if proxy_wr.write_all(&frame).await.is_err() {
                break;
            }
        }
        let _ = proxy_wr.shutdown().await;
    });

    client_side
}

async fn read_sftp_directory(sftp: &SftpSession, dir_path: &str) -> Result<Vec<SftpFileEntry>> {
    let read_dir = sftp
        .read_dir(dir_path)
        .await
        .with_context(|| format!("Unable to list directory '{}'", dir_path))?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let name = entry.file_name();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let meta = entry.metadata();
        let is_dir = meta
            .permissions
            .map(|p| (p & 0xF000) == 0x4000)
            .unwrap_or_else(|| meta.is_dir());
        let size = meta.size.unwrap_or(0);
        let modified_secs = meta.mtime.map(|m| m as u64);

        let full_path = if dir_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", dir_path.trim_end_matches('/'), name)
        };

        entries.push(SftpFileEntry {
            name,
            path: full_path,
            is_dir,
            size,
            modified_secs,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

async fn stream_sftp_file_to_disk(
    sftp: &SftpSession,
    remote_path: &str,
    dest_path: &Path,
) -> Result<()> {
    let remote_file = sftp
        .open(remote_path)
        .await
        .with_context(|| format!("Failed to open remote file '{}'", remote_path))?;

    let mut reader = BufReader::with_capacity(256 * 1024, remote_file);
    let mut local_file = tokio::fs::File::create(dest_path)
        .await
        .with_context(|| format!("Failed to create local file '{:?}'", dest_path))?;

    tokio::io::copy_buf(&mut reader, &mut local_file)
        .await
        .context("Failed while streaming file over SFTP")?;
    local_file.flush().await?;
    Ok(())
}

async fn delete_sftp_directory_recursive(sftp: &Arc<SftpSession>, root_dir: &str) -> Result<()> {
    // Iterative post-order traversal: always empty files and subdirectories first before calling remove_dir
    let mut stack: Vec<(String, bool)> = vec![(root_dir.to_string(), false)];

    while let Some((dir_path, children_processed)) = stack.pop() {
        if children_processed {
            sftp.remove_dir(&dir_path)
                .await
                .with_context(|| format!("Failed to remove directory '{}'", dir_path))?;
            continue;
        }

        stack.push((dir_path.clone(), true));

        let read_dir = sftp
            .read_dir(&dir_path)
            .await
            .with_context(|| format!("Failed to read directory '{}' for deletion", dir_path))?;

        let mut files_to_delete = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let child_path = if dir_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", dir_path.trim_end_matches('/'), name)
            };

            let meta = entry.metadata();
            let child_is_dir = meta
                .permissions
                .map(|p| (p & 0xF000) == 0x4000)
                .unwrap_or_else(|| meta.is_dir());

            if child_is_dir {
                stack.push((child_path, false));
            } else {
                files_to_delete.push(child_path);
            }
        }

        // Pipeline file deletions in concurrent batches of 8 over the multiplexed SFTP session
        for chunk in files_to_delete.chunks(8) {
            let mut join_set = tokio::task::JoinSet::new();
            for file_path in chunk {
                let file_path = file_path.clone();
                let sftp_clone = Arc::clone(sftp);
                join_set.spawn(async move {
                    sftp_clone
                        .remove_file(&file_path)
                        .await
                        .with_context(|| format!("Failed to delete file '{}'", file_path))
                });
            }
            while let Some(joined) = join_set.join_next().await {
                joined.context("File deletion task panicked")??;
            }
        }
    }

    Ok(())
}
