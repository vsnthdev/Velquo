use crate::kdeconnect::cert::{
    compute_verification_key, extract_cert_common_name, sha256_fingerprint, HostIdentity,
};
use crate::kdeconnect::clipboard::{
    get_macos_pasteboard_change_count, read_macos_clipboard_text, write_macos_clipboard_text,
};
use crate::kdeconnect::macos_media::{
    get_cached_mac_album_art, get_mac_player_list, handle_incoming_mpris_request,
    last_broadcast_sig_store, pause_mac_media_for_call, query_mac_player_status,
    resume_mac_media_after_call, DEFAULT_MAC_PLAYER,
};
use crate::kdeconnect::packet::{
    current_millis, current_secs, NetworkPacket, PACKET_TYPE_BATTERY, PACKET_TYPE_BATTERY_REQUEST,
    PACKET_TYPE_CLIPBOARD, PACKET_TYPE_CLIPBOARD_CONNECT, PACKET_TYPE_CONNECTIVITY_REPORT,
    PACKET_TYPE_FINDMYPHONE_REQUEST, PACKET_TYPE_IDENTITY, PACKET_TYPE_MOUSEPAD_REQUEST,
    PACKET_TYPE_MPRIS, PACKET_TYPE_MPRIS_REQUEST, PACKET_TYPE_NOTIFICATION, PACKET_TYPE_PAIR,
    PACKET_TYPE_PING, PACKET_TYPE_PRESENTER, PACKET_TYPE_RUNCOMMAND_OUTPUT,
    PACKET_TYPE_RUNCOMMAND_REQUEST, PACKET_TYPE_SFTP, PACKET_TYPE_SHARE_REQUEST,
    PACKET_TYPE_SHARE_REQUEST_UPDATE, PACKET_TYPE_TELEPHONY, PROTOCOL_VERSION,
};
use crate::kdeconnect::presenter_input::{handle_mousepad_packet, handle_presenter_packet};
use crate::kdeconnect::sftp::{SftpBrowseResponse, SftpManager};
use crate::kdeconnect::tls::{build_tls_acceptor, build_tls_connector};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, SockRef, Socket, TcpKeepalive, Type};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, oneshot};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use uuid::Uuid;

pub const UDP_PORT: u16 = 1716;
pub const MIN_TCP_PORT: u16 = 1716;
pub const MAX_TCP_PORT: u16 = 1764;
pub const MIN_PAYLOAD_PORT: u16 = 1739;
pub const MAX_PAYLOAD_PORT: u16 = 1764;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairState {
    NotPaired,
    Requested,
    RequestedByPeer,
    Paired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub protocol_version: i32,
    pub tcp_port: u16,
    pub udp_port: u16,
    pub cert_fingerprint: String,
    pub downloads_dir: String,
    pub log_file_path: String,
    pub local_ips: Vec<String>,
    pub ignore_whatsapp_conversations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingInfo {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub position_ms: i64,
    pub position_updated_at_ms: i64,
    pub length_ms: i64,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_seek: bool,
    pub album_art_url: Option<String>,
    pub album_art_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimSignalInfo {
    pub slot_id: String,
    pub network_type: String,
    pub signal_strength: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneNotification {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub text: String,
    pub ticker: String,
    pub is_clearable: bool,
    pub timestamp_ms: i64,
    pub otp_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCommandItem {
    pub key: String,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub protocol_version: i32,
    pub address: String,
    pub tcp_port: u16,
    pub reachable: bool,
    pub pair_state: PairState,
    pub verification_key: Option<String>,
    pub cert_fingerprint: Option<String>,
    pub incoming_capabilities: Vec<String>,
    pub outgoing_capabilities: Vec<String>,
    pub last_seen_secs: i64,
    pub battery_charge: Option<i32>,
    pub battery_charging: Option<bool>,
    pub cellular_network_type: Option<String>,
    pub cellular_signal_strength: Option<i32>,
    pub sim_signals: Vec<SimSignalInfo>,
    pub storage_free_gb: Option<f64>,
    pub now_playing: Option<NowPlayingInfo>,
    pub sync_clipboard: bool,
    pub allow_presenter: bool,
    pub sync_notifications: bool,
    pub auto_copy_otp: bool,
    pub is_ringing: bool,
    pub save_location: String,
    pub notifications: Vec<PhoneNotification>,
    pub run_commands: Vec<RunCommandItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub direction: String, // "Incoming" | "Outgoing"
    pub kind: String,      // "File" | "Text" | "Url"
    pub filename: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: String, // "InProgress" | "Completed" | "Failed"
    pub saved_path: Option<String>,
    pub text_content: Option<String>,
    pub url_content: Option<String>,
    pub error: Option<String>,
    pub timestamp_ms: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDeviceRecord {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub protocol_version: i32,
    pub cert_der: Vec<u8>,
    #[serde(default = "default_true")]
    pub sync_clipboard: bool,
    #[serde(default = "default_true")]
    pub allow_presenter: bool,
    #[serde(default = "default_true")]
    pub sync_notifications: bool,
    #[serde(default = "default_true")]
    pub auto_copy_otp: bool,
    #[serde(default)]
    pub save_location: Option<String>,
    #[serde(default)]
    pub run_commands: Vec<RunCommandItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_true")]
    pub ignore_whatsapp_conversations: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ignore_whatsapp_conversations: true,
        }
    }
}

pub struct ClipboardState {
    pub last_change_count: isize,
    pub last_content: String,
    pub last_timestamp_ms: i64,
}

pub struct DeviceEntry {
    pub view: DeviceView,
    pub peer_cert_der: Option<Vec<u8>>,
    pub sender: Option<mpsc::UnboundedSender<Vec<u8>>>,
    pub abort_tx: Option<oneshot::Sender<()>>,
    pub connection_generation: u64,
    pub mpris_players: HashMap<String, NowPlayingInfo>,
    pub mpris_player_order: Vec<String>,
    pub active_mpris_player: Option<String>,
    pub album_art_cache: HashMap<String, String>,
    pub album_art_in_flight: HashSet<String>,
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
    pub udp_socket: Mutex<Option<Arc<UdpSocket>>>,
    pub app_handle: Mutex<Option<AppHandle>>,
    pub active_call_events: Mutex<HashSet<String>>,
    pub call_paused_sources: Mutex<HashSet<String>>,
    pub call_state_seq: Mutex<u64>,
    pub clipboard_state: Mutex<ClipboardState>,
    pub app_settings: Mutex<AppSettings>,
    pub sftp_manager: SftpManager,
}

impl KdeConnectDaemon {
    pub fn new(config_dir: PathBuf, downloads_dir: PathBuf, log_file_path: PathBuf) -> Result<Arc<Self>> {
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
                        is_ringing: false,
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
                    album_art_in_flight: HashSet::new(),
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
            udp_socket: Mutex::new(None),
            app_handle: Mutex::new(None),
            active_call_events: Mutex::new(HashSet::new()),
            call_paused_sources: Mutex::new(HashSet::new()),
            call_state_seq: Mutex::new(0),
            clipboard_state: Mutex::new(ClipboardState {
                last_change_count: get_macos_pasteboard_change_count(),
                last_content: String::new(),
                last_timestamp_ms: 0,
            }),
            app_settings: Mutex::new(app_settings),
            sftp_manager: SftpManager::new(),
        }))
    }

    fn trusted_devices_file(config_dir: &Path) -> PathBuf {
        config_dir.join("trusted_devices.json")
    }

    fn load_trusted_devices(config_dir: &Path) -> HashMap<String, TrustedDeviceRecord> {
        let path = Self::trusted_devices_file(config_dir);
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, TrustedDeviceRecord>>(&data) {
                log::info!("[config] Loaded {} trusted device(s) from {:?}", map.len(), path);
                return map;
            }
        }
        HashMap::new()
    }

    fn save_trusted_devices(&self) {
        let path = Self::trusted_devices_file(&self.config_dir);
        if let Ok(guard) = self.trusted_devices.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                if let Err(e) = fs::write(&path, json) {
                    log::error!("[config] Failed to save trusted devices: {}", e);
                } else {
                    log::info!("[config] Saved {} trusted device(s) to {:?}", guard.len(), path);
                }
            }
        }
        let trusted_list = self.get_trusted_devices_menu_list();
        if let Ok(guard) = self.app_handle.lock() {
            if let Some(app) = guard.as_ref() {
                crate::refresh_tray_menu(app, trusted_list);
            }
        }
    }

    pub fn get_trusted_devices_menu_list(&self) -> Vec<(String, String, String)> {
        let mut list: Vec<(String, String, String)> = self
            .trusted_devices
            .lock()
            .map(|guard| {
                guard
                    .values()
                    .map(|rec| {
                        (
                            rec.device_id.clone(),
                            rec.device_name.clone(),
                            rec.device_type.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        list.sort_by(|a, b| a.1.cmp(&b.1));
        list
    }

    fn app_settings_file(config_dir: &Path) -> PathBuf {
        config_dir.join("app_settings.json")
    }

    fn load_app_settings(config_dir: &Path) -> AppSettings {
        let path = Self::app_settings_file(config_dir);
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&data) {
                return settings;
            }
        }
        AppSettings::default()
    }

    fn save_app_settings(&self) {
        let path = Self::app_settings_file(&self.config_dir);
        if let Ok(guard) = self.app_settings.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                let _ = fs::write(&path, json);
            }
        }
    }

    pub fn is_ignore_whatsapp_conversations_enabled(&self) -> bool {
        self.app_settings
            .lock()
            .map(|s| s.ignore_whatsapp_conversations)
            .unwrap_or(true)
    }

    pub fn set_ignore_whatsapp_conversations(&self, enabled: bool) -> Result<()> {
        {
            let mut settings = self.app_settings.lock().unwrap();
            settings.ignore_whatsapp_conversations = enabled;
        }
        self.save_app_settings();

        if enabled {
            if let Ok(mut devices) = self.devices.lock() {
                for entry in devices.values_mut() {
                    entry.view.notifications.retain(|n| {
                        let is_wa = n.app_name.eq_ignore_ascii_case("whatsapp")
                            || n.app_name.eq_ignore_ascii_case("whatsapp business")
                            || n.id.contains("com.whatsapp");
                        !(is_wa && n.otp_code.is_none())
                    });
                }
            }
        }

        self.notify_ui();
        Ok(())
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        if let Ok(mut guard) = self.app_handle.lock() {
            *guard = Some(handle);
        }
    }

    /// Displays a native macOS system notification with the Velquo icon, click-to-focus support,
    /// and an optional native "Dismiss" action button.
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

    pub fn notify_ui(&self) {
        if let Ok(guard) = self.app_handle.lock() {
            if let Some(app) = guard.as_ref() {
                let _ = app.emit("velquo://state-updated", ());
            }
        }
    }

    pub fn get_local_ipv4s() -> Vec<(Ipv4Addr, Ipv4Addr)> {
        let mut list = Vec::new();
        if let Ok(interfaces) = if_addrs::get_if_addrs() {
            for iface in interfaces {
                if iface.is_loopback() {
                    continue;
                }
                if let if_addrs::IfAddr::V4(v4) = iface.addr {
                    let ip = v4.ip;
                    let netmask = v4.netmask;
                    let broadcast = v4
                        .broadcast
                        .unwrap_or_else(|| Ipv4Addr::from(u32::from(ip) | !u32::from(netmask)));
                    list.push((ip, broadcast));
                }
            }
        }
        list
    }

    pub fn get_host_info(&self) -> HostInfo {
        let tcp_port = *self.tcp_port.lock().unwrap();
        let local_ips = Self::get_local_ipv4s()
            .into_iter()
            .map(|(ip, _)| ip.to_string())
            .collect();
        HostInfo {
            device_id: self.host.device_id.clone(),
            device_name: self.host.device_name.clone(),
            device_type: self.host.device_type.clone(),
            protocol_version: PROTOCOL_VERSION,
            tcp_port,
            udp_port: UDP_PORT,
            cert_fingerprint: self.host.cert_fingerprint.clone(),
            downloads_dir: self.downloads_dir.to_string_lossy().to_string(),
            log_file_path: self.log_file_path.to_string_lossy().to_string(),
            local_ips,
            ignore_whatsapp_conversations: self.is_ignore_whatsapp_conversations_enabled(),
        }
    }

    pub fn get_devices_list(&self) -> Vec<DeviceView> {
        let guard = self.devices.lock().unwrap();
        let mut list: Vec<DeviceView> = guard.values().map(|e| e.view.clone()).collect();
        list.sort_by(|a, b| {
            b.reachable
                .cmp(&a.reachable)
                .then_with(|| a.device_name.cmp(&b.device_name))
        });
        list
    }

    pub fn get_transfers_list(&self) -> Vec<TransferItem> {
        let guard = self.transfers.lock().unwrap();
        let mut list = guard.clone();
        list.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
        list
    }

    /// Starts TCP server, UDP discovery listener + periodic broadcaster, and mDNS service
    pub fn start(self: &Arc<Self>) {
        let daemon = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = daemon.run_network_services().await {
                log::error!("[daemon] Fatal error starting network services: {:?}", e);
            }
        });
    }

    async fn run_network_services(self: Arc<Self>) -> Result<()> {
        // 1. Bind TCP Listener in range 1716..=1764
        let mut bound_listener = None;
        let mut chosen_port = MIN_TCP_PORT;
        for port in MIN_TCP_PORT..=MAX_TCP_PORT {
            match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await {
                Ok(listener) => {
                    chosen_port = port;
                    bound_listener = Some(listener);
                    log::info!("[tcp-server] Listening for incoming KDE Connect TCP connections on 0.0.0.0:{}", port);
                    break;
                }
                Err(e) => {
                    log::warn!("[tcp-server] Port {} unavailable ({}), trying next...", port, e);
                }
            }
        }

        let tcp_listener = bound_listener.ok_or_else(|| {
            anyhow!(
                "Could not bind any TCP port in range {}..={}",
                MIN_TCP_PORT,
                MAX_TCP_PORT
            )
        })?;
        *self.tcp_port.lock().unwrap() = chosen_port;
        self.notify_ui();

        // 2. Bind UDP Discovery Socket on 0.0.0.0:1716 using socket2 with SO_REUSEADDR + SO_REUSEPORT + SO_BROADCAST
        let udp_sock = Self::bind_udp_discovery_socket()?;
        let udp_arc = Arc::new(udp_sock);
        *self.udp_socket.lock().unwrap() = Some(Arc::clone(&udp_arc));
        log::info!(
            "[udp] Bound UDP socket on 0.0.0.0:{} (broadcast=true, reuse_addr=true, reuse_port=true)",
            UDP_PORT
        );

        // 3. Spawn TCP accept loop
        let daemon_tcp = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                match tcp_listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        log::info!("[tcp-in] Accepted new TCP connection from {}", peer_addr);
                        let d = Arc::clone(&daemon_tcp);
                        tokio::spawn(async move {
                            if let Err(e) = d.handle_inbound_tcp(stream, peer_addr).await {
                                log::warn!("[tcp-in] Connection from {} closed/failed: {:?}", peer_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("[tcp-in] TCP accept error: {}", e);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        });

        // 4. Spawn UDP receive loop
        let daemon_udp_rx = Arc::clone(&self);
        let udp_rx = Arc::clone(&udp_arc);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 16384];
            loop {
                match udp_rx.recv_from(&mut buf).await {
                    Ok((len, sender_addr)) => {
                        let data = &buf[..len];
                        daemon_udp_rx.handle_udp_datagram(data, sender_addr);
                    }
                    Err(e) => {
                        log::warn!("[udp] recv_from error: {}", e);
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                }
            }
        });

        // 5. Start mDNS (_kdeconnect._udp.local.) announcer and browser
        let daemon_mdns = Arc::clone(&self);
        tokio::task::spawn_blocking(move || {
            daemon_mdns.run_mdns_service(chosen_port);
        });

        // 6. Initial UDP discovery broadcast + smart network/offline monitor
        // Avoids broadcasting every 12s when devices are already connected (which causes phones to tear down and reconnect)
        self.broadcast_udp_discovery().await;
        let daemon_bcast = Arc::clone(&self);
        tokio::spawn(async move {
            // One follow-up broadcast 5s after startup once ARP/mDNS settles
            tokio::time::sleep(Duration::from_secs(5)).await;
            if daemon_bcast.should_auto_broadcast() {
                daemon_bcast.broadcast_udp_discovery().await;
            }

            let mut last_ips: Vec<Ipv4Addr> = Self::get_local_ipv4s()
                .into_iter()
                .map(|(ip, _)| ip)
                .collect();
            let mut last_host_battery: Option<(i32, bool)> = Self::read_macos_battery_status();
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await; // consume immediate tick
            let mut tick_count: u64 = 0;

            loop {
                interval.tick().await;
                tick_count = tick_count.wrapping_add(1);

                // Every 30s (6 ticks), check if MacBook battery % or charging state changed
                if tick_count % 6 == 0 {
                    if let Some(current_bat) = Self::read_macos_battery_status() {
                        if last_host_battery != Some(current_bat) {
                            log::info!(
                                "[battery-out] Mac battery changed ({:?} -> {:?}), broadcasting to paired devices",
                                last_host_battery,
                                current_bat
                            );
                            last_host_battery = Some(current_bat);
                            daemon_bcast.broadcast_host_battery(current_bat.0, current_bat.1);
                        }
                    }
                }

                let current_ips: Vec<Ipv4Addr> = Self::get_local_ipv4s()
                    .into_iter()
                    .map(|(ip, _)| ip)
                    .collect();

                if current_ips != last_ips {
                    log::info!(
                        "[network] Local IPv4 addresses changed ({:?} -> {:?}), broadcasting discovery",
                        last_ips,
                        current_ips
                    );
                    last_ips = current_ips;
                    daemon_bcast.notify_ui();
                    daemon_bcast.broadcast_udp_discovery().await;
                } else if daemon_bcast.should_auto_broadcast() {
                    log::debug!(
                        "[udp] Auto-broadcasting discovery (waiting for offline/unconnected devices)"
                    );
                    daemon_bcast.broadcast_udp_discovery().await;
                }
            }
        });

        // 7. Spawn 500ms macOS Clipboard (`NSPasteboard.changeCount`) monitor
        let daemon_clip = Arc::clone(&self);
        tokio::spawn(async move {
            let mut clip_interval = tokio::time::interval(Duration::from_millis(500));
            clip_interval.tick().await;
            loop {
                clip_interval.tick().await;
                let current_count = get_macos_pasteboard_change_count();
                let count_changed = {
                    let state = daemon_clip.clipboard_state.lock().unwrap();
                    current_count != state.last_change_count
                };
                if !count_changed {
                    continue;
                }

                // Update last_change_count immediately
                {
                    let mut state = daemon_clip.clipboard_state.lock().unwrap();
                    state.last_change_count = current_count;
                }

                // Only read pasteboard text if at least one online paired device has clipboard sync enabled
                if !daemon_clip.has_online_clipboard_sync_device() {
                    continue;
                }

                if let Some(text) = read_macos_clipboard_text() {
                    let should_broadcast = {
                        let mut state = daemon_clip.clipboard_state.lock().unwrap();
                        if state.last_content == text {
                            false
                        } else {
                            state.last_content = text.clone();
                            state.last_timestamp_ms = current_millis();
                            true
                        }
                    };

                    if should_broadcast {
                        log::info!(
                            "[clipboard-out] Mac clipboard changed ({} chars), syncing to paired devices",
                            text.len()
                        );
                        let pkt = NetworkPacket::clipboard(&text);
                        daemon_clip.broadcast_clipboard_to_synced_devices(&pkt);
                    }
                }
            }
        });

        // 5. Proactive macOS Control Center Now Playing broadcaster to paired Android devices
        let daemon_mpris = Arc::clone(&self);
        tokio::spawn(async move {
            let mut mpris_interval = tokio::time::interval(Duration::from_millis(2000));
            mpris_interval.tick().await;

            loop {
                mpris_interval.tick().await;

                let online_paired_ids: Vec<String> = {
                    let devices = daemon_mpris.devices.lock().unwrap();
                    devices
                        .values()
                        .filter(|d| d.view.reachable && d.view.pair_state == PairState::Paired)
                        .map(|d| d.view.device_id.clone())
                        .collect()
                };

                if online_paired_ids.is_empty() {
                    if let Ok(mut sig_guard) = last_broadcast_sig_store().lock() {
                        *sig_guard = None;
                    }
                    continue;
                }

                let Ok((players, snap_opt)) = tokio::task::spawn_blocking(|| {
                    let p = get_mac_player_list();
                    let s = query_mac_player_status("");
                    (p, s)
                })
                .await
                else {
                    continue;
                };

                let current_sig = match &snap_opt {
                    Some(snap) => (
                        snap.player.clone(),
                        snap.title.clone(),
                        snap.artist.clone(),
                        snap.album.clone(),
                        snap.album_art_url.clone(),
                        snap.is_playing,
                        snap.length_ms,
                        players.clone(),
                    ),
                    None => (
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        false,
                        0,
                        players.clone(),
                    ),
                };

                let (list_changed, meta_changed) = {
                    let mut sig_guard = last_broadcast_sig_store().lock().unwrap();
                    let lc = sig_guard
                        .as_ref()
                        .map(|prev| prev.7 != current_sig.7)
                        .unwrap_or(true);
                    let mc = sig_guard.as_ref() != Some(&current_sig);
                    if mc {
                        *sig_guard = Some(current_sig);
                    }
                    (lc, mc)
                };

                if !meta_changed {
                    continue;
                }

                if list_changed {
                    let mut body = serde_json::Map::new();
                    body.insert("playerList".into(), serde_json::json!(players));
                    body.insert("supportAlbumArtPayload".into(), serde_json::json!(true));
                    let list_pkt = NetworkPacket::new(PACKET_TYPE_MPRIS, body);
                    for id in &online_paired_ids {
                        let _ = daemon_mpris.send_packet_to_device(id, &list_pkt);
                    }
                }

                if let Some(snap) = snap_opt {
                    log::info!(
                        "[mpris-host] Broadcasting Mac Control Center Now Playing to paired devices: player='{}' title='{}' artist='{}' isPlaying={} art='{}'",
                        snap.player,
                        snap.title,
                        snap.artist,
                        snap.is_playing,
                        snap.album_art_url
                    );
                    let pkt = snap.to_mpris_packet(true, true);
                    for id in &online_paired_ids {
                        let _ = daemon_mpris.send_packet_to_device(id, &pkt);
                    }
                }
            }
        });

        Ok(())
    }

    /// Returns true ONLY if at least one already-paired (trusted) device is currently offline.
    /// Searching for new unpaired devices only happens when the user opens the "Pair new device" page.
    fn should_auto_broadcast(&self) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .values()
            .any(|d| d.view.pair_state == PairState::Paired && !d.view.reachable)
    }

    /// Configures TCP_NODELAY, aggressive TCP Keepalive (2s idle, 1s interval, 2 retries),
    /// and macOS `TCP_RXT_CONNDROPTIME` (3s) so half-open Wi-Fi drops are detected rapidly.
    fn configure_tcp_socket(stream: &TcpStream) {
        let _ = stream.set_nodelay(true);
        let sock_ref = SockRef::from(stream);
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(2))
            .with_interval(Duration::from_secs(1))
            .with_retries(2);
        if let Err(e) = sock_ref.set_tcp_keepalive(&keepalive) {
            log::debug!("[tcp] Failed to set TCP keepalive on socket: {}", e);
        }

        #[cfg(target_os = "macos")]
        {
            const TCP_RXT_CONNDROPTIME: libc::c_int = 0x80;
            const TCP_RXT_FINDROP: libc::c_int = 0x100;
            let fd = stream.as_raw_fd();
            let drop_secs: libc::c_int = 3;
            let fin_drop: libc::c_int = 1;
            unsafe {
                let _ = libc::setsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    TCP_RXT_CONNDROPTIME,
                    &drop_secs as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
                let _ = libc::setsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    TCP_RXT_FINDROP,
                    &fin_drop as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
            }
        }
    }

    /// Returns the number of unacknowledged bytes sitting in the macOS kernel TCP send buffer (`SO_NWRITE`).
    /// When the remote peer is online, written bytes are ACKed in milliseconds (`0`).
    /// When the peer turns off Wi-Fi without sending FIN/RST, written bytes stay `> 0`.
    fn get_socket_unacked_bytes(raw_fd: RawFd) -> usize {
        #[cfg(target_os = "macos")]
        {
            let mut nwrite: libc::c_int = 0;
            let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            let res = unsafe {
                libc::getsockopt(
                    raw_fd,
                    libc::SOL_SOCKET,
                    libc::SO_NWRITE,
                    &mut nwrite as *mut _ as *mut libc::c_void,
                    &mut len,
                )
            };
            if res == 0 && nwrite > 0 {
                return nwrite as usize;
            }
            0
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = raw_fd;
            0
        }
    }

    fn bind_udp_discovery_socket() -> Result<UdpSocket> {
        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create IPv4 UDP socket")?;
        let _ = sock.set_reuse_address(true);
        let _ = sock.set_reuse_port(true);
        let _ = sock.set_broadcast(true);
        sock.set_nonblocking(true)?;

        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, UDP_PORT);
        sock.bind(&addr.into())
            .with_context(|| format!("Failed to bind UDP port {}", UDP_PORT))?;

        let std_sock: std::net::UdpSocket = sock.into();
        let tokio_sock = UdpSocket::from_std(std_sock)?;
        Ok(tokio_sock)
    }

    fn run_mdns_service(self: Arc<Self>, tcp_port: u16) {
        log::info!("[mdns] Starting mDNS daemon for _kdeconnect._udp.local.");
        let mdns = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("[mdns] Could not initialize mDNS daemon: {}", e);
                return;
            }
        };

        let service_type = "_kdeconnect._udp.local.";
        let instance_name = self.host.device_id.clone();
        let host_name = format!("{}.local.", self.host.device_id);
        let local_ips: Vec<IpAddr> = Self::get_local_ipv4s()
            .into_iter()
            .map(|(ip, _)| IpAddr::V4(ip))
            .collect();

        let properties = [
            ("id", self.host.device_id.as_str()),
            ("name", self.host.device_name.as_str()),
            ("type", self.host.device_type.as_str()),
            ("protocol", "8"),
        ];

        match ServiceInfo::new(
            service_type,
            &instance_name,
            &host_name,
            &local_ips[..],
            tcp_port,
            &properties[..],
        ) {
            Ok(service_info) => {
                if let Err(e) = mdns.register(service_info) {
                    log::warn!("[mdns] Failed to register mDNS service: {}", e);
                } else {
                    log::info!(
                        "[mdns] Registered mDNS service {} on port {} with IPs {:?}",
                        instance_name,
                        tcp_port,
                        local_ips
                    );
                }
            }
            Err(e) => {
                log::warn!("[mdns] Failed to build ServiceInfo: {}", e);
            }
        }

        match mdns.browse(service_type) {
            Ok(receiver) => {
                log::info!("[mdns] Browsing for _kdeconnect._udp.local. services...");
                while let Ok(event) = receiver.recv() {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let txt_id = info
                                .get_property_val_str("id")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| {
                                    info.get_fullname()
                                        .split('.')
                                        .next()
                                        .unwrap_or("")
                                        .to_string()
                                });

                            if txt_id.is_empty() || txt_id == self.host.device_id {
                                continue;
                            }

                            let proto = info
                                .get_property_val_str("protocol")
                                .and_then(|p| p.parse::<i32>().ok())
                                .unwrap_or(PROTOCOL_VERSION);
                            let port = info.get_port();

                            for ip in info.get_addresses_v4() {
                                let ip_val = *ip;
                                log::info!(
                                    "[mdns] Discovered peer via mDNS: id='{}' ip={} port={} proto={}",
                                    txt_id,
                                    ip_val,
                                    port,
                                    proto
                                );
                                let d = Arc::clone(&self);
                                let id_clone = txt_id.clone();
                                tauri::async_runtime::spawn(async move {
                                    d.on_device_discovered(
                                        IpAddr::V4(ip_val),
                                        port,
                                        id_clone,
                                        proto,
                                        None,
                                    )
                                    .await;
                                });
                            }
                        }
                        ServiceEvent::ServiceRemoved(_ty, fullname) => {
                            let removed_id = fullname.split('.').next().unwrap_or("").to_string();
                            if !removed_id.is_empty() && removed_id != self.host.device_id {
                                let mut aborted = false;
                                if let Ok(mut devices) = self.devices.lock() {
                                    if let Some(entry) = devices.get_mut(&removed_id) {
                                        if entry.view.reachable {
                                            log::info!(
                                                "[mdns] Peer '{}' ({}) unregistered mDNS service, closing active session",
                                                entry.view.device_name,
                                                removed_id
                                            );
                                            if let Some(abort_tx) = entry.abort_tx.take() {
                                                let _ = abort_tx.send(());
                                                aborted = true;
                                            }
                                        }
                                    }
                                }
                                if aborted {
                                    self.notify_ui();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                log::warn!("[mdns] Failed to browse mDNS: {}", e);
            }
        }
    }

    pub async fn broadcast_udp_discovery(&self) {
        let tcp_port = *self.tcp_port.lock().unwrap();
        let packet = NetworkPacket::udp_discovery(&self.host, tcp_port);
        let bytes = match packet.serialize_line() {
            Ok(b) => b,
            Err(e) => {
                log::error!("[udp] Failed to serialize UDP discovery packet: {}", e);
                return;
            }
        };

        let interfaces = Self::get_local_ipv4s();
        log::debug!(
            "[udp] Broadcasting UDP discovery packet ({} bytes) across {} interface(s)",
            bytes.len(),
            interfaces.len()
        );

        // Broadcast once from each local interface IP to its subnet broadcast address
        let mut sent_any = false;
        for (source_ip, subnet_bcast) in &interfaces {
            if let Ok(sock) = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
                let _ = sock.set_broadcast(true);
                let bind_addr = SocketAddrV4::new(*source_ip, 0);
                if sock.bind(&bind_addr.into()).is_ok() {
                    let std_sock: std::net::UdpSocket = sock.into();
                    let dest_subnet = SocketAddrV4::new(*subnet_bcast, UDP_PORT);
                    if std_sock.send_to(&bytes, dest_subnet).is_ok() {
                        sent_any = true;
                        log::debug!(
                            "[udp] Sent discovery broadcast from {} -> {}",
                            source_ip,
                            dest_subnet
                        );
                    }
                }
            }
        }

        // Fallback to global broadcast only if no subnet broadcast succeeded
        if !sent_any {
            let maybe_sock = self.udp_socket.lock().unwrap().clone();
            if let Some(sock) = maybe_sock {
                let _ = sock
                    .send_to(&bytes, SocketAddrV4::new(Ipv4Addr::BROADCAST, UDP_PORT))
                    .await;
            }
        }
    }

    pub async fn probe_ip(&self, ip_str: &str) -> Result<()> {
        let ip: IpAddr = ip_str
            .parse()
            .with_context(|| format!("Invalid IP address: {}", ip_str))?;
        let tcp_port = *self.tcp_port.lock().unwrap();
        let packet = NetworkPacket::udp_discovery(&self.host, tcp_port);
        let bytes = packet.serialize_line()?;

        let dest = SocketAddr::new(ip, UDP_PORT);
        log::info!("[udp] Sending directed UDP discovery probe to {}", dest);
        let maybe_sock = self.udp_socket.lock().unwrap().clone();
        if let Some(sock) = maybe_sock {
            sock.send_to(&bytes, dest).await?;
        }
        Ok(())
    }

    fn handle_udp_datagram(self: &Arc<Self>, data: &[u8], sender_addr: SocketAddr) {
        let ip = match sender_addr.ip() {
            IpAddr::V4(v4) => IpAddr::V4(v4),
            IpAddr::V6(v6) => {
                if let Some(mapped) = v6.to_ipv4_mapped() {
                    IpAddr::V4(mapped)
                } else {
                    IpAddr::V6(v6)
                }
            }
        };

        // Ignore our own local IPs
        let local_ips = Self::get_local_ipv4s();
        if let IpAddr::V4(v4) = ip {
            if v4.is_loopback() || local_ips.iter().any(|(local, _)| *local == v4) {
                return;
            }
        }

        let packet = match NetworkPacket::parse_line(data) {
            Ok(p) => p,
            Err(e) => {
                log::debug!(
                    "[udp] Ignoring non-JSON or malformed UDP datagram from {}: {}",
                    sender_addr,
                    e
                );
                return;
            }
        };

        if packet.packet_type != PACKET_TYPE_IDENTITY {
            return;
        }

        let Some(device_id) = packet.get_str("deviceId").map(|s| s.to_string()) else {
            return;
        };

        if device_id == self.host.device_id {
            return;
        }

        let protocol_version = packet
            .get_i64("protocolVersion")
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;
        let tcp_port = packet
            .get_i64("tcpPort")
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(MIN_TCP_PORT);
        let device_name = packet
            .get_str("deviceName")
            .unwrap_or("Unknown")
            .to_string();

        log::info!(
            "[udp] Received UDP identity broadcast from '{}' (id={}, ip={}, tcpPort={}, proto=v{})",
            device_name,
            device_id,
            ip,
            tcp_port,
            protocol_version
        );

        let daemon = Arc::clone(self);
        tokio::spawn(async move {
            daemon
                .on_device_discovered(ip, tcp_port, device_id, protocol_version, Some(packet))
                .await;
        });
    }

    pub async fn on_device_discovered(
        self: &Arc<Self>,
        ip: IpAddr,
        tcp_port: u16,
        device_id: String,
        protocol_version: i32,
        udp_packet: Option<NetworkPacket>,
    ) {
        if !(MIN_TCP_PORT..=MAX_TCP_PORT).contains(&tcp_port) {
            log::warn!(
                "[discovery] Ignoring device {} with out-of-range TCP port {}",
                device_id,
                tcp_port
            );
            return;
        }

        // Check if this device already has an active, healthy TLS connection at this IP
        // (Matches Soduto's ConnectionProviderDelegate.isNewConnectionNeeded)
        {
            let mut devices = self.devices.lock().unwrap();
            if let Some(entry) = devices.get_mut(&device_id) {
                if entry.view.reachable
                    && entry.sender.is_some()
                    && entry.view.address == ip.to_string()
                {
                    entry.view.last_seen_secs = current_secs();
                    log::debug!(
                        "[discovery] Device '{}' ({}) is already connected and online at {}, skipping redundant outbound connection",
                        entry.view.device_name,
                        device_id,
                        ip
                    );
                    return;
                }
            }
        }

        // Rate-limit connection attempts to same device within 1500ms
        let now = current_millis();
        {
            let mut attempts = self.last_discovery_attempt_ms.lock().unwrap();
            if let Some(last) = attempts.get(&device_id) {
                if now - *last < 1500 {
                    log::debug!(
                        "[discovery] Skipping duplicate discovery of {} ({}ms since last attempt)",
                        device_id,
                        now - *last
                    );
                    return;
                }
            }
            attempts.insert(device_id.clone(), now);
        }

        let target_addr = SocketAddr::new(ip, tcp_port);
        log::info!(
            "[tcp-out] Connecting to discovered device {} at {} (proto=v{})...",
            device_id,
            target_addr,
            protocol_version
        );

        match tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(target_addr)).await {
            Ok(Ok(tcp_stream)) => {
                if let Err(e) = self
                    .handle_outbound_tcp(
                        tcp_stream,
                        target_addr,
                        device_id.clone(),
                        protocol_version,
                        udp_packet,
                    )
                    .await
                {
                    log::warn!(
                        "[tcp-out] Outbound session with {} ({}) ended: {:?}",
                        device_id,
                        target_addr,
                        e
                    );
                }
            }
            Ok(Err(e)) => {
                log::warn!(
                    "[tcp-out] TCP connect to {} failed ({}), sending reverse UDP discovery packet",
                    target_addr,
                    e
                );
                let _ = self.probe_ip(&ip.to_string()).await;
            }
            Err(_) => {
                log::warn!(
                    "[tcp-out] TCP connect to {} timed out, sending reverse UDP discovery packet",
                    target_addr
                );
                let _ = self.probe_ip(&ip.to_string()).await;
            }
        }
    }

    /// Outbound TCP Connection Flow (`LanLinkProvider::tcpSocketConnected` + `LanLinkProvider::encrypted`):
    /// 1. Send plaintext connection/identity packet
    /// 2. Act as TLS SERVER (`TlsAcceptor`)
    /// 3. Send our encrypted full identity packet and read peer's encrypted identity packet (if v8)
    async fn handle_outbound_tcp(
        self: &Arc<Self>,
        mut tcp_stream: TcpStream,
        peer_addr: SocketAddr,
        expected_device_id: String,
        protocol_version: i32,
        udp_identity_fallback: Option<NetworkPacket>,
    ) -> Result<()> {
        Self::configure_tcp_socket(&tcp_stream);
        let raw_fd = tcp_stream.as_raw_fd();

        let conn_packet = if protocol_version >= 8 {
            NetworkPacket::tcp_connection_packet(&self.host, &expected_device_id, protocol_version)
        } else {
            NetworkPacket::full_identity(&self.host)
        };
        let conn_bytes = conn_packet.serialize_line()?;
        tcp_stream.write_all(&conn_bytes).await?;
        tcp_stream.flush().await?;
        log::debug!(
            "[tcp-out] Sent plaintext connection packet to {} ({} bytes), starting TLS Server handshake...",
            peer_addr,
            conn_bytes.len()
        );

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(6),
            self.tls_acceptor.accept(tcp_stream),
        )
        .await
        .context("TLS Server handshake timed out")?
        .context("TLS Server handshake failed")?;

        let peer_cert_der = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .map(|c| c.as_ref().to_vec())
            .ok_or_else(|| anyhow!("Peer did not present a client X.509 certificate during TLS handshake"))?;

        log::info!(
            "[tls] Outbound TCP -> TLS Server handshake succeeded with {} (cert SHA256={})",
            peer_addr,
            sha256_fingerprint(&peer_cert_der)
        );

        // In Protocol v8, both sides exchange their full identity packet over the encrypted TLS stream
        let my_identity_bytes = NetworkPacket::full_identity(&self.host).serialize_line()?;
        tls_stream.write_all(&my_identity_bytes).await?;
        tls_stream.flush().await?;
        log::debug!(
            "[tls] Sent encrypted full identity packet to {}",
            peer_addr
        );

        let mut buf_reader = BufReader::new(tls_stream);
        let mut first_line = String::new();

        let final_identity = if protocol_version >= 8 {
            let n = tokio::time::timeout(
                Duration::from_secs(3),
                buf_reader.read_line(&mut first_line),
            )
            .await
            .context("Timed out waiting for peer encrypted identity packet (v8)")??;
            if n == 0 {
                return Err(anyhow!("Peer closed TLS stream before sending encrypted identity"));
            }
            let pkt = NetworkPacket::parse_line(first_line.as_bytes())?;
            log::info!(
                "[tls] Received encrypted v8 identity packet from '{}'",
                pkt.get_str("deviceName").unwrap_or("Unknown")
            );
            pkt
        } else {
            udp_identity_fallback.ok_or_else(|| anyhow!("Missing v7 identity packet"))?
        };

        self.run_authenticated_link(buf_reader, raw_fd, peer_addr, final_identity, peer_cert_der)
            .await
    }

    /// Inbound TCP Connection Flow (`LanLinkProvider::newTcpConnection` + `tcpPacketReceived` + `encrypted`):
    /// 1. Read peer's plaintext connection/identity packet from TCP socket
    /// 2. Act as TLS CLIENT (`TlsConnector`)
    /// 3. Send our encrypted full identity packet and read peer's encrypted identity packet (if v8)
    async fn handle_inbound_tcp(
        self: &Arc<Self>,
        tcp_stream: TcpStream,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        Self::configure_tcp_socket(&tcp_stream);
        let raw_fd = tcp_stream.as_raw_fd();
        let mut plain_reader = BufReader::new(tcp_stream);
        let mut line = String::new();

        let n = tokio::time::timeout(Duration::from_secs(3), plain_reader.read_line(&mut line))
            .await
            .context("Timed out waiting for plaintext connection packet on inbound TCP")??;
        if n == 0 {
            return Err(anyhow!("Inbound TCP closed before sending connection packet"));
        }

        let initial_packet = NetworkPacket::parse_line(line.as_bytes())?;
        if initial_packet.packet_type != PACKET_TYPE_IDENTITY {
            return Err(anyhow!(
                "Expected kdeconnect.identity packet on inbound TCP, got {}",
                initial_packet.packet_type
            ));
        }

        let peer_device_id = initial_packet
            .get_str("deviceId")
            .ok_or_else(|| anyhow!("Inbound connection packet missing deviceId"))?
            .to_string();
        let peer_proto = initial_packet
            .get_i64("protocolVersion")
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;

        // Deduplicate burst inbound TCP connections from the same device within 800ms
        let now_ms = current_millis();
        {
            let mut attempts = self.last_discovery_attempt_ms.lock().unwrap();
            let key = format!("inbound:{}", peer_device_id);
            if let Some(last) = attempts.get(&key) {
                if now_ms - *last < 800 {
                    log::debug!(
                        "[tcp-in] Dropping duplicate burst inbound TCP connection from {} (deviceId={}, {}ms since previous)",
                        peer_addr,
                        peer_device_id,
                        now_ms - *last
                    );
                    return Ok(());
                }
            }
            attempts.insert(key, now_ms);
        }

        log::info!(
            "[tcp-in] Received plaintext connection packet from {} (deviceId={}, proto=v{}), starting TLS Client handshake...",
            peer_addr,
            peer_device_id,
            peer_proto
        );

        let raw_tcp = plain_reader.into_inner();
        let server_name = ServerName::try_from("kdeconnect.local")
            .unwrap_or_else(|_| ServerName::IpAddress(peer_addr.ip().into()));

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(6),
            self.tls_connector.connect(server_name, raw_tcp),
        )
        .await
        .context("TLS Client handshake timed out")?
        .context("TLS Client handshake failed")?;

        let peer_cert_der = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .map(|c| c.as_ref().to_vec())
            .ok_or_else(|| anyhow!("Peer did not present a server X.509 certificate during TLS handshake"))?;

        log::info!(
            "[tls] Inbound TCP -> TLS Client handshake succeeded with {} (cert SHA256={})",
            peer_addr,
            sha256_fingerprint(&peer_cert_der)
        );

        if peer_proto >= 8 {
            let my_identity_bytes = NetworkPacket::full_identity(&self.host).serialize_line()?;
            tls_stream.write_all(&my_identity_bytes).await?;
            tls_stream.flush().await?;
            log::debug!(
                "[tls] Sent encrypted full identity packet to {} (inbound TCP)",
                peer_addr
            );

            let mut buf_reader = BufReader::new(tls_stream);
            let mut sec_line = String::new();
            let n = tokio::time::timeout(
                Duration::from_secs(3),
                buf_reader.read_line(&mut sec_line),
            )
            .await
            .context("Timed out waiting for peer encrypted v8 identity on inbound TCP")??;
            if n == 0 {
                return Err(anyhow!(
                    "Peer closed inbound TLS stream before sending encrypted identity"
                ));
            }
            let sec_pkt = NetworkPacket::parse_line(sec_line.as_bytes())?;
            log::info!(
                "[tls] Received encrypted v8 identity packet from '{}' (id={})",
                sec_pkt.get_str("deviceName").unwrap_or("Unknown"),
                sec_pkt.get_str("deviceId").unwrap_or("?")
            );
            self.run_authenticated_link(buf_reader, raw_fd, peer_addr, sec_pkt, peer_cert_der)
                .await
        } else {
            let buf_reader = BufReader::new(tls_stream);
            self.run_authenticated_link(buf_reader, raw_fd, peer_addr, initial_packet, peer_cert_der)
                .await
        }
    }

    async fn run_authenticated_link<S>(
        self: &Arc<Self>,
        buf_reader: BufReader<S>,
        raw_fd: RawFd,
        peer_addr: SocketAddr,
        identity_packet: NetworkPacket,
        peer_cert_der: Vec<u8>,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let device_id = identity_packet
            .get_str("deviceId")
            .ok_or_else(|| anyhow!("Identity packet missing deviceId"))?
            .to_string();
        let device_name = identity_packet
            .get_str("deviceName")
            .unwrap_or("KDE Connect Device")
            .to_string();
        let device_type = identity_packet
            .get_str("deviceType")
            .unwrap_or("phone")
            .to_string();
        let protocol_version = identity_packet
            .get_i64("protocolVersion")
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;
        let incoming_caps = identity_packet.get_string_list("incomingCapabilities");
        let outgoing_caps = identity_packet.get_string_list("outgoingCapabilities");

        if let Some(cert_cn) = extract_cert_common_name(&peer_cert_der) {
            if cert_cn != device_id {
                log::warn!(
                    "[tls] Certificate CN '{}' differs from identity packet deviceId '{}'",
                    cert_cn,
                    device_id
                );
            }
        }

        // Verify pinned certificate if this device is already trusted/paired
        let default_downloads_str = self.downloads_dir.to_string_lossy().to_string();
        let (
            is_trusted,
            saved_sync_clipboard,
            saved_allow_presenter,
            saved_sync_notifications,
            saved_auto_copy_otp,
            saved_save_location,
            saved_run_commands,
        ) = {
            let trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get(&device_id) {
                if rec.cert_der != peer_cert_der {
                    log::error!(
                        "[tls] SECURITY ALERT: Device {} ('{}') presented a different certificate than pinned! Aborting connection.",
                        device_id,
                        device_name
                    );
                    return Err(anyhow!("Certificate mismatch for paired device {}", device_id));
                }
                let loc = rec
                    .save_location
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| default_downloads_str.clone());
                (
                    true,
                    rec.sync_clipboard,
                    rec.allow_presenter,
                    rec.sync_notifications,
                    rec.auto_copy_otp,
                    loc,
                    rec.run_commands.clone(),
                )
            } else {
                (false, true, true, true, true, default_downloads_str, Vec::new())
            }
        };

        let cert_fp = sha256_fingerprint(&peer_cert_der);
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (abort_tx, mut abort_rx) = oneshot::channel::<()>();

        let generation = {
            let mut devices = self.devices.lock().unwrap();
            let mut prev_abort: Option<oneshot::Sender<()>> = None;
            let next_gen = if let Some(existing) = devices.get_mut(&device_id) {
                prev_abort = existing.abort_tx.take();
                existing.connection_generation + 1
            } else {
                1
            };

            if let Some(old_abort) = prev_abort {
                let _ = old_abort.send(());
            }

            let pair_state = if is_trusted {
                PairState::Paired
            } else {
                devices
                    .get(&device_id)
                    .map(|d| d.view.pair_state)
                    .unwrap_or(PairState::NotPaired)
            };

            let verification_key = devices
                .get(&device_id)
                .and_then(|d| d.view.verification_key.clone())
                .or_else(|| {
                    compute_verification_key(&self.host.cert_der, &peer_cert_der, current_secs())
                });

            let (
                prev_bat_charge,
                prev_bat_charging,
                prev_net_type,
                prev_sig_str,
                prev_sim_signals,
                prev_storage,
                prev_now_playing,
                prev_mpris_players,
                prev_mpris_order,
                prev_active_mpris,
                prev_art_cache,
            ) = devices
                .get(&device_id)
                .map(|d| {
                    (
                        d.view.battery_charge,
                        d.view.battery_charging,
                        d.view.cellular_network_type.clone(),
                        d.view.cellular_signal_strength,
                        d.view.sim_signals.clone(),
                        d.view.storage_free_gb,
                        d.view.now_playing.clone(),
                        d.mpris_players.clone(),
                        d.mpris_player_order.clone(),
                        d.active_mpris_player.clone(),
                        d.album_art_cache.clone(),
                    )
                })
                .unwrap_or((
                    None,
                    None,
                    None,
                    None,
                    Vec::new(),
                    None,
                    None,
                    HashMap::new(),
                    Vec::new(),
                    None,
                    HashMap::new(),
                ));

            let prev_notifications = devices
                .get(&device_id)
                .map(|d| d.view.notifications.clone())
                .unwrap_or_default();

            devices.insert(
                device_id.clone(),
                DeviceEntry {
                    view: DeviceView {
                        device_id: device_id.clone(),
                        device_name: device_name.clone(),
                        device_type,
                        protocol_version,
                        address: peer_addr.ip().to_string(),
                        tcp_port: peer_addr.port(),
                        reachable: true,
                        pair_state,
                        verification_key,
                        cert_fingerprint: Some(cert_fp),
                        incoming_capabilities: incoming_caps.clone(),
                        outgoing_capabilities: outgoing_caps.clone(),
                        last_seen_secs: current_secs(),
                        battery_charge: prev_bat_charge,
                        battery_charging: prev_bat_charging,
                        cellular_network_type: prev_net_type,
                        cellular_signal_strength: prev_sig_str,
                        sim_signals: prev_sim_signals,
                        storage_free_gb: prev_storage,
                        now_playing: prev_now_playing,
                        sync_clipboard: saved_sync_clipboard,
                        allow_presenter: saved_allow_presenter,
                        sync_notifications: saved_sync_notifications,
                        auto_copy_otp: saved_auto_copy_otp,
                        is_ringing: false,
                        save_location: saved_save_location,
                        notifications: prev_notifications,
                        run_commands: saved_run_commands,
                    },
                    peer_cert_der: Some(peer_cert_der.clone()),
                    sender: Some(tx),
                    abort_tx: Some(abort_tx),
                    connection_generation: next_gen,
                    mpris_players: prev_mpris_players,
                    mpris_player_order: prev_mpris_order,
                    active_mpris_player: prev_active_mpris,
                    album_art_cache: prev_art_cache,
                    album_art_in_flight: HashSet::new(),
                },
            );
            next_gen
        };

        log::info!(
            "[device] Device '{}' ({}) is now ONLINE at {} (paired={}, caps_in={:?})",
            device_name,
            device_id,
            peer_addr.ip(),
            is_trusted,
            incoming_caps
        );

        // Request initial battery and connectivity status if paired
        if is_trusted {
            self.request_device_telemetry(&device_id);
        }
        self.notify_ui();

        let (read_half, mut write_half) = tokio::io::split(buf_reader);
        let mut line_reader = BufReader::new(read_half);
        let mut line = String::new();
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(2));
        heartbeat_interval.tick().await; // consume initial immediate tick
        let mut unacked_since: Option<Instant> = None;

        loop {
            tokio::select! {
                _ = &mut abort_rx => {
                    log::debug!(
                        "[device] Connection gen {} for '{}' ({}) superseded by newer session, closing old link",
                        generation,
                        device_name,
                        device_id
                    );
                    break;
                }
                _ = heartbeat_interval.tick() => {
                    let unacked = Self::get_socket_unacked_bytes(raw_fd);
                    if unacked > 0 {
                        if let Some(since) = unacked_since {
                            if since.elapsed() >= Duration::from_millis(1800) {
                                log::warn!(
                                    "[heartbeat] Device '{}' ({}) has {} unacknowledged TCP bytes after {:?} — peer is offline, closing link",
                                    device_name,
                                    device_id,
                                    unacked,
                                    since.elapsed()
                                );
                                break;
                            }
                        } else {
                            unacked_since = Some(Instant::now());
                        }
                    } else {
                        unacked_since = None;
                        let is_currently_paired = self
                            .devices
                            .lock()
                            .ok()
                            .and_then(|d| d.get(&device_id).map(|e| e.view.pair_state == PairState::Paired))
                            .unwrap_or(false);

                        // Only send application-layer keepalive packets when PAIRED!
                        // In KDE Connect, sending any non-`kdeconnect.pair` packet to an unpaired device
                        // causes the remote peer to respond with `{"type":"kdeconnect.pair","body":{"pair":false}}`.
                        if is_currently_paired {
                            if let Ok(hb_bytes) = NetworkPacket::keepalive().serialize_line() {
                                if let Err(e) = write_half.write_all(&hb_bytes).await {
                                    log::warn!("[heartbeat] Write error to '{}': {}", device_name, e);
                                    break;
                                }
                                if let Err(e) = write_half.flush().await {
                                    log::warn!("[heartbeat] Flush error to '{}': {}", device_name, e);
                                    break;
                                }
                                unacked_since = Some(Instant::now());
                            }
                        }
                    }
                }
                maybe_out = rx.recv() => {
                    match maybe_out {
                        Some(bytes) => {
                            if let Err(e) = write_half.write_all(&bytes).await {
                                log::warn!("[device] Write error to '{}': {}", device_name, e);
                                break;
                            }
                            let _ = write_half.flush().await;
                            if unacked_since.is_none() {
                                unacked_since = Some(Instant::now());
                            }
                        }
                        None => break,
                    }
                }
                read_res = line_reader.read_line(&mut line) => {
                    match read_res {
                        Ok(0) => {
                            log::info!("[device] Device '{}' ({}) closed the TLS connection", device_name, device_id);
                            break;
                        }
                        Ok(_) => {
                            unacked_since = None;
                            let raw_line = line.clone();
                            line.clear();
                            if let Ok(mut devices) = self.devices.lock() {
                                if let Some(entry) = devices.get_mut(&device_id) {
                                    entry.view.last_seen_secs = current_secs();
                                }
                            }
                            match NetworkPacket::parse_line(raw_line.as_bytes()) {
                                Ok(pkt) => {
                                    self.handle_incoming_packet(&device_id, &device_name, peer_addr.ip(), &peer_cert_der, pkt).await;
                                }
                                Err(e) => {
                                    log::warn!("[device] Failed to parse packet from '{}': {} | raw={}", device_name, e, raw_line.trim());
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("[device] Read error from '{}' ({}): {}", device_name, device_id, e);
                            break;
                        }
                    }
                }
            }
        }

        // Mark offline if this was the latest active connection generation
        let mut became_offline = false;
        {
            let mut devices = self.devices.lock().unwrap();
            if let Some(entry) = devices.get_mut(&device_id) {
                if entry.connection_generation == generation {
                    entry.view.reachable = false;
                    entry.view.is_ringing = false;
                    entry.view.now_playing = None;
                    entry.view.notifications.clear();
                    entry.mpris_players.clear();
                    entry.mpris_player_order.clear();
                    entry.active_mpris_player = None;
                    entry.album_art_in_flight.clear();
                    entry.sender = None;
                    entry.abort_tx = None;
                    became_offline = true;
                    log::info!(
                        "[device] Device '{}' ({}) marked OFFLINE",
                        device_name,
                        device_id
                    );
                }
            }
        }
        self.notify_ui();

        if became_offline {
            self.sftp_manager.clear_device(&device_id);
            // Send a discovery broadcast so the device can reconnect immediately as soon as its Wi-Fi returns
            let daemon = Arc::clone(self);
            tokio::spawn(async move {
                daemon.broadcast_udp_discovery().await;
            });
        }

        Ok(())
    }

    async fn handle_incoming_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        peer_cert_der: &[u8],
        packet: NetworkPacket,
    ) {
        if packet.packet_type == "velquo.keepalive" || packet.packet_type == "soduto.keepalive" {
            return;
        }

        log::info!(
            "[packet-in] From '{}' ({}): type='{}' body={} payloadSize={:?} transferInfo={:?}",
            device_name,
            device_id,
            packet.packet_type,
            serde_json::to_string(&packet.body).unwrap_or_default(),
            packet.payload_size,
            packet.payload_transfer_info
        );

        match packet.packet_type.as_str() {
            PACKET_TYPE_PAIR => {
                self.handle_pair_packet(device_id, device_name, peer_cert_der, &packet);
            }
            PACKET_TYPE_PING => {
                let msg = packet.get_str("message").unwrap_or("Ping!");
                log::info!("[ping] Received Ping from '{}': \"{}\"", device_name, msg);
                Self::show_macos_notification(device_name, msg, Some(device_id), None, false);
                if let Ok(guard) = self.app_handle.lock() {
                    if let Some(app) = guard.as_ref() {
                        let _ = app.emit(
                            "velquo://ping",
                            serde_json::json!({
                                "deviceId": device_id,
                                "deviceName": device_name,
                                "message": msg,
                            }),
                        );
                    }
                }
            }
            PACKET_TYPE_BATTERY_REQUEST => {
                log::info!(
                    "[battery-out] Device '{}' ({}) requested Mac battery status",
                    device_name,
                    device_id
                );
                self.send_host_battery_to_device(device_id);
            }
            PACKET_TYPE_BATTERY => {
                if packet.get_bool("request").unwrap_or(false) {
                    log::info!(
                        "[battery-out] Device '{}' ({}) requested Mac battery status (legacy packet)",
                        device_name,
                        device_id
                    );
                    self.send_host_battery_to_device(device_id);
                }
                let charge = packet.get_i64("currentCharge").map(|c| c as i32);
                let is_charging = packet.get_bool("isCharging");
                if charge.is_some() || is_charging.is_some() {
                    log::info!(
                        "[battery] Device '{}' ({}) battery: {:?}% (charging={:?})",
                        device_name,
                        device_id,
                        charge,
                        is_charging
                    );
                    if let Ok(mut devices) = self.devices.lock() {
                        if let Some(entry) = devices.get_mut(device_id) {
                            if let Some(c) = charge {
                                if (0..=100).contains(&c) {
                                    entry.view.battery_charge = Some(c);
                                }
                            }
                            if let Some(ch) = is_charging {
                                entry.view.battery_charging = Some(ch);
                            }
                        }
                    }
                    self.notify_ui();
                }
            }
            PACKET_TYPE_CONNECTIVITY_REPORT => {
                if let Some(subs) = packet.body.get("signalStrengths").and_then(|v| v.as_object()) {
                    let mut sim_list: Vec<SimSignalInfo> = subs
                        .iter()
                        .filter_map(|(slot_key, sub_val)| {
                            let net_type = sub_val
                                .get("networkType")
                                .and_then(|v| v.as_str())?
                                .to_string();
                            let sig_strength = sub_val
                                .get("signalStrength")
                                .and_then(|v| v.as_i64())?
                                as i32;
                            Some(SimSignalInfo {
                                slot_id: slot_key.clone(),
                                network_type: net_type,
                                signal_strength: sig_strength,
                            })
                        })
                        .collect();

                    sim_list.sort_by(|a, b| {
                        a.slot_id
                            .parse::<i64>()
                            .unwrap_or(i64::MAX)
                            .cmp(&b.slot_id.parse::<i64>().unwrap_or(i64::MAX))
                            .then_with(|| a.slot_id.cmp(&b.slot_id))
                    });

                    if !sim_list.is_empty() {
                        log::info!(
                            "[connectivity] Device '{}' ({}) SIM signals: {:?}",
                            device_name,
                            device_id,
                            sim_list
                        );
                        if let Ok(mut devices) = self.devices.lock() {
                            if let Some(entry) = devices.get_mut(device_id) {
                                entry.view.cellular_network_type =
                                    Some(sim_list[0].network_type.clone());
                                entry.view.cellular_signal_strength =
                                    Some(sim_list[0].signal_strength);
                                entry.view.sim_signals = sim_list;
                            }
                        }
                        self.notify_ui();
                    }
                }
            }
            PACKET_TYPE_MPRIS => {
                self.handle_mpris_packet(device_id, device_name, peer_ip, &packet);
            }
            PACKET_TYPE_MPRIS_REQUEST => {
                let daemon = Arc::clone(self);
                let dev_id = device_id.to_string();
                let dev_name = device_name.to_string();
                let pkt_clone = packet.clone();
                tokio::spawn(async move {
                    if let Some(requested_art_url) = pkt_clone.get_str("albumArtUrl") {
                        let player = pkt_clone
                            .get_str("player")
                            .unwrap_or(DEFAULT_MAC_PLAYER)
                            .to_string();
                        let art_url = requested_art_url.to_string();
                        daemon
                            .serve_mac_album_art_to_device(&dev_id, &dev_name, &player, &art_url)
                            .await;
                        return;
                    }

                    if let Ok(responses) =
                        tokio::task::spawn_blocking(move || handle_incoming_mpris_request(&pkt_clone))
                            .await
                    {
                        for resp in responses {
                            if let Err(e) = daemon.send_packet_to_device(&dev_id, &resp) {
                                log::warn!(
                                    "[mpris-host] Failed to send Mac MPRIS response to '{}': {}",
                                    dev_name,
                                    e
                                );
                            }
                        }
                    }
                });
            }
            PACKET_TYPE_TELEPHONY => {
                self.handle_telephony_packet(device_id, device_name, &packet);
            }
            PACKET_TYPE_CLIPBOARD | PACKET_TYPE_CLIPBOARD_CONNECT => {
                self.handle_clipboard_packet(device_id, device_name, &packet);
            }
            PACKET_TYPE_PRESENTER => {
                if self.is_device_presenter_allowed(device_id) {
                    handle_presenter_packet(&packet);
                }
            }
            PACKET_TYPE_MOUSEPAD_REQUEST => {
                if self.is_device_presenter_allowed(device_id) {
                    handle_mousepad_packet(&packet);
                }
            }
            PACKET_TYPE_FINDMYPHONE_REQUEST => {
                log::info!(
                    "[findmyphone] Received Find My Mac request from '{}'",
                    device_name
                );
                Self::show_macos_notification(
                    device_name,
                    "Find My Mac triggered!",
                    Some(device_id),
                    None,
                    false,
                );
                let _ = std::process::Command::new("/usr/bin/afplay")
                    .arg("/System/Library/Sounds/Submarine.aiff")
                    .spawn();
            }
            PACKET_TYPE_NOTIFICATION => {
                self.handle_notification_packet(device_id, device_name, &packet);
            }
            PACKET_TYPE_RUNCOMMAND_REQUEST => {
                self.handle_runcommand_request(device_id, device_name, &packet);
            }
            PACKET_TYPE_SHARE_REQUEST => {
                self.handle_share_request(device_id, device_name, peer_ip, packet)
                    .await;
            }
            PACKET_TYPE_SHARE_REQUEST_UPDATE => {
                log::info!(
                    "[share-in] Share update packet from '{}': {:?}",
                    device_name,
                    packet.body
                );
            }
            PACKET_TYPE_SFTP => {
                self.sftp_manager.handle_incoming_sftp_packet(
                    device_id,
                    device_name,
                    peer_ip,
                    &packet,
                );
            }
            other => {
                log::debug!(
                    "[packet-in] Ignoring unhandled packet type '{}' from '{}'",
                    other,
                    device_name
                );
            }
        }
    }

    /// Handles incoming `kdeconnect.telephony` packets to automatically pause Mac media during
    /// incoming/active phone calls (`ringing` / `talking`) and resume when the call ends (`isCancel: true`).
    fn handle_telephony_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        let Some(event) = packet.get_str("event") else {
            return;
        };
        let is_cancel = packet.get_bool("isCancel").unwrap_or(false);
        let contact = packet
            .get_str("contactName")
            .or_else(|| packet.get_str("phoneNumber"))
            .unwrap_or("Unknown caller");

        log::info!(
            "[telephony] Device '{}' ({}) telephony event='{}' isCancel={} caller='{}'",
            device_name,
            device_id,
            event,
            is_cancel,
            contact
        );

        match event {
            "ringing" | "talking" if !is_cancel => {
                let already_paused_for_this_call = {
                    let mut active = self.active_call_events.lock().unwrap();
                    active.insert(format!("{}:{}", device_id, event));
                    let mut seq = self.call_state_seq.lock().unwrap();
                    *seq = seq.wrapping_add(1);
                    let paused = self.call_paused_sources.lock().unwrap();
                    !paused.is_empty()
                };

                // Only run `pause_mac_media_for_call` if we haven't already paused Mac media for this call
                // (prevents a `ringing` -> `talking` transition from sending a second toggle key!)
                if !already_paused_for_this_call {
                    let daemon = Arc::clone(self);
                    tokio::spawn(async move {
                        if let Ok(newly_paused) =
                            tokio::task::spawn_blocking(pause_mac_media_for_call).await
                        {
                            if !newly_paused.is_empty() {
                                let mut paused = daemon.call_paused_sources.lock().unwrap();
                                for src in newly_paused {
                                    paused.insert(src);
                                }
                            }
                        }
                    });
                }
            }
            "ringing" | "talking" if is_cancel => {
                let (all_calls_ended, snapshot_seq) = {
                    let mut active = self.active_call_events.lock().unwrap();
                    active.remove(&format!("{}:{}", device_id, event));
                    if event == "talking" {
                        active.remove(&format!("{}:ringing", device_id));
                    }
                    let mut seq = self.call_state_seq.lock().unwrap();
                    *seq = seq.wrapping_add(1);
                    (active.is_empty(), *seq)
                };

                if all_calls_ended {
                    let daemon = Arc::clone(self);
                    tokio::spawn(async move {
                        // Brief 450ms debounce so a `ringing (isCancel=true)` -> `talking` transition
                        // when answering a call does not momentarily unpause media.
                        tokio::time::sleep(Duration::from_millis(450)).await;
                        let sources_to_resume: Vec<String> = {
                            let active = daemon.active_call_events.lock().unwrap();
                            let seq = *daemon.call_state_seq.lock().unwrap();
                            if !active.is_empty() || seq != snapshot_seq {
                                return;
                            }
                            let mut paused = daemon.call_paused_sources.lock().unwrap();
                            paused.drain().collect()
                        };

                        if !sources_to_resume.is_empty() {
                            let _ = tokio::task::spawn_blocking(move || {
                                resume_mac_media_after_call(&sources_to_resume);
                            })
                            .await;
                        }
                    });
                }
            }
            "missedCall" if !is_cancel => {
                let (all_calls_ended, snapshot_seq) = {
                    let mut active = self.active_call_events.lock().unwrap();
                    active.remove(&format!("{}:ringing", device_id));
                    let mut seq = self.call_state_seq.lock().unwrap();
                    *seq = seq.wrapping_add(1);
                    (active.is_empty(), *seq)
                };

                if all_calls_ended {
                    let daemon = Arc::clone(self);
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(450)).await;
                        let sources_to_resume: Vec<String> = {
                            let active = daemon.active_call_events.lock().unwrap();
                            let seq = *daemon.call_state_seq.lock().unwrap();
                            if !active.is_empty() || seq != snapshot_seq {
                                return;
                            }
                            let mut paused = daemon.call_paused_sources.lock().unwrap();
                            paused.drain().collect()
                        };

                        if !sources_to_resume.is_empty() {
                            let _ = tokio::task::spawn_blocking(move || {
                                resume_mac_media_after_call(&sources_to_resume);
                            })
                            .await;
                        }
                    });
                }
            }
            _ => {}
        }
    }

    fn handle_pair_packet(
        &self,
        device_id: &str,
        device_name: &str,
        peer_cert_der: &[u8],
        packet: &NetworkPacket,
    ) {
        let wants_pair = packet.get_bool("pair").unwrap_or(false);
        let timestamp = packet.get_i64("timestamp").unwrap_or_else(current_secs);

        let mut devices = self.devices.lock().unwrap();
        let Some(entry) = devices.get_mut(device_id) else {
            return;
        };

        if wants_pair {
            match entry.view.pair_state {
                PairState::Requested => {
                    // We requested pairing and peer accepted!
                    log::info!(
                        "[pair] Device '{}' ({}) ACCEPTED our pairing request!",
                        device_name,
                        device_id
                    );
                    entry.view.pair_state = PairState::Paired;
                    entry.view.verification_key = None;
                    let rec = TrustedDeviceRecord {
                        device_id: device_id.to_string(),
                        device_name: device_name.to_string(),
                        device_type: entry.view.device_type.clone(),
                        protocol_version: entry.view.protocol_version,
                        cert_der: peer_cert_der.to_vec(),
                        sync_clipboard: entry.view.sync_clipboard,
                        allow_presenter: entry.view.allow_presenter,
                        sync_notifications: entry.view.sync_notifications,
                        auto_copy_otp: entry.view.auto_copy_otp,
                        save_location: Some(entry.view.save_location.clone()),
                        run_commands: entry.view.run_commands.clone(),
                    };
                    drop(devices);
                    self.trusted_devices
                        .lock()
                        .unwrap()
                        .insert(device_id.to_string(), rec);
                    self.save_trusted_devices();
                    self.request_device_telemetry(device_id);
                }
                PairState::NotPaired | PairState::Paired => {
                    let vkey =
                        compute_verification_key(&self.host.cert_der, peer_cert_der, timestamp);
                    log::info!(
                        "[pair] Incoming pairing request from '{}' ({})! Verification Key: {:?}",
                        device_name,
                        device_id,
                        vkey
                    );
                    entry.view.pair_state = PairState::RequestedByPeer;
                    entry.view.verification_key = vkey;
                    drop(devices);
                }
                PairState::RequestedByPeer => {
                    log::debug!(
                        "[pair] Duplicate pairing request from '{}', still waiting for user confirmation",
                        device_name
                    );
                    drop(devices);
                }
            }
        } else {
            log::info!(
                "[pair] Device '{}' ({}) sent UNPAIR / REJECT packet",
                device_name,
                device_id
            );
            entry.view.pair_state = PairState::NotPaired;
            entry.view.verification_key = None;
            entry.view.now_playing = None;
            entry.view.notifications.clear();
            entry.view.battery_charge = None;
            entry.view.battery_charging = None;
            entry.view.sim_signals.clear();
            entry.mpris_players.clear();
            entry.mpris_player_order.clear();
            entry.active_mpris_player = None;
            entry.album_art_in_flight.clear();
            drop(devices);
            self.trusted_devices.lock().unwrap().remove(device_id);
            self.save_trusted_devices();
        }

        self.notify_ui();
    }

    pub fn send_packet_to_device(&self, device_id: &str, packet: &NetworkPacket) -> Result<()> {
        let bytes = packet.serialize_line()?;
        let devices = self.devices.lock().unwrap();
        let entry = devices
            .get(device_id)
            .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
        let sender = entry
            .sender
            .as_ref()
            .ok_or_else(|| anyhow!("Device '{}' is currently offline", entry.view.device_name))?;
        sender
            .send(bytes)
            .map_err(|_| anyhow!("Failed to send packet to device channel"))?;
        log::info!(
            "[packet-out] Sent '{}' to '{}' ({})",
            packet.packet_type,
            entry.view.device_name,
            device_id
        );
        Ok(())
    }

    pub fn request_pairing(&self, device_id: &str) -> Result<()> {
        let ts = current_secs();
        let pkt = NetworkPacket::pair_request(ts);
        self.send_packet_to_device(device_id, &pkt)?;

        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.pair_state = PairState::Requested;
                if let Some(peer_cert) = &entry.peer_cert_der {
                    entry.view.verification_key =
                        compute_verification_key(&self.host.cert_der, peer_cert, ts);
                }
                log::info!(
                    "[pair] Sent pairing request to '{}' (ts={}, verification_key={:?})",
                    entry.view.device_name,
                    ts,
                    entry.view.verification_key
                );
            }
        }
        self.notify_ui();
        Ok(())
    }

    pub fn accept_pairing(&self, device_id: &str) -> Result<()> {
        let pkt = NetworkPacket::pair_accept();
        self.send_packet_to_device(device_id, &pkt)?;

        let mut trusted_record = None;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.pair_state = PairState::Paired;
                entry.view.verification_key = None;
                if let Some(cert_der) = &entry.peer_cert_der {
                    trusted_record = Some(TrustedDeviceRecord {
                        device_id: entry.view.device_id.clone(),
                        device_name: entry.view.device_name.clone(),
                        device_type: entry.view.device_type.clone(),
                        protocol_version: entry.view.protocol_version,
                        cert_der: cert_der.clone(),
                        sync_clipboard: entry.view.sync_clipboard,
                        allow_presenter: entry.view.allow_presenter,
                        sync_notifications: entry.view.sync_notifications,
                        auto_copy_otp: entry.view.auto_copy_otp,
                        save_location: Some(entry.view.save_location.clone()),
                        run_commands: entry.view.run_commands.clone(),
                    });
                }
                log::info!(
                    "[pair] Accepted pairing with '{}' ({})",
                    entry.view.device_name,
                    device_id
                );
            }
        }

        if let Some(rec) = trusted_record {
            self.trusted_devices
                .lock()
                .unwrap()
                .insert(device_id.to_string(), rec);
            self.save_trusted_devices();
            self.request_device_telemetry(device_id);
        }

        self.notify_ui();
        Ok(())
    }

    pub fn request_device_telemetry(&self, device_id: &str) {
        self.send_host_battery_to_device(device_id);
        let _ = self.send_packet_to_device(device_id, &NetworkPacket::battery_request());
        let _ =
            self.send_packet_to_device(device_id, &NetworkPacket::connectivity_report_request());
        let _ = self.send_packet_to_device(device_id, &NetworkPacket::mpris_request_player_list());
        self.send_host_mpris_to_device(device_id);
        self.send_host_clipboard_connect_to_device(device_id);
        let allow_pres = self.is_device_presenter_allowed(device_id);
        let _ = self.send_packet_to_device(
            device_id,
            &NetworkPacket::mousepad_keyboard_state(allow_pres),
        );
        if self.is_device_notifications_allowed(device_id) {
            let _ = self.send_packet_to_device(device_id, &NetworkPacket::notification_request());
        }
        self.send_host_runcommands_to_device(device_id);
    }

    fn serialize_command_list_json(commands: &[RunCommandItem]) -> String {
        let mut map = serde_json::Map::new();
        for item in commands {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), serde_json::json!(item.name));
            obj.insert("command".into(), serde_json::json!(item.command));
            map.insert(item.key.clone(), serde_json::Value::Object(obj));
        }
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn send_host_runcommands_to_device(&self, device_id: &str) {
        let cmds_json = {
            let devices = self.devices.lock().unwrap();
            let Some(entry) = devices.get(device_id) else {
                return;
            };
            if entry.view.pair_state != PairState::Paired {
                return;
            }
            Self::serialize_command_list_json(&entry.view.run_commands)
        };
        let pkt = NetworkPacket::runcommand_list(&cmds_json);
        let _ = self.send_packet_to_device(device_id, &pkt);
    }

    pub fn add_device_run_command(
        &self,
        device_id: &str,
        name: &str,
        command: &str,
    ) -> Result<()> {
        let clean_name = name.trim();
        let clean_cmd = command.trim();
        if clean_name.is_empty() || clean_cmd.is_empty() {
            return Err(anyhow!("Command name and command string cannot be empty"));
        }

        let item = RunCommandItem {
            key: Uuid::new_v4().to_string(),
            name: clean_name.to_string(),
            command: clean_cmd.to_string(),
        };

        let mut is_online_and_paired = false;
        let mut updated_list = Vec::new();
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.run_commands.push(item);
                updated_list = entry.view.run_commands.clone();
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[runcommand] Added command '{}' ('{}') for '{}' ({})",
                    clean_name,
                    clean_cmd,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.run_commands = updated_list;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if is_online_and_paired {
            self.send_host_runcommands_to_device(device_id);
        }

        Ok(())
    }

    pub fn remove_device_run_command(&self, device_id: &str, key: &str) -> Result<()> {
        let mut is_online_and_paired = false;
        let mut updated_list = Vec::new();
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.run_commands.retain(|c| c.key != key);
                updated_list = entry.view.run_commands.clone();
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[runcommand] Removed command key='{}' for '{}' ({})",
                    key,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.run_commands = updated_list;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if is_online_and_paired {
            self.send_host_runcommands_to_device(device_id);
        }

        Ok(())
    }

    fn handle_runcommand_request(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        let is_paired = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .map(|d| d.view.pair_state == PairState::Paired)
                .unwrap_or(false)
        };
        if !is_paired {
            return;
        }

        if packet.get_bool("requestCommandList").unwrap_or(false) {
            self.send_host_runcommands_to_device(device_id);
            return;
        }

        if packet.body.contains_key("setup") {
            log::info!(
                "[runcommand] Device '{}' ({}) requested Run Command setup window",
                device_name,
                device_id
            );
            if let Ok(guard) = self.app_handle.lock() {
                if let Some(app) = guard.as_ref() {
                    #[cfg(target_os = "macos")]
                    {
                        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                        let _ = app.show();
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit(
                        "velquo://open-runcommand-setup",
                        serde_json::json!({ "deviceId": device_id }),
                    );
                }
            }
            return;
        }

        let Some(key) = packet.get_str("key") else {
            return;
        };

        let maybe_cmd = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .and_then(|d| d.view.run_commands.iter().find(|c| c.key == key).cloned())
        };

        let Some(cmd_item) = maybe_cmd else {
            log::warn!(
                "[runcommand] Device '{}' requested unknown command key '{}'",
                device_name,
                key
            );
            return;
        };

        log::info!(
            "[runcommand] Executing command '{}' (`{}`) triggered by '{}'",
            cmd_item.name,
            cmd_item.command,
            device_name
        );

        let daemon = Arc::clone(self);
        let dev_id = device_id.to_string();
        let cmd_str = cmd_item.command.clone();
        tauri::async_runtime::spawn(async move {
            let run_id = (current_millis() & 0x7FFF_FFFF) as u32;
            let mut started_body = serde_json::Map::new();
            started_body.insert("commandStarted".into(), serde_json::json!(true));
            started_body.insert("command".into(), serde_json::json!(cmd_str));
            started_body.insert("id".into(), serde_json::json!(run_id));
            let _ = daemon.send_packet_to_device(
                &dev_id,
                &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, started_body),
            );

            let cmd_for_exec = cmd_str.clone();
            let output_res = tokio::task::spawn_blocking(move || {
                std::process::Command::new("/bin/sh")
                    .args(["-c", &cmd_for_exec])
                    .output()
            })
            .await;

            match output_res {
                Ok(Ok(output)) => {
                    let stdout_lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(|s| s.to_string())
                        .collect();
                    let stderr_lines: Vec<String> = String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .map(|s| s.to_string())
                        .collect();
                    if !stdout_lines.is_empty() || !stderr_lines.is_empty() {
                        let mut out_body = serde_json::Map::new();
                        out_body.insert("commandOutput".into(), serde_json::json!(true));
                        out_body.insert("stdout".into(), serde_json::json!(stdout_lines));
                        out_body.insert("stderr".into(), serde_json::json!(stderr_lines));
                        out_body.insert("id".into(), serde_json::json!(run_id));
                        let _ = daemon.send_packet_to_device(
                            &dev_id,
                            &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, out_body),
                        );
                    }
                    let exit_code = output.status.code().unwrap_or(0);
                    let mut fin_body = serde_json::Map::new();
                    fin_body.insert("commandFinished".into(), serde_json::json!(true));
                    fin_body.insert("success".into(), serde_json::json!(output.status.success()));
                    fin_body.insert("exitCode".into(), serde_json::json!(exit_code));
                    fin_body.insert("id".into(), serde_json::json!(run_id));
                    let _ = daemon.send_packet_to_device(
                        &dev_id,
                        &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, fin_body),
                    );
                }
                _ => {
                    let mut fin_body = serde_json::Map::new();
                    fin_body.insert("commandFinished".into(), serde_json::json!(true));
                    fin_body.insert("success".into(), serde_json::json!(false));
                    fin_body.insert("exitCode".into(), serde_json::json!(1));
                    fin_body.insert("id".into(), serde_json::json!(run_id));
                    let _ = daemon.send_packet_to_device(
                        &dev_id,
                        &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, fin_body),
                    );
                }
            }
        });
    }

    pub fn is_device_notifications_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.sync_notifications)
            .unwrap_or(false)
    }

    pub fn is_device_auto_copy_otp_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.auto_copy_otp)
            .unwrap_or(false)
    }

    pub fn set_device_auto_copy_otp(&self, device_id: &str, enabled: bool) -> Result<()> {
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.auto_copy_otp = enabled;
                log::info!(
                    "[otp] Set auto_copy_otp={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.auto_copy_otp = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }

    pub fn set_device_notifications_sync(&self, device_id: &str, enabled: bool) -> Result<()> {
        let mut is_online_and_paired = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.sync_notifications = enabled;
                if !enabled {
                    entry.view.notifications.clear();
                }
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[notifications] Set sync_notifications={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.sync_notifications = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if enabled && is_online_and_paired {
            let _ = self.send_packet_to_device(device_id, &NetworkPacket::notification_request());
        }

        Ok(())
    }

    pub fn dismiss_device_notification(
        &self,
        device_id: &str,
        notification_id: &str,
    ) -> Result<()> {
        let _ = self.send_packet_to_device(
            device_id,
            &NetworkPacket::notification_cancel(notification_id),
        );
        let norm_target = Self::normalize_notif_id(notification_id);
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry
                    .view
                    .notifications
                    .retain(|n| Self::normalize_notif_id(&n.id) != norm_target);
            }
        }
        self.notify_ui();
        Ok(())
    }

    pub fn ring_device(&self, device_id: &str) -> Result<()> {
        let pkt = NetworkPacket::find_my_phone();
        self.send_packet_to_device(device_id, &pkt)?;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.is_ringing = true;
            }
        }
        self.notify_ui();
        Ok(())
    }

    pub fn copy_text_to_clipboard(self: &Arc<Self>, text: &str) -> Result<()> {
        let new_count = write_macos_clipboard_text(text);
        let mut state = self.clipboard_state.lock().unwrap();
        state.last_content = text.to_string();
        state.last_timestamp_ms = current_millis();
        if let Some(cnt) = new_count {
            state.last_change_count = cnt;
        }
        Ok(())
    }

    /// Extracts a 4..=8 digit OTP / 2FA verification code when the notification text contains
    /// verification/OTP context keywords (`otp`, `code`, `verification`, `verify`, `passcode`, `2fa`, `pin`, etc.).
    fn extract_otp_code(combined_text: &str) -> Option<String> {
        let lower = combined_text.to_ascii_lowercase();
        const KEYWORDS: [&str; 16] = [
            "otp",
            "code",
            "verification",
            "verify",
            "passcode",
            "password",
            "2fa",
            "security",
            "one-time",
            "one time",
            "auth",
            "pin",
            "login",
            "confirm",
            "is your",
            "use ",
        ];
        if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
            return None;
        }

        for raw_token in combined_text.split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | '.' | '!' | '?' | ';' | ':' | '(' | ')' | '[' | ']' | '"' | '\'' | '<' | '>'
                )
        }) {
            let trimmed = raw_token.trim_matches('-');
            if trimmed.is_empty() {
                continue;
            }

            // Support either pure digits (4..=8 chars) or hyphenated 6-digit codes like `123-456`
            let digits_only: String = if trimmed.chars().all(|c| c.is_ascii_digit()) {
                trimmed.to_string()
            } else if trimmed.len() == 7
                && trimmed.as_bytes()[3] == b'-'
                && trimmed[..3].chars().all(|c| c.is_ascii_digit())
                && trimmed[4..].chars().all(|c| c.is_ascii_digit())
            {
                format!("{}{}", &trimmed[..3], &trimmed[4..])
            } else {
                continue;
            };

            if (4..=8).contains(&digits_only.len()) {
                return Some(digits_only);
            }
        }

        None
    }

    fn normalize_notif_id(raw: &str) -> String {
        let mut s = raw.replace(['\r', '\n'], "").trim().to_string();
        if let Some(idx) = s.find("org.kde.kdeconnect_tp::") {
            s = s[idx + "org.kde.kdeconnect_tp::".len()..].to_string();
        }
        s
    }

    fn extract_notif_package(norm_id: &str) -> Option<&str> {
        // Android StatusBarNotification key format: `userId|packageName|id|tag|uid`
        let mut parts = norm_id.split('|');
        let _user = parts.next()?;
        let pkg = parts.next()?;
        if pkg.contains('.') {
            Some(pkg)
        } else {
            None
        }
    }

    fn is_group_summary_notif_id(norm_id: &str) -> bool {
        // Group summary notifications typically have `null` as the 4th pipe-separated segment (`tag`)
        let parts: Vec<&str> = norm_id.split('|').collect();
        parts.len() >= 4 && parts[3] == "null"
    }

    fn is_generic_summary_placeholder(n: &PhoneNotification) -> bool {
        let norm = Self::normalize_notif_id(&n.id);
        let title_generic = n.title.is_empty() || n.title.eq_ignore_ascii_case(&n.app_name);
        let body_redacted = n
            .text
            .to_ascii_lowercase()
            .contains("sensitive notification content hidden");
        Self::is_group_summary_notif_id(&norm) && (title_generic || body_redacted)
    }

    fn handle_notification_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        let is_cancel = packet.get_bool("isCancel").unwrap_or(false);
        let raw_id_original = packet.get_str("id").unwrap_or("").to_string();
        let norm_id = Self::normalize_notif_id(&raw_id_original);

        // When the user dismisses the Find My Phone ring alert on their phone, Android sends
        // `kdeconnect.notification` with `isCancel: true` (often with id containing `org.kde.kdeconnect_tp`).
        if is_cancel {
            let mut ring_stopped = false;
            if let Ok(mut devices) = self.devices.lock() {
                if let Some(entry) = devices.get_mut(device_id) {
                    if entry.view.is_ringing
                        && (norm_id.contains("kdeconnect") || !norm_id.is_empty())
                    {
                        entry.view.is_ringing = false;
                        ring_stopped = true;
                        log::info!(
                            "[findmyphone] Ringing dismissed on '{}' (notif_id='{}')",
                            device_name,
                            norm_id
                        );
                    }
                }
            }
            if ring_stopped {
                self.notify_ui();
            }
        }

        if !self.is_device_notifications_allowed(device_id) {
            return;
        }

        if packet.get_bool("request").unwrap_or(false) {
            return;
        }

        if norm_id.is_empty() {
            return;
        }

        if is_cancel {
            let mut removed = false;
            let cancel_pkg = Self::extract_notif_package(&norm_id).map(|s| s.to_string());
            let cancel_is_summary = Self::is_group_summary_notif_id(&norm_id);

            if let Ok(mut devices) = self.devices.lock() {
                if let Some(entry) = devices.get_mut(device_id) {
                    let before = entry.view.notifications.len();

                    // 1. Remove exact normalized ID match
                    entry.view.notifications.retain(|n| {
                        let existing_norm = Self::normalize_notif_id(&n.id);
                        existing_norm != norm_id
                            && !existing_norm.ends_with(&format!("::{}", norm_id))
                            && !norm_id.ends_with(&format!("::{}", existing_norm))
                    });

                    // 2. If Android cancelled the group summary (`|null|`), or if a child notification
                    // was cancelled leaving only a generic summary placeholder for that package, clean it up
                    if let Some(ref pkg) = cancel_pkg {
                        if cancel_is_summary && entry.view.notifications.len() == before {
                            // User swiped away the grouped notification stack or Android cancelled a summary ID
                            // Remove any generic summary placeholder for this package
                            entry.view.notifications.retain(|n| {
                                let n_norm = Self::normalize_notif_id(&n.id);
                                let same_pkg = Self::extract_notif_package(&n_norm) == Some(pkg.as_str());
                                !(same_pkg && Self::is_generic_summary_placeholder(n))
                            });
                            // If still nothing was removed (e.g. only 1 notification from this package existed with a different sub-id), remove it
                            if entry.view.notifications.len() == before {
                                let pkg_count = entry
                                    .view
                                    .notifications
                                    .iter()
                                    .filter(|n| {
                                        Self::extract_notif_package(&Self::normalize_notif_id(&n.id))
                                            == Some(pkg.as_str())
                                    })
                                    .count();
                                if pkg_count == 1 {
                                    entry.view.notifications.retain(|n| {
                                        Self::extract_notif_package(&Self::normalize_notif_id(&n.id))
                                            != Some(pkg.as_str())
                                    });
                                }
                            }
                        } else {
                            // If a specific child notification was removed, check if only a generic summary placeholder remains for this package
                            let remaining_specific_for_pkg = entry
                                .view
                                .notifications
                                .iter()
                                .filter(|n| {
                                    let n_norm = Self::normalize_notif_id(&n.id);
                                    Self::extract_notif_package(&n_norm) == Some(pkg.as_str())
                                        && !Self::is_generic_summary_placeholder(n)
                                })
                                .count();
                            if remaining_specific_for_pkg == 0 {
                                entry.view.notifications.retain(|n| {
                                    let n_norm = Self::normalize_notif_id(&n.id);
                                    let same_pkg =
                                        Self::extract_notif_package(&n_norm) == Some(pkg.as_str());
                                    !(same_pkg && Self::is_generic_summary_placeholder(n))
                                });
                            }
                        }
                    }

                    removed = entry.view.notifications.len() != before;
                }
            }
            if removed {
                self.notify_ui();
            }
            return;
        }

        let app_name = packet
            .get_str("appName")
            .unwrap_or("")
            .trim()
            .to_string();
        if app_name.eq_ignore_ascii_case("KDE Connect") {
            return;
        }

        let title = packet.get_str("title").unwrap_or("").trim().to_string();
        let text = packet.get_str("text").unwrap_or("").trim().to_string();
        let ticker = packet.get_str("ticker").unwrap_or("").trim().to_string();

        if title.is_empty() && text.is_empty() && ticker.is_empty() {
            return;
        }

        let is_clearable = packet.get_bool("isClearable").unwrap_or(true);
        let is_request_answer = packet.get_bool("requestAnswer").unwrap_or(false);
        let is_silent = packet.get_bool("silent").unwrap_or(false);
        let timestamp_ms = packet
            .get_i64("time")
            .filter(|&t| t > 0)
            .unwrap_or_else(current_millis);

        let combined = format!("{} {} {}", title, text, ticker);
        let otp_code = Self::extract_otp_code(&combined);

        let is_whatsapp = app_name.eq_ignore_ascii_case("whatsapp")
            || app_name.eq_ignore_ascii_case("whatsapp business")
            || norm_id.contains("|com.whatsapp|")
            || norm_id.contains("|com.whatsapp.w4b|");
        if is_whatsapp && otp_code.is_none() && self.is_ignore_whatsapp_conversations_enabled() {
            log::debug!(
                "[notifications] Ignoring non-OTP WhatsApp conversation notification from '{}'",
                device_name
            );
            return;
        }

        let notif_item = PhoneNotification {
            id: raw_id_original.clone(),
            app_name: if app_name.is_empty() {
                device_name.to_string()
            } else {
                app_name.clone()
            },
            title: title.clone(),
            text: text.clone(),
            ticker: ticker.clone(),
            is_clearable,
            timestamp_ms,
            otp_code: otp_code.clone(),
        };

        let incoming_pkg = Self::extract_notif_package(&norm_id).map(|s| s.to_string());
        let incoming_is_generic_summary = Self::is_generic_summary_placeholder(&notif_item);

        let mut is_new_or_changed = false;
        let mut is_new_otp = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                // If this incoming notification is a generic group-summary placeholder (`|null|`)
                // and we already have a specific notification for the same package, ignore the redundant summary card!
                if incoming_is_generic_summary {
                    if let Some(ref pkg) = incoming_pkg {
                        let has_specific = entry.view.notifications.iter().any(|n| {
                            let n_norm = Self::normalize_notif_id(&n.id);
                            Self::extract_notif_package(&n_norm) == Some(pkg.as_str())
                                && !Self::is_generic_summary_placeholder(n)
                        });
                        if has_specific {
                            return;
                        }
                    }
                } else if let Some(ref pkg) = incoming_pkg {
                    // Conversely, when a specific notification arrives for a package, remove any generic summary placeholder for that package
                    entry.view.notifications.retain(|n| {
                        let n_norm = Self::normalize_notif_id(&n.id);
                        let same_pkg = Self::extract_notif_package(&n_norm) == Some(pkg.as_str());
                        !(same_pkg && Self::is_generic_summary_placeholder(n))
                    });
                }

                if let Some(existing) = entry
                    .view
                    .notifications
                    .iter_mut()
                    .find(|n| Self::normalize_notif_id(&n.id) == norm_id)
                {
                    if existing.title != notif_item.title
                        || existing.text != notif_item.text
                        || existing.ticker != notif_item.ticker
                    {
                        is_new_or_changed = true;
                    }
                    if notif_item.otp_code.is_some() && existing.otp_code != notif_item.otp_code {
                        is_new_otp = true;
                    }
                    *existing = notif_item;
                } else {
                    is_new_or_changed = true;
                    if notif_item.otp_code.is_some() {
                        is_new_otp = true;
                    }
                    entry.view.notifications.insert(0, notif_item);
                }

                entry
                    .view
                    .notifications
                    .sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
                if entry.view.notifications.len() > 25 {
                    entry.view.notifications.truncate(25);
                }
            }
        }

        self.notify_ui();

        let notif_heading = if !app_name.is_empty() {
            format!("{} • {}", app_name, device_name)
        } else {
            device_name.to_string()
        };
        let notif_body = if !title.is_empty() && !text.is_empty() && title != app_name {
            format!("{}: {}", title, text)
        } else if !text.is_empty() {
            text.clone()
        } else if !ticker.is_empty() {
            ticker.clone()
        } else {
            title.clone()
        };

        // If an OTP is detected and auto-copy OTP is enabled for this device, always copy the OTP
        // (regardless of `is_silent`) and show ONLY the "Copied OTP: {{otp}}" macOS system notification.
        if let Some(ref code) = otp_code {
            let is_recent_or_live =
                !is_request_answer || (current_millis() - timestamp_ms).abs() <= 90_000;
            if is_new_otp && is_recent_or_live && self.is_device_auto_copy_otp_allowed(device_id) {
                log::info!(
                    "[otp] Extracted OTP code '{}' from '{}' notification on '{}' (silent={}), auto-copying to Mac clipboard",
                    code,
                    app_name,
                    device_name,
                    is_silent
                );
                let _ = self.copy_text_to_clipboard(code);

                let otp_notif_title = format!("Copied OTP: {}", code);
                Self::show_macos_notification(
                    &otp_notif_title,
                    &notif_body,
                    Some(device_id),
                    Some(&raw_id_original),
                    true,
                );

                if let Ok(guard) = self.app_handle.lock() {
                    if let Some(app) = guard.as_ref() {
                        let _ = app.emit(
                            "velquo://ping",
                            serde_json::json!({
                                "deviceId": device_id,
                                "deviceName": format!("Copied OTP: {}", code),
                                "message": notif_body,
                            }),
                        );
                    }
                }
                return;
            }
        }

        // For non-OTP live, non-silent notifications, show the standard native macOS notification
        if is_new_or_changed && !is_request_answer && !is_silent {
            Self::show_macos_notification(
                &notif_heading,
                &notif_body,
                Some(device_id),
                Some(&raw_id_original),
                true,
            );
        }
    }

    pub fn is_device_presenter_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.allow_presenter)
            .unwrap_or(false)
    }

    pub fn set_device_presenter(&self, device_id: &str, enabled: bool) -> Result<()> {
        let mut is_online_and_paired = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.allow_presenter = enabled;
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[presenter-input] Set allow_presenter={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.allow_presenter = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if is_online_and_paired {
            let _ = self.send_packet_to_device(
                device_id,
                &NetworkPacket::mousepad_keyboard_state(enabled),
            );
        }

        Ok(())
    }

    pub fn has_online_clipboard_sync_device(&self) -> bool {
        let devices = self.devices.lock().unwrap();
        devices.values().any(|d| {
            d.view.reachable && d.view.pair_state == PairState::Paired && d.view.sync_clipboard
        })
    }

    pub fn broadcast_clipboard_to_synced_devices(&self, pkt: &NetworkPacket) {
        let synced_online_ids: Vec<String> = {
            let devices = self.devices.lock().unwrap();
            devices
                .values()
                .filter(|d| {
                    d.view.reachable
                        && d.view.pair_state == PairState::Paired
                        && d.view.sync_clipboard
                })
                .map(|d| d.view.device_id.clone())
                .collect()
        };
        for id in synced_online_ids {
            let _ = self.send_packet_to_device(&id, pkt);
        }
    }

    pub fn send_host_clipboard_connect_to_device(&self, device_id: &str) {
        let maybe_sender = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .filter(|d| d.view.pair_state == PairState::Paired && d.view.sync_clipboard)
                .and_then(|d| d.sender.clone())
        };
        let Some(sender) = maybe_sender else {
            return;
        };

        let existing_ts = {
            let state = self.clipboard_state.lock().unwrap();
            state.last_timestamp_ms
        };

        if let Some(text) = read_macos_clipboard_text() {
            let ts = if existing_ts > 0 {
                existing_ts
            } else {
                current_millis()
            };
            let pkt = NetworkPacket::clipboard_connect(&text, ts);
            if let Ok(bytes) = pkt.serialize_line() {
                let _ = sender.send(bytes);
            }
        }
    }

    pub fn set_device_clipboard_sync(&self, device_id: &str, enabled: bool) -> Result<()> {
        let mut is_online_and_paired = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.sync_clipboard = enabled;
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[clipboard] Set sync_clipboard={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.sync_clipboard = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if enabled && is_online_and_paired {
            self.send_host_clipboard_connect_to_device(device_id);
        }

        Ok(())
    }

    pub fn set_device_save_location(&self, device_id: &str, save_location: &str) -> Result<()> {
        let trimmed = save_location.trim();
        let target_dir = if trimmed.is_empty() {
            self.downloads_dir.clone()
        } else {
            let p = PathBuf::from(trimmed);
            fs::create_dir_all(&p)
                .with_context(|| format!("Cannot create or access directory {:?}", p))?;
            p
        };
        let target_str = target_dir.to_string_lossy().to_string();

        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.save_location = target_str.clone();
                log::info!(
                    "[settings] Set save_location='{}' for '{}' ({})",
                    target_str,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.save_location = Some(target_str);
            }
        }
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }

    fn handle_clipboard_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        let is_allowed = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .map(|d| d.view.pair_state == PairState::Paired && d.view.sync_clipboard)
                .unwrap_or(false)
        };
        if !is_allowed {
            log::debug!(
                "[clipboard-in] Ignoring clipboard packet from '{}' (sync_clipboard is disabled or unpaired)",
                device_name
            );
            return;
        }

        let Some(content) = packet.get_str("content") else {
            return;
        };
        if content.is_empty() {
            return;
        }

        if packet.packet_type == PACKET_TYPE_CLIPBOARD_CONNECT {
            let remote_ts = packet.get_i64("timestamp").unwrap_or(0);
            let local_ts = self.clipboard_state.lock().unwrap().last_timestamp_ms;
            if remote_ts == 0 || (local_ts > 0 && remote_ts < local_ts) {
                log::debug!(
                    "[clipboard-in] Ignoring older clipboard.connect from '{}' (remote_ts={}, local_ts={})",
                    device_name,
                    remote_ts,
                    local_ts
                );
                return;
            }
        }

        let text_to_write = content.to_string();
        let now_ms = packet
            .get_i64("timestamp")
            .filter(|&t| t > 0)
            .unwrap_or_else(current_millis);

        {
            let mut state = self.clipboard_state.lock().unwrap();
            if state.last_content == text_to_write {
                return;
            }
            state.last_content = text_to_write.clone();
            state.last_timestamp_ms = now_ms;
        }

        log::info!(
            "[clipboard-in] Received clipboard from '{}' ({} chars), updating macOS pasteboard",
            device_name,
            text_to_write.len()
        );

        if let Some(new_count) = write_macos_clipboard_text(&text_to_write) {
            let mut state = self.clipboard_state.lock().unwrap();
            state.last_change_count = new_count;
        }
    }

    pub fn has_online_paired_device(&self) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .values()
            .any(|d| d.view.reachable && d.view.pair_state == PairState::Paired)
    }

    pub fn broadcast_to_paired_devices(&self, pkt: &NetworkPacket) {
        let paired_online_ids: Vec<String> = {
            let devices = self.devices.lock().unwrap();
            devices
                .values()
                .filter(|d| d.view.reachable && d.view.pair_state == PairState::Paired)
                .map(|d| d.view.device_id.clone())
                .collect()
        };
        for id in paired_online_ids {
            let _ = self.send_packet_to_device(&id, pkt);
        }
    }

    pub fn send_host_mpris_to_device(&self, device_id: &str) {
        let maybe_sender = {
            let devices = self.devices.lock().unwrap();
            devices.get(device_id).and_then(|d| d.sender.clone())
        };
        let Some(sender) = maybe_sender else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            for delay_ms in [0u64, 900u64] {
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                if let Ok((players, snap_opt)) = tokio::task::spawn_blocking(|| {
                    let p = get_mac_player_list();
                    let s = query_mac_player_status("");
                    (p, s)
                })
                .await
                {
                    let mut body = serde_json::Map::new();
                    body.insert("playerList".into(), serde_json::json!(players));
                    body.insert("supportAlbumArtPayload".into(), serde_json::json!(true));
                    let list_pkt = NetworkPacket::new(PACKET_TYPE_MPRIS, body);
                    if let Ok(bytes) = list_pkt.serialize_line() {
                        let _ = sender.send(bytes);
                    }
                    if let Some(snap) = snap_opt {
                        if let Ok(bytes) = snap.to_mpris_packet(true, true).serialize_line() {
                            let _ = sender.send(bytes);
                        }
                    }
                }
            }
        });
    }

    pub async fn serve_mac_album_art_to_device(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        player: &str,
        album_art_url: &str,
    ) {
        let url_owned = album_art_url.to_string();
        let maybe_bytes =
            tokio::task::spawn_blocking(move || get_cached_mac_album_art(&url_owned))
                .await
                .ok()
                .flatten();

        let Some(art_bytes) = maybe_bytes else {
            log::debug!(
                "[mpris-host] No cached Mac album art found for '{}' requested by '{}'",
                album_art_url,
                device_name
            );
            return;
        };

        let mut payload_listener = None;
        let mut payload_port = MIN_PAYLOAD_PORT;
        for port in MIN_PAYLOAD_PORT..=MAX_PAYLOAD_PORT {
            if let Ok(listener) =
                TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await
            {
                payload_port = port;
                payload_listener = Some(listener);
                break;
            }
        }

        let Some(listener) = payload_listener else {
            log::warn!(
                "[mpris-host] No free payload port ({}..={}) to stream Mac album art",
                MIN_PAYLOAD_PORT,
                MAX_PAYLOAD_PORT
            );
            return;
        };

        let mut body = serde_json::Map::new();
        body.insert("transferringAlbumArt".into(), serde_json::json!(true));
        body.insert("player".into(), serde_json::json!(player));
        body.insert("albumArtUrl".into(), serde_json::json!(album_art_url));

        let mut transfer_info = serde_json::Map::new();
        transfer_info.insert("port".into(), serde_json::json!(payload_port));

        let mut pkt = NetworkPacket::new(PACKET_TYPE_MPRIS, body);
        pkt.payload_size = Some(art_bytes.len() as i64);
        pkt.payload_transfer_info = Some(transfer_info);

        if let Err(e) = self.send_packet_to_device(device_id, &pkt) {
            log::warn!(
                "[mpris-host] Failed to send transferringAlbumArt packet to '{}': {}",
                device_name,
                e
            );
            return;
        }

        log::info!(
            "[mpris-host] Streaming Mac album art ({} bytes) for '{}' on port {} to '{}'",
            art_bytes.len(),
            player,
            payload_port,
            device_name
        );

        if let Ok(Ok((tcp_stream, _peer_addr))) =
            tokio::time::timeout(Duration::from_secs(10), listener.accept()).await
        {
            if let Ok(Ok(mut tls_stream)) = tokio::time::timeout(
                Duration::from_secs(8),
                self.tls_acceptor.accept(tcp_stream),
            )
            .await
            {
                let _ = tls_stream.write_all(&art_bytes).await;
                let _ = tls_stream.flush().await;
                let _ = tls_stream.shutdown().await;
                log::info!(
                    "[mpris-host] Finished streaming Mac album art ({} bytes) to '{}'",
                    art_bytes.len(),
                    device_name
                );
            }
        }
    }

    /// Reads the host MacBook's battery percentage (0..=100) and charging status via `pmset -g batt`.
    pub fn read_macos_battery_status() -> Option<(i32, bool)> {
        let output = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "batt"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let on_ac_power = stdout.contains("'AC Power'");
        for line in stdout.lines() {
            if let Some(pct_idx) = line.find("%;") {
                let prefix = &line[..pct_idx];
                let num_str: String = prefix
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                if let Ok(charge) = num_str.parse::<i32>() {
                    let clamped = charge.clamp(0, 100);
                    let after = &line[pct_idx + 2..];
                    let is_charging = on_ac_power
                        || (after.contains("charging") && !after.contains("discharging") && !after.contains("not charging"))
                        || after.contains("charged")
                        || after.contains("finishing charge");
                    return Some((clamped, is_charging));
                }
            }
        }
        None
    }

    pub fn send_host_battery_to_device(&self, device_id: &str) {
        if let Some((charge, is_charging)) = Self::read_macos_battery_status() {
            let pkt = NetworkPacket::battery_status(charge, is_charging);
            if self.send_packet_to_device(device_id, &pkt).is_ok() {
                log::info!(
                    "[battery-out] Sent Mac battery status to {}: {}% (charging={})",
                    device_id,
                    charge,
                    is_charging
                );
            }
        }
    }

    pub fn broadcast_host_battery(&self, charge: i32, is_charging: bool) {
        let pkt = NetworkPacket::battery_status(charge, is_charging);
        let paired_online_ids: Vec<String> = {
            let devices = self.devices.lock().unwrap();
            devices
                .values()
                .filter(|d| d.view.reachable && d.view.pair_state == PairState::Paired)
                .map(|d| d.view.device_id.clone())
                .collect()
        };
        for id in paired_online_ids {
            let _ = self.send_packet_to_device(&id, &pkt);
        }
    }

    pub fn send_mpris_action(&self, device_id: &str, player: &str, action: &str) -> Result<()> {
        // Pin this player as the active sticky player when the user interacts with it
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.active_mpris_player = Some(player.to_string());
            }
        }
        let pkt = NetworkPacket::mpris_action(player, action);
        self.send_packet_to_device(device_id, &pkt)?;
        // Also request refreshed status shortly after action
        let _ = self.send_packet_to_device(
            device_id,
            &NetworkPacket::mpris_request_player_status(player),
        );
        Ok(())
    }

    pub fn send_mpris_seek(&self, device_id: &str, player: &str, position_ms: i64) -> Result<()> {
        let clamped_pos = position_ms.max(0);
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.active_mpris_player = Some(player.to_string());
                if let Some(p_info) = entry.mpris_players.get_mut(player) {
                    p_info.position_ms = clamped_pos;
                    p_info.position_updated_at_ms = current_millis();
                }
                Self::recompute_active_mpris(entry);
            }
        }
        self.notify_ui();

        let pkt = NetworkPacket::mpris_set_position(player, clamped_pos);
        self.send_packet_to_device(device_id, &pkt)?;
        Ok(())
    }

    fn recompute_active_mpris(entry: &mut DeviceEntry) {
        // 1. If the currently pinned active player is actively playing (is_playing == true), keep it
        if let Some(active_name) = &entry.active_mpris_player {
            if let Some(info) = entry.mpris_players.get(active_name) {
                if info.is_playing && !info.title.is_empty() {
                    entry.view.now_playing = Some(info.clone());
                    return;
                }
            }
        }

        // 2. If any player in Android's playerList order (or mpris_players) is actively playing, switch to it
        for name in &entry.mpris_player_order {
            if let Some(info) = entry.mpris_players.get(name) {
                if info.is_playing && !info.title.is_empty() {
                    entry.active_mpris_player = Some(name.clone());
                    entry.view.now_playing = Some(info.clone());
                    return;
                }
            }
        }
        for (name, info) in &entry.mpris_players {
            if info.is_playing && !info.title.is_empty() {
                entry.active_mpris_player = Some(name.clone());
                entry.view.now_playing = Some(info.clone());
                return;
            }
        }

        // 3. No player is currently playing (all paused):
        //    If we already have a sticky active player that still exists in playerList and has a title, keep it
        //    so pausing from Velquo or the phone keeps the card visible!
        if let Some(active_name) = &entry.active_mpris_player {
            let still_in_list = entry.mpris_player_order.is_empty()
                || entry.mpris_player_order.contains(active_name);
            if still_in_list {
                if let Some(info) = entry.mpris_players.get(active_name) {
                    if !info.title.is_empty() {
                        entry.view.now_playing = Some(info.clone());
                        return;
                    }
                }
            }
        }

        // 4. Otherwise, pick the first player in Android's playerList order (most recently active session)
        //    that has a non-empty title.
        for name in &entry.mpris_player_order {
            if let Some(info) = entry.mpris_players.get(name) {
                if !info.title.is_empty() {
                    // Only pin active_mpris_player if the primary player (index 0) has already reported
                    let first_has_reported = entry
                        .mpris_player_order
                        .first()
                        .map(|first| entry.mpris_players.contains_key(first))
                        .unwrap_or(true);
                    if first_has_reported {
                        entry.active_mpris_player = Some(name.clone());
                    }
                    entry.view.now_playing = Some(info.clone());
                    return;
                }
            }
        }

        // 5. No active media players with a track
        entry.active_mpris_player = None;
        entry.view.now_playing = None;
    }

    fn handle_mpris_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        packet: &NetworkPacket,
    ) {
        // 0. Handle incoming album art binary payload transfer (transferringAlbumArt: true)
        if packet.get_bool("transferringAlbumArt").unwrap_or(false) {
            let Some(port) = packet.payload_port() else {
                log::warn!("[mpris] transferringAlbumArt=true but no payload port present");
                return;
            };
            let Some(album_art_url) = packet.get_str("albumArtUrl").map(|s| s.to_string()) else {
                log::warn!("[mpris] transferringAlbumArt=true but no albumArtUrl present");
                return;
            };
            let player = packet
                .get_str("player")
                .unwrap_or("")
                .to_string();
            let payload_size = packet.payload_size.unwrap_or(0).max(0) as usize;
            if payload_size > 8 * 1024 * 1024 {
                log::warn!("[mpris] Album art payload too large ({} bytes), ignoring", payload_size);
                return;
            }

            let daemon = Arc::clone(self);
            let dev_id = device_id.to_string();
            let dev_name = device_name.to_string();
            tokio::spawn(async move {
                match daemon
                    .download_album_art_payload(peer_ip, port, payload_size)
                    .await
                {
                    Ok(data_url) => {
                        log::info!(
                            "[mpris] Downloaded album art from '{}' for player '{}' ({} chars data URL)",
                            dev_name,
                            player,
                            data_url.len()
                        );
                        if let Ok(mut devices) = daemon.devices.lock() {
                            if let Some(entry) = devices.get_mut(&dev_id) {
                                entry.album_art_in_flight.remove(&album_art_url);
                                entry
                                    .album_art_cache
                                    .insert(album_art_url.clone(), data_url.clone());

                                // Attach to any matching player entry
                                for p_info in entry.mpris_players.values_mut() {
                                    if p_info.album_art_url.as_deref() == Some(album_art_url.as_str()) {
                                        p_info.album_art_data = Some(data_url.clone());
                                    }
                                }
                                Self::recompute_active_mpris(entry);
                            }
                        }
                        daemon.notify_ui();
                    }
                    Err(e) => {
                        log::warn!(
                            "[mpris] Failed to download album art payload from '{}': {:?}",
                            dev_name,
                            e
                        );
                        if let Ok(mut devices) = daemon.devices.lock() {
                            if let Some(entry) = devices.get_mut(&dev_id) {
                                entry.album_art_in_flight.remove(&album_art_url);
                            }
                        }
                    }
                }
            });
            return;
        }

        // 1. Handle playerList updates (sent on connect and whenever a media app is opened/closed on Android)
        if packet.body.contains_key("playerList") {
            let players = packet.get_string_list("playerList");
            log::info!(
                "[mpris] Device '{}' ({}) playerList: {:?}",
                device_name,
                device_id,
                players
            );

            let newly_added_players: Vec<String> = {
                let mut devices = self.devices.lock().unwrap();
                let Some(entry) = devices.get_mut(device_id) else {
                    return;
                };

                let new_ones: Vec<String> = players
                    .iter()
                    .filter(|p| !entry.mpris_players.contains_key(*p))
                    .cloned()
                    .collect();

                // Prune any player that was closed on the phone (no longer in playerList)
                entry.mpris_players.retain(|k, _| players.contains(k));
                entry.mpris_player_order = players.clone();

                if let Some(active) = &entry.active_mpris_player {
                    if !players.contains(active) {
                        log::info!(
                            "[mpris] Active player '{}' was closed on '{}', removing from Now Playing",
                            active,
                            device_name
                        );
                        entry.active_mpris_player = None;
                    }
                }

                Self::recompute_active_mpris(entry);
                new_ones
            };

            self.notify_ui();

            // Query status only for players we don't have metadata for yet (prevents infinite request loops)
            for player in &newly_added_players {
                let _ = self.send_packet_to_device(
                    device_id,
                    &NetworkPacket::mpris_request_player_status(player),
                );
            }
        }

        // 2. Handle per-player metadata/playback state updates
        if let Some(player) = packet.get_str("player") {
            let mut art_to_request: Option<(String, String)> = None;

            {
                let mut devices = self.devices.lock().unwrap();
                let Some(entry) = devices.get_mut(device_id) else {
                    return;
                };

                let prev_player = entry.mpris_players.get(player).cloned();

                let title = if packet.body.contains_key("title") || packet.body.contains_key("nowPlaying") {
                    packet
                        .get_str("title")
                        .or_else(|| packet.get_str("nowPlaying"))
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else {
                    prev_player
                        .as_ref()
                        .map(|p| p.title.clone())
                        .unwrap_or_default()
                };

                let artist = if packet.body.contains_key("artist") {
                    packet.get_str("artist").unwrap_or("").trim().to_string()
                } else {
                    prev_player
                        .as_ref()
                        .map(|p| p.artist.clone())
                        .unwrap_or_default()
                };

                let album = if packet.body.contains_key("album") {
                    packet.get_str("album").unwrap_or("").trim().to_string()
                } else {
                    prev_player
                        .as_ref()
                        .map(|p| p.album.clone())
                        .unwrap_or_default()
                };

                let album_art_url = if packet.body.contains_key("albumArtUrl") {
                    let raw = packet.get_str("albumArtUrl").unwrap_or("").trim();
                    if raw.is_empty() {
                        None
                    } else {
                        Some(raw.to_string())
                    }
                } else {
                    prev_player.as_ref().and_then(|p| p.album_art_url.clone())
                };

                let album_art_data = if let Some(ref url) = album_art_url {
                    if let Some(cached) = entry.album_art_cache.get(url) {
                        Some(cached.clone())
                    } else {
                        if !entry.album_art_in_flight.contains(url) {
                            entry.album_art_in_flight.insert(url.clone());
                            art_to_request = Some((player.to_string(), url.clone()));
                        }
                        None
                    }
                } else {
                    None
                };

                let is_playing = packet
                    .get_bool("isPlaying")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.is_playing).unwrap_or(false));

                let (position_ms, position_updated_at_ms) = if let Some(pos) = packet.get_i64("pos") {
                    (pos, current_millis())
                } else if let Some(prev) = &prev_player {
                    (prev.position_ms, prev.position_updated_at_ms)
                } else {
                    (0, current_millis())
                };

                let length_ms = packet
                    .get_i64("length")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.length_ms).unwrap_or(0));

                let can_play = packet
                    .get_bool("canPlay")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.can_play).unwrap_or(true));
                let can_pause = packet
                    .get_bool("canPause")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.can_pause).unwrap_or(true));
                let can_go_next = packet
                    .get_bool("canGoNext")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.can_go_next).unwrap_or(true));
                let can_go_previous = packet
                    .get_bool("canGoPrevious")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.can_go_previous).unwrap_or(true));
                let can_seek = packet
                    .get_bool("canSeek")
                    .unwrap_or_else(|| prev_player.as_ref().map(|p| p.can_seek).unwrap_or(true));

                if title.is_empty() && !is_playing {
                    log::info!(
                        "[mpris] Player '{}' on '{}' has no active track, removing from active players",
                        player,
                        device_name
                    );
                    entry.mpris_players.remove(player);
                    if entry.active_mpris_player.as_deref() == Some(player) {
                        entry.active_mpris_player = None;
                    }
                } else {
                    log::info!(
                        "[mpris] Player update on '{}' ({}): '{}' by '{}' (isPlaying={}, hasArt={})",
                        device_name,
                        player,
                        title,
                        artist,
                        is_playing,
                        album_art_data.is_some()
                    );
                    entry.mpris_players.insert(
                        player.to_string(),
                        NowPlayingInfo {
                            player: player.to_string(),
                            title,
                            artist,
                            album,
                            is_playing,
                            position_ms,
                            position_updated_at_ms,
                            length_ms,
                            can_play,
                            can_pause,
                            can_go_next,
                            can_go_previous,
                            can_seek,
                            album_art_url,
                            album_art_data,
                        },
                    );
                }

                Self::recompute_active_mpris(entry);
            }

            if let Some((req_player, req_url)) = art_to_request {
                log::info!(
                    "[mpris] Requesting album art from '{}' for player '{}': {}",
                    device_name,
                    req_player,
                    req_url
                );
                let _ = self.send_packet_to_device(
                    device_id,
                    &NetworkPacket::mpris_request_album_art(&req_player, &req_url),
                );
            }

            self.notify_ui();
        }
    }

    async fn download_album_art_payload(
        &self,
        peer_ip: IpAddr,
        port: u16,
        expected_bytes: usize,
    ) -> Result<String> {
        let target = SocketAddr::new(peer_ip, port);
        let tcp = tokio::time::timeout(Duration::from_secs(8), TcpStream::connect(target))
            .await
            .context("Album art TCP connection timed out")?
            .context("Album art TCP connection failed")?;

        let server_name = ServerName::try_from("kdeconnect.local")
            .unwrap_or_else(|_| ServerName::IpAddress(peer_ip.into()));

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(8),
            self.tls_connector.connect(server_name, tcp),
        )
        .await
        .context("Album art TLS handshake timed out")?
        .context("Album art TLS handshake failed")?;

        let mut bytes = Vec::with_capacity(expected_bytes.min(5 * 1024 * 1024));
        let mut buf = [0u8; 16 * 1024];
        loop {
            let n = tls_stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..n]);
            if expected_bytes > 0 && bytes.len() >= expected_bytes {
                break;
            }
            if bytes.len() > 8 * 1024 * 1024 {
                return Err(anyhow!("Album art exceeded max size limit"));
            }
        }

        if bytes.is_empty() {
            return Err(anyhow!("Received empty album art payload"));
        }

        let mime = if bytes.starts_with(b"\x89PNG") {
            "image/png"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            "image/jpeg"
        };

        let encoded = BASE64_STANDARD.encode(&bytes);
        Ok(format!("data:{};base64,{}", mime, encoded))
    }

    pub fn unpair_or_reject(&self, device_id: &str) -> Result<()> {
        let pkt = NetworkPacket::unpair();
        let _ = self.send_packet_to_device(device_id, &pkt);

        if let Ok(mut devices) = self.devices.lock() {
            let should_remove = if let Some(entry) = devices.get_mut(device_id) {
                entry.view.pair_state = PairState::NotPaired;
                entry.view.verification_key = None;
                entry.view.now_playing = None;
                entry.view.battery_charge = None;
                entry.view.battery_charging = None;
                entry.view.sim_signals.clear();
                entry.mpris_players.clear();
                entry.mpris_player_order.clear();
                entry.active_mpris_player = None;
                entry.album_art_in_flight.clear();
                log::info!(
                    "[pair] Unpaired / rejected pairing with '{}' ({})",
                    entry.view.device_name,
                    device_id
                );
                !entry.view.reachable
            } else {
                false
            };

            if should_remove {
                devices.remove(device_id);
            }
        }
        self.trusted_devices.lock().unwrap().remove(device_id);
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }

    /// Handles incoming `kdeconnect.share.request` (File payload, Text, or URL)
    async fn handle_share_request(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        packet: NetworkPacket,
    ) {
        // Case 1: File transfer via auxiliary TLS payload socket
        if let Some(port) = packet.payload_port() {
            let raw_filename = packet
                .get_str("filename")
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("kdeconnect-{}.bin", current_millis()));
            let clean_name = Path::new(&raw_filename)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shared_file")
                .to_string();
            let total_bytes = packet.payload_size.unwrap_or(0).max(0) as u64;

            let transfer_id = Uuid::new_v4().to_string();
            let dest_path = self.unique_download_path(device_id, &clean_name);

            log::info!(
                "[share-in] Starting file download '{}' ({} bytes) from '{}' at {}:{} -> {:?}",
                clean_name,
                total_bytes,
                device_name,
                peer_ip,
                port,
                dest_path
            );

            self.upsert_transfer(TransferItem {
                id: transfer_id.clone(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                direction: "Incoming".to_string(),
                kind: "File".to_string(),
                filename: clean_name.clone(),
                total_bytes,
                transferred_bytes: 0,
                status: "InProgress".to_string(),
                saved_path: None,
                text_content: None,
                url_content: None,
                error: None,
                timestamp_ms: current_millis(),
            });

            let daemon = Arc::clone(self);
            let dev_id = device_id.to_string();
            let dev_name = device_name.to_string();
            tokio::spawn(async move {
                match daemon
                    .download_payload_file(
                        &transfer_id,
                        &dev_id,
                        &dev_name,
                        peer_ip,
                        port,
                        total_bytes,
                        &dest_path,
                    )
                    .await
                {
                    Ok(saved) => {
                        log::info!(
                            "[share-in] Completed file download '{}' -> {:?}",
                            clean_name,
                            saved
                        );
                        daemon.finish_transfer(
                            &transfer_id,
                            "Completed",
                            Some(saved.to_string_lossy().to_string()),
                            None,
                        );
                    }
                    Err(e) => {
                        log::error!(
                            "[share-in] File download '{}' failed: {:?}",
                            clean_name,
                            e
                        );
                        daemon.finish_transfer(
                            &transfer_id,
                            "Failed",
                            None,
                            Some(e.to_string()),
                        );
                    }
                }
            });
            return;
        }

        // Case 2: Shared Text
        if let Some(text) = packet.get_str("text") {
            let filename = format!("shared-text-{}.txt", current_millis());
            let save_path = self.unique_download_path(device_id, &filename);
            let _ = fs::write(&save_path, text.as_bytes());
            log::info!(
                "[share-in] Received shared text from '{}' ({} chars), saved to {:?}",
                device_name,
                text.len(),
                save_path
            );

            // Check if the last non-empty line is a valid http/https URL (matches kdeconnect-kde SharePlugin::receivePacket)
            let extracted_url = text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .last()
                .filter(|cand| cand.starts_with("http://") || cand.starts_with("https://"))
                .map(|s| s.to_string());

            self.upsert_transfer(TransferItem {
                id: Uuid::new_v4().to_string(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                direction: "Incoming".to_string(),
                kind: "Text".to_string(),
                filename: format!("Text: {}", text.chars().take(36).collect::<String>()),
                total_bytes: text.len() as u64,
                transferred_bytes: text.len() as u64,
                status: "Completed".to_string(),
                saved_path: Some(save_path.to_string_lossy().to_string()),
                text_content: Some(text.to_string()),
                url_content: extracted_url,
                error: None,
                timestamp_ms: current_millis(),
            });
            return;
        }

        // Case 3: Shared URL -> Automatically open in default macOS browser (matches kdeconnect-kde & Soduto)
        if let Some(url) = packet.get_str("url") {
            let trimmed_url = url.trim();
            log::info!(
                "[share-in] Received shared URL from '{}': {}",
                device_name,
                trimmed_url
            );

            let is_safe_scheme = trimmed_url.starts_with("http://")
                || trimmed_url.starts_with("https://")
                || trimmed_url.starts_with("mailto:");

            let mut open_err: Option<String> = None;
            if is_safe_scheme {
                match open::that(trimmed_url) {
                    Ok(()) => {
                        log::info!(
                            "[share-in] Auto-opened shared URL in default browser: {}",
                            trimmed_url
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "[share-in] Failed to auto-open shared URL '{}': {}",
                            trimmed_url,
                            e
                        );
                        open_err = Some(format!("Failed to auto-open: {}", e));
                    }
                }
            } else {
                log::warn!(
                    "[share-in] Blocked auto-opening URL with non-http(s)/mailto scheme: {}",
                    trimmed_url
                );
            }

            self.upsert_transfer(TransferItem {
                id: Uuid::new_v4().to_string(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                direction: "Incoming".to_string(),
                kind: "Url".to_string(),
                filename: trimmed_url.to_string(),
                total_bytes: 0,
                transferred_bytes: 0,
                status: "Completed".to_string(),
                saved_path: None,
                text_content: None,
                url_content: Some(trimmed_url.to_string()),
                error: open_err,
                timestamp_ms: current_millis(),
            });
        }
    }

    async fn download_payload_file(
        &self,
        transfer_id: &str,
        _device_id: &str,
        _device_name: &str,
        peer_ip: IpAddr,
        port: u16,
        expected_bytes: u64,
        dest_path: &Path,
    ) -> Result<PathBuf> {
        let target = SocketAddr::new(peer_ip, port);
        log::debug!("[share-in] Connecting to payload socket at {}...", target);
        let tcp = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(target))
            .await
            .context("Payload TCP connection timed out")?
            .context("Payload TCP connection failed")?;

        let server_name = ServerName::try_from("kdeconnect.local")
            .unwrap_or_else(|_| ServerName::IpAddress(peer_ip.into()));

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(10),
            self.tls_connector.connect(server_name, tcp),
        )
        .await
        .context("Payload TLS handshake timed out")?
        .context("Payload TLS handshake failed")?;

        log::info!(
            "[share-in] Payload TLS client connected to {}, streaming to {:?}",
            target,
            dest_path
        );

        let mut file = tokio::fs::File::create(dest_path)
            .await
            .context("Failed to create destination file")?;

        let mut buf = vec![0u8; 64 * 1024];
        let mut received: u64 = 0;
        let mut last_notify_ms = 0i64;

        loop {
            let n = tls_stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).await?;
            received += n as u64;

            let now = current_millis();
            if now - last_notify_ms >= 120 || (expected_bytes > 0 && received >= expected_bytes) {
                last_notify_ms = now;
                self.update_transfer_progress(transfer_id, received);
            }

            if expected_bytes > 0 && received >= expected_bytes {
                break;
            }
        }

        file.flush().await?;
        self.update_transfer_progress(transfer_id, received);
        Ok(dest_path.to_path_buf())
    }

    /// Sends a URL (`kdeconnect.share.request` with `{"url": ...}`) to a paired KDE Connect device
    pub fn send_url_to_device(&self, device_id: &str, url: &str) -> Result<String> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("URL cannot be empty"));
        }

        let lower = trimmed.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(anyhow!("URL must start with http:// or https://"));
        }

        let device_name = {
            let devices = self.devices.lock().unwrap();
            let entry = devices
                .get(device_id)
                .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
            if entry.view.pair_state != PairState::Paired {
                return Err(anyhow!("Device '{}' is not paired", entry.view.device_name));
            }
            if !entry.view.reachable {
                return Err(anyhow!("Device '{}' is offline", entry.view.device_name));
            }
            entry.view.device_name.clone()
        };

        let pkt = NetworkPacket::share_url(trimmed);
        self.send_packet_to_device(device_id, &pkt)?;

        log::info!(
            "[share-out] Shared URL '{}' to '{}' ({})",
            trimmed,
            device_name,
            device_id
        );

        let transfer_id = Uuid::new_v4().to_string();
        self.upsert_transfer(TransferItem {
            id: transfer_id.clone(),
            device_id: device_id.to_string(),
            device_name,
            direction: "Outgoing".to_string(),
            kind: "Url".to_string(),
            filename: trimmed.to_string(),
            total_bytes: 0,
            transferred_bytes: 0,
            status: "Completed".to_string(),
            saved_path: None,
            text_content: None,
            url_content: Some(trimmed.to_string()),
            error: None,
            timestamp_ms: current_millis(),
        });

        Ok(transfer_id)
    }

    /// Sends one or more local files sequentially in a single KDE Connect batch (`CompositeUploadJob` + `UploadJob`)
    pub async fn send_files_to_device(
        self: &Arc<Self>,
        device_id: &str,
        file_paths: Vec<PathBuf>,
    ) -> Result<Vec<String>> {
        if file_paths.is_empty() {
            return Ok(Vec::new());
        }

        let device_name = {
            let devices = self.devices.lock().unwrap();
            let entry = devices
                .get(device_id)
                .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
            if entry.view.pair_state != PairState::Paired {
                return Err(anyhow!("Device '{}' is not paired", entry.view.device_name));
            }
            if !entry.view.reachable {
                return Err(anyhow!("Device '{}' is offline", entry.view.device_name));
            }
            entry.view.device_name.clone()
        };

        let mut batch_items: Vec<(String, PathBuf, String, u64)> =
            Vec::with_capacity(file_paths.len());
        let mut total_payload_size: u64 = 0;

        for file_path in file_paths {
            let metadata = tokio::fs::metadata(&file_path)
                .await
                .with_context(|| format!("Cannot read file metadata for {:?}", file_path))?;
            if !metadata.is_file() {
                continue;
            }
            let file_size = metadata.len();
            let filename = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shared_file")
                .to_string();
            let transfer_id = Uuid::new_v4().to_string();
            total_payload_size = total_payload_size.saturating_add(file_size);
            batch_items.push((transfer_id, file_path, filename, file_size));
        }

        if batch_items.is_empty() {
            return Err(anyhow!("No valid regular files to send"));
        }

        let number_of_files = batch_items.len();

        // Bind a single TCP listener on 1739..=1764 for the sequential batch upload (`CompositeUploadJob`)
        let mut payload_listener = None;
        let mut payload_port = MIN_PAYLOAD_PORT;
        for port in MIN_PAYLOAD_PORT..=MAX_PAYLOAD_PORT {
            if let Ok(listener) =
                TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await
            {
                payload_port = port;
                payload_listener = Some(listener);
                break;
            }
        }
        let listener = payload_listener.ok_or_else(|| {
            anyhow!(
                "No available payload port in range {}..={}",
                MIN_PAYLOAD_PORT,
                MAX_PAYLOAD_PORT
            )
        })?;

        log::info!(
            "[share-out] Bound payload TCP server on port {} for batch uploading {} file(s) ({} total bytes) to '{}'",
            payload_port,
            number_of_files,
            total_payload_size,
            device_name
        );

        let base_ms = current_millis();
        let mut transfer_ids = Vec::with_capacity(number_of_files);

        // Insert all batch items in reverse so item 0 appears at index 0 in the UI list
        for (idx, (transfer_id, file_path, filename, file_size)) in
            batch_items.iter().enumerate().rev()
        {
            self.upsert_transfer(TransferItem {
                id: transfer_id.clone(),
                device_id: device_id.to_string(),
                device_name: device_name.clone(),
                direction: "Outgoing".to_string(),
                kind: "File".to_string(),
                filename: filename.clone(),
                total_bytes: *file_size,
                transferred_bytes: 0,
                status: "InProgress".to_string(),
                saved_path: Some(file_path.to_string_lossy().to_string()),
                text_content: None,
                url_content: None,
                error: None,
                timestamp_ms: base_ms + (number_of_files.saturating_sub(idx) as i64),
            });
        }

        for (tid, _, _, _) in &batch_items {
            transfer_ids.push(tid.clone());
        }

        let daemon = Arc::clone(self);
        let dev_id = device_id.to_string();
        let dev_name = device_name.clone();
        tokio::spawn(async move {
            for (idx, (tid, path_buf, fname, file_size)) in batch_items.into_iter().enumerate() {
                let now_ms = current_millis();
                let share_packet = NetworkPacket::share_file_header(
                    &fname,
                    file_size,
                    now_ms,
                    now_ms,
                    payload_port,
                    number_of_files,
                    total_payload_size,
                );

                log::info!(
                    "[share-out] Sending file {}/{}: '{}' ({} bytes) on port {} to '{}'",
                    idx + 1,
                    number_of_files,
                    fname,
                    file_size,
                    payload_port,
                    dev_name
                );

                if let Err(e) = daemon.send_packet_to_device(&dev_id, &share_packet) {
                    log::error!(
                        "[share-out] Failed to send share header for '{}' to '{}': {:?}",
                        fname,
                        dev_name,
                        e
                    );
                    daemon.finish_transfer(&tid, "Failed", None, Some(e.to_string()));
                    continue;
                }

                match daemon
                    .serve_outgoing_payload(&tid, &listener, &path_buf, file_size)
                    .await
                {
                    Ok(()) => {
                        log::info!(
                            "[share-out] Successfully uploaded {}/{} '{}' ({} bytes) to '{}'",
                            idx + 1,
                            number_of_files,
                            fname,
                            file_size,
                            dev_name
                        );
                        daemon.finish_transfer(
                            &tid,
                            "Completed",
                            Some(path_buf.to_string_lossy().to_string()),
                            None,
                        );
                    }
                    Err(e) => {
                        log::error!(
                            "[share-out] Upload of '{}' to '{}' failed: {:?}",
                            fname,
                            dev_name,
                            e
                        );
                        daemon.finish_transfer(&tid, "Failed", None, Some(e.to_string()));
                    }
                }
            }
        });

        Ok(transfer_ids)
    }

    /// Sends a single local file to a paired KDE Connect device
    pub async fn send_file_to_device(
        self: &Arc<Self>,
        device_id: &str,
        file_path: &Path,
    ) -> Result<String> {
        let mut ids = self
            .send_files_to_device(device_id, vec![file_path.to_path_buf()])
            .await?;
        ids.pop()
            .ok_or_else(|| anyhow!("Failed to enqueue file transfer"))
    }

    async fn serve_outgoing_payload(
        &self,
        transfer_id: &str,
        listener: &TcpListener,
        file_path: &Path,
        file_size: u64,
    ) -> Result<()> {
        let (tcp_stream, peer_addr) =
            tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .context("Timed out waiting for peer to connect to payload port")??;

        log::info!(
            "[share-out] Peer {} connected to payload socket, starting TLS Server handshake...",
            peer_addr
        );

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(10),
            self.tls_acceptor.accept(tcp_stream),
        )
        .await
        .context("Payload TLS Server handshake timed out")?
        .context("Payload TLS Server handshake failed")?;

        log::info!(
            "[share-out] Payload TLS Server handshake succeeded with {}, streaming {} bytes...",
            peer_addr,
            file_size
        );

        let mut file = tokio::fs::File::open(file_path).await?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut sent: u64 = 0;
        let mut last_notify_ms = 0i64;

        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            tls_stream.write_all(&buf[..n]).await?;
            sent += n as u64;

            let now = current_millis();
            if now - last_notify_ms >= 120 || sent >= file_size {
                last_notify_ms = now;
                self.update_transfer_progress(transfer_id, sent);
            }
        }

        tls_stream.flush().await?;
        let _ = tls_stream.shutdown().await;
        self.update_transfer_progress(transfer_id, sent);
        Ok(())
    }

    fn resolve_device_save_dir(&self, device_id: &str) -> PathBuf {
        let configured = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .map(|d| d.view.save_location.trim().to_string())
                .filter(|s| !s.is_empty())
        }
        .or_else(|| {
            let trusted = self.trusted_devices.lock().unwrap();
            trusted
                .get(device_id)
                .and_then(|r| r.save_location.clone())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });

        if let Some(dir_str) = configured {
            let dir = PathBuf::from(dir_str);
            if fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }
        let _ = fs::create_dir_all(&self.downloads_dir);
        self.downloads_dir.clone()
    }

    fn unique_download_path(&self, device_id: &str, filename: &str) -> PathBuf {
        let base_dir = self.resolve_device_save_dir(device_id);
        let candidate = base_dir.join(filename);
        if !candidate.exists() {
            return candidate;
        }
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default();

        for i in 1..10000 {
            let next = base_dir.join(format!("{} ({}){}", stem, i, ext));
            if !next.exists() {
                return next;
            }
        }
        base_dir.join(format!("{}-{}", current_millis(), filename))
    }

    fn upsert_transfer(&self, item: TransferItem) {
        let mut guard = self.transfers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|t| t.id == item.id) {
            *existing = item;
        } else {
            guard.insert(0, item);
        }
        drop(guard);
        self.notify_ui();
    }

    fn update_transfer_progress(&self, transfer_id: &str, bytes: u64) {
        let mut guard = self.transfers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|t| t.id == transfer_id) {
            existing.transferred_bytes = bytes;
        }
        drop(guard);
        self.notify_ui();
    }

    fn finish_transfer(
        &self,
        transfer_id: &str,
        status: &str,
        saved_path: Option<String>,
        error: Option<String>,
    ) {
        let mut guard = self.transfers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|t| t.id == transfer_id) {
            existing.status = status.to_string();
            if status == "Completed" && existing.total_bytes > 0 {
                existing.transferred_bytes = existing.total_bytes;
            }
            if let Some(p) = saved_path {
                existing.saved_path = Some(p);
            }
            existing.error = error;
            existing.timestamp_ms = current_millis();
        }
        drop(guard);
        self.notify_ui();
    }

    pub async fn sftp_browse(
        &self,
        device_id: &str,
        path: Option<String>,
    ) -> Result<SftpBrowseResponse> {
        self.sftp_manager
            .browse(device_id, path, &self.config_dir, || {
                self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing())
            })
            .await
    }

    pub async fn sftp_download_file(
        &self,
        device_id: &str,
        remote_path: &str,
        file_name: &str,
    ) -> Result<String> {
        let clean_name = Path::new(file_name)
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("downloaded_file")
            .to_string();
        let dest_path = self.unique_download_path(device_id, &clean_name);

        let saved = self
            .sftp_manager
            .download_file(device_id, remote_path, &dest_path, &self.config_dir, || {
                self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing())
            })
            .await?;

        Ok(saved.to_string_lossy().to_string())
    }

    pub async fn sftp_delete_entry(
        &self,
        device_id: &str,
        remote_path: &str,
        is_dir: bool,
    ) -> Result<()> {
        self.sftp_manager
            .delete_entry(device_id, remote_path, is_dir, &self.config_dir, || {
                self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing())
            })
            .await
    }
}
