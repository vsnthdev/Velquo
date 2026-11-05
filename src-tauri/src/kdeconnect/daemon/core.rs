use super::*;
use crate::kdeconnect::cert::{sha256_fingerprint, HostIdentity};
use crate::kdeconnect::clipboard::get_macos_pasteboard_change_count;
use crate::kdeconnect::sftp::SftpManager;
use crate::kdeconnect::tls::{build_tls_acceptor, build_tls_connector};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::net::UdpSocket;
use tokio::sync::Semaphore;
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub(crate) const DISCOVERY_BUCKET_CAPACITY: f64 = 8.0;
pub(crate) const DISCOVERY_BUCKET_REFILL_PER_SEC: f64 = 2.0;
pub(crate) const MAX_CONCURRENT_DISCOVERY_CONNECTIONS: usize = 8;
pub(crate) const DISCOVERY_ATTEMPT_RETRY_MS: i64 = 1_500;
pub(crate) const DISCOVERY_ATTEMPT_TTL_MS: i64 = 60_000;

#[derive(Debug)]
pub(crate) struct DiscoveryRateLimiter {
    tokens: f64,
    last_refill: Instant,
}

impl DiscoveryRateLimiter {
    pub(crate) fn new() -> Self {
        Self {
            tokens: DISCOVERY_BUCKET_CAPACITY,
            last_refill: Instant::now(),
        }
    }

    pub(crate) fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        self.tokens = (self.tokens + elapsed * DISCOVERY_BUCKET_REFILL_PER_SEC)
            .min(DISCOVERY_BUCKET_CAPACITY);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

pub struct KdeConnectDaemon {
    pub host: HostIdentity,
    pub config_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub log_file_path: PathBuf,
    pub tcp_port: Mutex<u16>,
    pub tls_acceptor: TlsAcceptor,
    pub tls_connector: TlsConnector,
    pub devices: Mutex<HashMap<String, DeviceEntry>>,
    pub trusted_devices: Mutex<HashMap<String, TrustedDeviceRecord>>,
    pub transfers: Mutex<Vec<TransferItem>>,
    pub last_discovery_attempt_ms: Mutex<HashMap<String, i64>>,
    pub(crate) discovery_rate_limiter: Mutex<DiscoveryRateLimiter>,
    pub discovery_connection_semaphore: Arc<Semaphore>,
    pub udp_socket: Mutex<Option<Arc<UdpSocket>>>,
    pub app_handle: Mutex<Option<AppHandle>>,
    pub call_media_state: Mutex<CallMediaState>,
    pub clipboard_state: Mutex<ClipboardState>,
    pub recently_copied_otps: Mutex<HashMap<(String, String), i64>>,
    pub app_settings: Mutex<AppSettings>,
    pub sftp_manager: SftpManager,
}

impl KdeConnectDaemon {
    pub fn new(
        config_dir: PathBuf,
        downloads_dir: PathBuf,
        log_file_path: PathBuf,
    ) -> Result<Arc<Self>> {
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&downloads_dir);

        let host = HostIdentity::load_or_create(&config_dir)?;
        let tls_acceptor = build_tls_acceptor(&host)?;
        let tls_connector = build_tls_connector(&host)?;

        let trusted_map = Self::load_trusted_devices(&config_dir);
        let app_settings = Self::load_app_settings(&config_dir);
        let default_downloads_str = downloads_dir.to_string_lossy().to_string();
        let mut initial_devices = HashMap::new();
        for (id, rec) in &trusted_map {
            let fp = sha256_fingerprint(&rec.cert_der);
            let device_save_loc = rec
                .save_location
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| default_downloads_str.clone());
            initial_devices.insert(
                id.clone(),
                DeviceEntry {
                    view: DeviceView {
                        device_id: rec.device_id.clone(),
                        device_name: rec.device_name.clone(),
                        device_type: rec.device_type.clone(),
                        protocol_version: rec.protocol_version,
                        address: "Offline".to_string(),
                        tcp_port: 1716,
                        reachable: false,
                        pair_state: PairState::Paired,
                        verification_key: None,
                        cert_fingerprint: Some(fp),
                        incoming_capabilities: Vec::new(),
                        outgoing_capabilities: Vec::new(),
                        last_seen_secs: 0,
                        battery_charge: None,
                        battery_charging: None,
                        cellular_network_type: None,
                        cellular_signal_strength: None,
                        sim_signals: Vec::new(),
                        storage_free_gb: None,
                        now_playing: None,
                        sync_clipboard: rec.sync_clipboard,
                        allow_presenter: rec.allow_presenter,
                        sync_notifications: rec.sync_notifications,
                        auto_copy_otp: rec.auto_copy_otp,
                        pause_media_on_calls: rec.pause_media_on_calls,
                        save_location: device_save_loc,
                        notifications: Vec::new(),
                        run_commands: rec.run_commands.clone(),
                    },
                    peer_cert_der: Some(rec.cert_der.clone()),
                    sender: None,
                    abort_tx: None,
                    connection_generation: 0,
                    mpris_players: HashMap::new(),
                    mpris_player_order: Vec::new(),
                    active_mpris_player: None,
                    album_art_cache: HashMap::new(),
                    album_art_in_flight: HashMap::new(),
                    paired: Arc::new(AtomicBool::new(true)),
                },
            );
        }

        Ok(Arc::new(Self {
            host,
            config_dir,
            downloads_dir,
            log_file_path,
            tcp_port: Mutex::new(MIN_TCP_PORT),
            tls_acceptor,
            tls_connector,
            devices: Mutex::new(initial_devices),
            trusted_devices: Mutex::new(trusted_map),
            transfers: Mutex::new(Vec::new()),
            last_discovery_attempt_ms: Mutex::new(HashMap::new()),
            discovery_rate_limiter: Mutex::new(DiscoveryRateLimiter::new()),
            discovery_connection_semaphore: Arc::new(Semaphore::new(
                MAX_CONCURRENT_DISCOVERY_CONNECTIONS,
            )),
            udp_socket: Mutex::new(None),
            app_handle: Mutex::new(None),
            call_media_state: Mutex::new(CallMediaState::default()),
            clipboard_state: Mutex::new(ClipboardState {
                last_change_count: get_macos_pasteboard_change_count(),
                last_content: String::new(),
                last_timestamp_ms: 0,
            }),
            recently_copied_otps: Mutex::new(HashMap::new()),
            app_settings: Mutex::new(app_settings),
            sftp_manager: SftpManager::new(),
        }))
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        if let Ok(mut guard) = self.app_handle.lock() {
            *guard = Some(handle);
        }
    }

    /// Displays a native macOS system notification with the Velquo icon and click-to-focus support.
    pub fn show_macos_notification(
        title: &str,
        body: &str,
        device_id: Option<&str>,
        notification_id: Option<&str>,
        dismissable: bool,
    ) {
        #[cfg(target_os = "macos")]
        {
            crate::macos_notify::show_macos_notification(
                title,
                None,
                body,
                device_id,
                notification_id,
                dismissable,
            );
        }
    }

    pub fn remove_macos_notifications(device_id: &str, notification_ids: &[String]) {
        #[cfg(target_os = "macos")]
        {
            crate::macos_notify::remove_macos_notifications(device_id, notification_ids);
        }
    }

    pub fn notify_ui(&self) {
        if let Ok(guard) = self.app_handle.lock() {
            if let Some(app) = guard.as_ref() {
                let _ = app.emit("velquo://state-updated", ());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_bucket_allows_startup_burst_then_blocks() {
        let mut limiter = DiscoveryRateLimiter::new();
        for _ in 0..DISCOVERY_BUCKET_CAPACITY as usize {
            assert!(limiter.try_consume());
        }
        assert!(!limiter.try_consume());
    }
}
