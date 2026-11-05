use crate::kdeconnect::packet::current_secs;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

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
    pub pause_media_on_calls: bool,
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
    #[serde(default = "default_true")]
    pub pause_media_on_calls: bool,
    #[serde(default)]
    pub save_location: Option<String>,
    #[serde(default)]
    pub run_commands: Vec<RunCommandItem>,
    /// Pinned SSH host-key fingerprint for this device's SFTP server.
    #[serde(default)]
    pub ssh_host_key_fingerprint: Option<String>,
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

pub(crate) const MAX_CALL_PAUSE_RESUME_DURATION: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub struct CallMediaState {
    pub active_events: HashSet<String>,
    pub paused_sources: HashSet<String>,
    pub paused_at: Option<Instant>,
    pub paused_at_secs: Option<i64>,
    pub seq: u64,
}

impl CallMediaState {
    pub(super) fn pause_elapsed(&self) -> Option<Duration> {
        let mono = self.paused_at.map(|t| t.elapsed());
        let wall = self
            .paused_at_secs
            .and_then(|s| u64::try_from(current_secs().saturating_sub(s)).ok())
            .map(Duration::from_secs);
        match (mono, wall) {
            (Some(m), Some(w)) => Some(m.max(w)),
            (Some(m), None) => Some(m),
            (None, Some(w)) => Some(w),
            (None, None) => None,
        }
    }

    pub(super) fn is_pause_expired(&self) -> bool {
        self.pause_elapsed()
            .is_some_and(|elapsed| elapsed > MAX_CALL_PAUSE_RESUME_DURATION)
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
    /// Album-art URLs currently being fetched, with the time the request was sent.
    pub album_art_in_flight: HashMap<String, Instant>,
    /// Lock-free copy of `view.pair_state == PairState::Paired` for the hot packet path.
    pub paired: Arc<AtomicBool>,
}

impl DeviceEntry {
    pub(super) fn set_pair_state(&mut self, state: PairState) {
        self.view.pair_state = state;
        self.paired
            .store(state == PairState::Paired, Ordering::Release);
    }
}
