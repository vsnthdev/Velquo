use crate::kdeconnect::packet::{NetworkPacket, PACKET_TYPE_MPRIS};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Map};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub const DEFAULT_MAC_PLAYER: &str = "macOS Media";
pub const PLAYER_SPOTIFY: &str = "Spotify";
pub const PLAYER_APPLE_MUSIC: &str = "Music";

// MediaRemote command IDs (from MediaRemote.framework)
const MR_CMD_PLAY: i32 = 0;
const MR_CMD_PAUSE: i32 = 1;
const MR_CMD_TOGGLE_PLAY_PAUSE: i32 = 2;
const MR_CMD_STOP: i32 = 3;
const MR_CMD_NEXT_TRACK: i32 = 4;
const MR_CMD_PREVIOUS_TRACK: i32 = 5;

// IOKit/hidsystem/ev_keymap.h media key constants
const NX_KEYTYPE_PLAY: i32 = 16;
const NX_KEYTYPE_NEXT: i32 = 17;
const NX_KEYTYPE_PREVIOUS: i32 = 18;
const NX_KEYTYPE_FAST: i32 = 19;
const NX_KEYTYPE_REWIND: i32 = 20;

static CHECKED_AX_TRUST: AtomicBool = AtomicBool::new(false);

struct CachedAlbumArt {
    track_key: String,
    album_art_url: String,
    bytes: Vec<u8>,
    itunes_checked: bool,
}

static ALBUM_ART_CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
static LAST_TRACK_ART: OnceLock<Mutex<Option<CachedAlbumArt>>> = OnceLock::new();
static MR_DYLIB_PATH: OnceLock<PathBuf> = OnceLock::new();

fn album_art_store() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    ALBUM_ART_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn last_track_art_store() -> &'static Mutex<Option<CachedAlbumArt>> {
    LAST_TRACK_ART.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "macos")]
fn ensure_mr_bridge_dylib() -> Option<PathBuf> {
    if let Some(p) = MR_DYLIB_PATH.get() {
        if p.exists() {
            return Some(p.clone());
        }
    }

    const EMBEDDED_DYLIB: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/libvelquo_mr_bridge.dylib"));

    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("dev.vsnth.velquo");
    let _ = fs::create_dir_all(&dir);
    let dylib_path = dir.join("libvelquo_mr_bridge.dylib");

    let needs_write = match fs::read(&dylib_path) {
        Ok(existing) => existing != EMBEDDED_DYLIB,
        Err(_) => true,
    };

    if needs_write {
        if let Err(e) = fs::write(&dylib_path, EMBEDDED_DYLIB) {
            log::warn!(
                "[mpris-host] Failed to write libvelquo_mr_bridge.dylib to {:?}: {}",
                dylib_path,
                e
            );
            return None;
        }
    }

    let _ = MR_DYLIB_PATH.set(dylib_path.clone());
    Some(dylib_path)
}

fn deserialize_bool_or_int<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(deserializer)?;
    match val {
        serde_json::Value::Bool(b) => Ok(b),
        serde_json::Value::Number(n) => Ok(n.as_i64().unwrap_or(0) != 0),
        _ => Ok(false),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlCenterNowPlaying {
    title: String,
    artist: String,
    album: String,
    app_name: String,
    bundle_id: String,
    #[serde(deserialize_with = "deserialize_bool_or_int")]
    is_playing: bool,
    duration_ms: i64,
    position_ms: i64,
    artwork_id: String,
    artwork_base64: String,
}

#[cfg(target_os = "macos")]
fn query_control_center_now_playing(include_artwork: bool) -> Option<ControlCenterNowPlaying> {
    let dylib_path = ensure_mr_bridge_dylib()?;
    let perl_code = r#"my $h = DynaLoader::dl_load_file($ARGV[0], 0) or exit 1; my $s = DynaLoader::dl_find_symbol($h, "velquo_mr_get") or exit 1; DynaLoader::dl_install_xsub("main::run", $s); run();"#;

    let output = Command::new("/usr/bin/perl")
        .arg("-MDynaLoader")
        .arg("-e")
        .arg(perl_code)
        .arg(dylib_path.to_str()?)
        .env(
            "VELQUO_MR_INCLUDE_ARTWORK",
            if include_artwork { "1" } else { "0" },
        )
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return None;
    }

    serde_json::from_str::<ControlCenterNowPlaying>(trimmed).ok()
}

#[cfg(not(target_os = "macos"))]
fn query_control_center_now_playing(_include_artwork: bool) -> Option<ControlCenterNowPlaying> {
    None
}

#[cfg(target_os = "macos")]
fn send_control_center_mr_command(command_id: Option<i32>, seek_ms: Option<i64>) -> bool {
    let Some(dylib_path) = ensure_mr_bridge_dylib() else {
        return false;
    };
    let Some(dylib_str) = dylib_path.to_str() else {
        return false;
    };

    let perl_code = r#"my $h = DynaLoader::dl_load_file($ARGV[0], 0) or exit 1; my $s = DynaLoader::dl_find_symbol($h, "velquo_mr_control") or exit 1; DynaLoader::dl_install_xsub("main::run", $s); run();"#;

    let mut cmd = Command::new("/usr/bin/perl");
    cmd.arg("-MDynaLoader")
        .arg("-e")
        .arg(perl_code)
        .arg(dylib_str);

    if let Some(cid) = command_id {
        cmd.env("VELQUO_MR_COMMAND", cid.to_string());
    }
    if let Some(ms) = seek_ms {
        cmd.env("VELQUO_MR_SEEK_MS", ms.to_string());
    }

    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn send_control_center_mr_command(_command_id: Option<i32>, _seek_ms: Option<i64>) -> bool {
    false
}

/// Retrieves the cached album art bytes for a given `albumArtUrl` requested by the phone.
pub fn get_cached_mac_album_art(album_art_url: &str) -> Option<Vec<u8>> {
    if let Ok(guard) = album_art_store().lock() {
        if let Some(bytes) = guard.get(album_art_url) {
            return Some(bytes.clone());
        }
    }

    // If not in the map yet, trigger a fresh Control Center artwork fetch and check again
    if let Some(cc) = query_control_center_now_playing(true) {
        if !cc.artwork_base64.is_empty() {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&cc.artwork_base64)
            {
                if !bytes.is_empty() {
                    if let Ok(mut guard) = album_art_store().lock() {
                        guard.insert(album_art_url.to_string(), bytes.clone());
                    }
                    return Some(bytes);
                }
            }
        }
    }

    None
}

fn url_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn fetch_itunes_hires_artwork(title: &str, artist: &str) -> Option<Vec<u8>> {
    let query = format!("{} {}", artist.trim(), title.trim())
        .trim()
        .to_string();
    if query.is_empty() {
        return None;
    }

    let search_url = format!(
        "https://itunes.apple.com/search?term={}&entity=song&limit=1",
        url_encode_query(&query)
    );

    let search_out = Command::new("/usr/bin/curl")
        .args(["-sL", "--max-time", "5", &search_url])
        .output()
        .ok()?;
    if !search_out.status.success() {
        return None;
    }

    let json_val: serde_json::Value = serde_json::from_slice(&search_out.stdout).ok()?;
    let raw_art_url = json_val
        .get("results")?
        .as_array()?
        .first()?
        .get("artworkUrl100")?
        .as_str()?;

    let hires_url = raw_art_url.replace("100x100bb", "600x600bb");
    let img_out = Command::new("/usr/bin/curl")
        .args(["-sL", "--max-time", "8", &hires_url])
        .output()
        .ok()?;
    if !img_out.status.success() || img_out.stdout.len() < 1024 {
        return None;
    }

    Some(img_out.stdout)
}

fn spawn_itunes_hires_upgrade(track_key: String, title: String, artist: String) {
    std::thread::spawn(move || {
        let Some(hires_bytes) = fetch_itunes_hires_artwork(&title, &artist) else {
            return;
        };

        let mut hasher = Sha256::new();
        hasher.update(track_key.as_bytes());
        hasher.update(&(hires_bytes.len() as u64).to_le_bytes());
        hasher.update(b"itunes-600x600");
        let digest = hasher.finalize();
        let short_hash = digest[..8]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        let hires_url = format!("file:///velquo-macos-artwork/hires-{}.jpg", short_hash);

        if let Ok(mut store) = album_art_store().lock() {
            if store.len() > 24 {
                store.clear();
            }
            store.insert(hires_url.clone(), hires_bytes.clone());
        }

        if let Ok(mut last_guard) = last_track_art_store().lock() {
            if let Some(cached) = last_guard.as_mut() {
                if cached.track_key == track_key {
                    log::info!(
                        "[mpris-host] Upgraded Mac Now Playing artwork to 600x600 iTunes high-res ({} bytes) -> {}",
                        hires_bytes.len(),
                        hires_url
                    );
                    cached.album_art_url = hires_url;
                    cached.bytes = hires_bytes;
                    cached.itunes_checked = true;
                }
            } else {
                *last_guard = Some(CachedAlbumArt {
                    track_key,
                    album_art_url: hires_url,
                    bytes: hires_bytes,
                    itunes_checked: true,
                });
            }
        }
    });
}

fn resolve_and_cache_artwork_for_cc(cc: &ControlCenterNowPlaying) -> String {
    if cc.title.is_empty() && cc.artist.is_empty() && cc.artwork_id.is_empty() {
        return String::new();
    }

    let track_key = format!(
        "{}|{}|{}|{}|{}",
        cc.bundle_id, cc.title, cc.artist, cc.album, cc.artwork_id
    );

    // Fast path: if the same track is still playing, return its current artwork URL
    // (which starts as `thumb-<hash>.jpg` and automatically upgrades to `hires-<hash>.jpg` once iTunes finishes!)
    if let Ok(guard) = last_track_art_store().lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.track_key == track_key {
                return cached.album_art_url.clone();
            }
        }
    }

    // Stage 1: Immediately extract the local Control Center thumbnail so the phone gets instant artwork
    let mut art_bytes: Option<Vec<u8>> = None;
    if !cc.artwork_base64.is_empty() {
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&cc.artwork_base64) {
            if !decoded.is_empty() {
                art_bytes = Some(decoded);
            }
        }
    }

    if art_bytes.is_none() {
        if let Some(with_art) = query_control_center_now_playing(true) {
            if !with_art.artwork_base64.is_empty() {
                if let Ok(decoded) =
                    base64::engine::general_purpose::STANDARD.decode(&with_art.artwork_base64)
                {
                    if !decoded.is_empty() {
                        art_bytes = Some(decoded);
                    }
                }
            }
        }
    }

    let mut initial_url = String::new();
    let initial_bytes = art_bytes.unwrap_or_default();

    if !initial_bytes.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(track_key.as_bytes());
        hasher.update(&(initial_bytes.len() as u64).to_le_bytes());
        let digest = hasher.finalize();
        let short_hash = digest[..8]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        initial_url = format!("file:///velquo-macos-artwork/thumb-{}.jpg", short_hash);

        if let Ok(mut store) = album_art_store().lock() {
            if store.len() > 24 {
                store.clear();
            }
            store.insert(initial_url.clone(), initial_bytes.clone());
        }
    }

    if let Ok(mut last) = last_track_art_store().lock() {
        *last = Some(CachedAlbumArt {
            track_key: track_key.clone(),
            album_art_url: initial_url.clone(),
            bytes: initial_bytes,
            itunes_checked: false,
        });
    }

    // Stage 2: Spawn background 600x600 iTunes high-res lookup; once resolved, it updates
    // `LAST_TRACK_ART.album_art_url` to `hires-<hash>.jpg` so the 2s broadcaster re-streams it to Android!
    if !cc.title.is_empty() {
        spawn_itunes_hires_upgrade(track_key, cc.title.clone(), cc.artist.clone());
    }

    initial_url
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacPlayerSnapshot {
    pub player: String,
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_art_url: String,
    pub is_playing: bool,
    pub position_ms: i64,
    pub length_ms: i64,
    pub volume: i32,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_seek: bool,
}

impl MacPlayerSnapshot {
    pub fn to_mpris_packet(
        &self,
        include_now_playing: bool,
        include_volume: bool,
    ) -> NetworkPacket {
        let mut body = Map::new();
        body.insert("player".into(), json!(self.player));

        if include_now_playing {
            let now_playing = if !self.artist.is_empty() && !self.title.is_empty() {
                format!("{} - {}", self.artist, self.title)
            } else if !self.title.is_empty() {
                self.title.clone()
            } else {
                String::new()
            };

            body.insert("nowPlaying".into(), json!(now_playing));
            body.insert("title".into(), json!(self.title));
            body.insert("artist".into(), json!(self.artist));
            body.insert("album".into(), json!(self.album));
            body.insert("albumArtUrl".into(), json!(self.album_art_url));
            body.insert("isPlaying".into(), json!(self.is_playing));
            body.insert("pos".into(), json!(self.position_ms));
            body.insert("length".into(), json!(self.length_ms));
            body.insert("canPlay".into(), json!(self.can_play));
            body.insert("canPause".into(), json!(self.can_pause));
            body.insert("canGoNext".into(), json!(self.can_go_next));
            body.insert("canGoPrevious".into(), json!(self.can_go_previous));
            body.insert("canSeek".into(), json!(self.can_seek));
        }

        if include_volume && self.volume >= 0 {
            body.insert("volume".into(), json!(self.volume));
        }

        NetworkPacket::new(PACKET_TYPE_MPRIS, body)
    }
}

/// Queries which macOS media players are currently active in Control Center and returns `playerList`.
/// Returns an empty list `[]` when no media session is active, so no generic `"macOS Media"` placeholder appears.
pub fn get_mac_player_list() -> Vec<String> {
    let mut players = Vec::new();

    if let Some(cc) = query_control_center_now_playing(false) {
        if !cc.title.is_empty() || !cc.artist.is_empty() || cc.is_playing {
            let app = cc.app_name.trim();
            if !app.is_empty() {
                players.push(app.to_string());
            }
        }
    }

    if players.is_empty() {
        for app in [PLAYER_SPOTIFY, PLAYER_APPLE_MUSIC] {
            if let Some(snap) = query_app_player(app, app) {
                if snap.is_playing || !snap.title.is_empty() {
                    players.push(app.to_string());
                }
            }
        }
    }

    players
}

/// Queries the current playback metadata, position, duration, album art URL, and volume for the active Mac player.
/// Returns `None` if no media is currently active on macOS.
pub fn query_mac_player_status(requested_player: &str) -> Option<MacPlayerSnapshot> {
    let sys_vol = get_macos_system_volume().unwrap_or(50);

    // 1. Primary source: macOS Control Center Now Playing (via MediaRemote bridge)
    if let Some(cc) = query_control_center_now_playing(false) {
        if !cc.title.is_empty() || !cc.artist.is_empty() || cc.is_playing {
            let player_name = if !cc.app_name.trim().is_empty() {
                cc.app_name.trim().to_string()
            } else if !requested_player.is_empty() {
                requested_player.to_string()
            } else {
                return None;
            };

            let album_art_url = resolve_and_cache_artwork_for_cc(&cc);
            let title = if cc.title.is_empty() {
                player_name.clone()
            } else {
                cc.title.clone()
            };
            let length_ms = cc.duration_ms.max(0);
            let position_ms = cc.position_ms.max(0);

            return Some(MacPlayerSnapshot {
                player: player_name.clone(),
                app_name: player_name,
                title,
                artist: cc.artist,
                album: cc.album,
                album_art_url,
                is_playing: cc.is_playing,
                position_ms,
                length_ms,
                volume: sys_vol,
                can_play: true,
                can_pause: true,
                can_go_next: true,
                can_go_previous: true,
                can_seek: length_ms > 0,
            });
        }
    }

    // 2. Fallback: direct AppleScript query for Spotify or Apple Music
    for app in [PLAYER_SPOTIFY, PLAYER_APPLE_MUSIC] {
        if let Some(snap) = query_app_player(app, app) {
            if snap.is_playing || !snap.title.is_empty() {
                return Some(snap);
            }
        }
    }

    None
}

/// Automatically pauses any currently playing Mac media when a phone call starts (`ringing` or `talking`),
/// returning the list of player source names that were actively playing and paused by Velquo.
pub fn pause_mac_media_for_call() -> Vec<String> {
    let mut paused_sources = Vec::new();

    if let Some(cc) = query_control_center_now_playing(false) {
        if cc.is_playing {
            log::info!(
                "[pause-music] Pausing Mac Control Center player '{}' during phone call",
                cc.app_name
            );
            let _ = send_control_center_mr_command(Some(MR_CMD_PAUSE), None);
            paused_sources.push(DEFAULT_MAC_PLAYER.to_string());
            return paused_sources;
        }
    }

    for app in [PLAYER_SPOTIFY, PLAYER_APPLE_MUSIC] {
        if let Some(snap) = query_app_player(app, app) {
            if snap.is_playing {
                log::info!(
                    "[pause-music] Pausing Mac '{}' playback during phone call",
                    app
                );
                execute_mac_player_action(app, "Pause");
                paused_sources.push(app.to_string());
            }
        }
    }

    if paused_sources.is_empty() && is_macos_audio_output_running() {
        log::info!(
            "[pause-music] Active macOS audio output detected during phone call, sending hardware Play/Pause key"
        );
        send_macos_media_key(NX_KEYTYPE_PLAY);
        paused_sources.push(DEFAULT_MAC_PLAYER.to_string());
    }

    paused_sources
}

/// Resumes only the Mac media sources that were automatically paused by `pause_mac_media_for_call`
/// once the phone call ends (`isCancel: true`).
pub fn resume_mac_media_after_call(paused_sources: &[String]) {
    for source in paused_sources {
        match source.as_str() {
            PLAYER_SPOTIFY | PLAYER_APPLE_MUSIC => {
                log::info!(
                    "[pause-music] Resuming Mac '{}' playback after phone call ended",
                    source
                );
                execute_mac_player_action(source, "Play");
            }
            DEFAULT_MAC_PLAYER => {
                if !is_macos_audio_output_running() {
                    log::info!(
                        "[pause-music] Resuming macOS Control Center media playback after phone call ended"
                    );
                    if !send_control_center_mr_command(Some(MR_CMD_PLAY), None) {
                        send_macos_media_key(NX_KEYTYPE_PLAY);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Handles an incoming `kdeconnect.mpris.request` packet from a paired phone and returns any response packets to send back.
pub fn handle_incoming_mpris_request(packet: &NetworkPacket) -> Vec<NetworkPacket> {
    let mut responses = Vec::new();

    if packet.get_bool("requestPlayerList").unwrap_or(false) {
        let player_list = get_mac_player_list();
        let mut body = Map::new();
        body.insert("playerList".into(), json!(player_list));
        body.insert("supportAlbumArtPayload".into(), json!(true));
        responses.push(NetworkPacket::new(PACKET_TYPE_MPRIS, body));

        // Immediately send the current track metadata alongside `playerList` so that even if
        // Mac media is currently paused when the phone connects, Android's MprisPlugin populates
        // the player title, artist, and album art right away without waiting for Play.
        if packet.get_str("player").is_none() {
            if let Some(snap) = query_mac_player_status("") {
                responses.push(snap.to_mpris_packet(true, true));
            }
        }
    }

    let Some(player) = packet.get_str("player") else {
        return responses;
    };

    let req_now_playing_flag = packet.get_bool("requestNowPlaying").unwrap_or(false);
    let req_volume_flag = packet.get_bool("requestVolume").unwrap_or(false);
    let has_action = packet.body.contains_key("action");
    let has_set_pos = packet.body.contains_key("SetPosition");
    let has_seek = packet.body.contains_key("Seek");

    // Fast path for `setVolume`-only packets: apply volume in <0.1ms via CoreAudio without
    // spawning any subprocesses or echoing a redundant volume packet back while dragging!
    if let Some(vol) = packet.get_i64("setVolume") {
        let clamped = vol.clamp(0, 100) as i32;
        set_mac_player_volume(player, clamped);
        if !req_now_playing_flag && !req_volume_flag && !has_action && !has_set_pos && !has_seek {
            return responses;
        }
    }

    let known_players = get_mac_player_list();
    if !known_players.iter().any(|p| p == player) {
        let mut body = Map::new();
        body.insert("playerList".into(), json!(known_players));
        body.insert("supportAlbumArtPayload".into(), json!(true));
        responses.push(NetworkPacket::new(PACKET_TYPE_MPRIS, body));
        return responses;
    }

    let mut did_mutate = false;
    let mut expected_playing_override: Option<bool> = None;

    // 1. Playback actions: PlayPause, Play, Pause, Stop, Next, Previous
    if let Some(action) = packet.get_str("action") {
        log::info!(
            "[mpris-host] Executing action '{}' on Mac player '{}'",
            action,
            player
        );
        match action {
            "Play" => expected_playing_override = Some(true),
            "Pause" | "Stop" => expected_playing_override = Some(false),
            "PlayPause" => {
                if let Some(pre) = query_mac_player_status(player) {
                    expected_playing_override = Some(!pre.is_playing);
                }
            }
            _ => {}
        }
        execute_mac_player_action(player, action);
        // Allow browser/player Web MediaSession IPC to settle before reading updated state
        std::thread::sleep(std::time::Duration::from_millis(160));
        did_mutate = true;
    }

    // 2. Absolute seek position (`SetPosition` in milliseconds)
    if let Some(pos_ms) = packet.get_i64("SetPosition") {
        log::info!(
            "[mpris-host] Setting position to {}ms on Mac player '{}'",
            pos_ms,
            player
        );
        set_mac_player_position_ms(player, pos_ms.max(0));
        std::thread::sleep(std::time::Duration::from_millis(100));
        did_mutate = true;
    }

    // 3. Relative seek offset (`Seek` in microseconds, e.g. ±10_000_000 for ±10s)
    if let Some(offset_us) = packet.get_i64("Seek") {
        log::info!(
            "[mpris-host] Relative seek {}us on Mac player '{}'",
            offset_us,
            player
        );
        seek_mac_player_offset_us(player, offset_us);
        std::thread::sleep(std::time::Duration::from_millis(100));
        did_mutate = true;
    }

    let req_now_playing = req_now_playing_flag || did_mutate;
    let req_volume = req_volume_flag;

    if req_now_playing || req_volume {
        if let Some(mut snap) = query_mac_player_status(player) {
            snap.player = player.to_string();
            if let Some(override_playing) = expected_playing_override {
                snap.is_playing = override_playing;
            }
            responses.push(snap.to_mpris_packet(req_now_playing, req_volume));
        }
    }

    responses
}

fn query_app_player(app_name: &str, reported_player_name: &str) -> Option<MacPlayerSnapshot> {
    let is_spotify = app_name == PLAYER_SPOTIFY;
    let duration_expr = if is_spotify {
        "set trackLenMs to (duration of current track) as integer"
    } else {
        "set trackLenMs to ((duration of current track) * 1000) as integer"
    };

    let script = format!(
        r#"
tell application "System Events"
    if not (exists (processes where name is "{app}")) then
        return ""
    end if
end tell
tell application "{app}"
    try
        set pState to (player state as string)
        set isPlayingVal to "false"
        if pState is "playing" then
            set isPlayingVal to "true"
        end if
        set volVal to (sound volume as integer)
        if pState is "stopped" then
            return isPlayingVal & "\t" & volVal & "\t0\t0\t\t\t"
        end if
        set tTitle to (name of current track as string)
        set tArtist to (artist of current track as string)
        set tAlbum to (album of current track as string)
        set posMs to ((player position) * 1000) as integer
        {duration_expr}
        return isPlayingVal & "\t" & volVal & "\t" & posMs & "\t" & trackLenMs & "\t" & tTitle & "\t" & tArtist & "\t" & tAlbum
    on error
        return ""
    end try
end tell
"#,
        app = app_name,
        duration_expr = duration_expr
    );

    let out = run_osascript(&script)?;
    if out.is_empty() {
        return None;
    }

    let parts: Vec<&str> = out.splitn(7, '\t').collect();
    if parts.len() < 7 {
        return None;
    }

    let is_playing = parts[0].trim() == "true";
    let volume = parts[1].trim().parse::<i32>().unwrap_or(50).clamp(0, 100);
    let position_ms = parts[2].trim().parse::<i64>().unwrap_or(0).max(0);
    let length_ms = parts[3].trim().parse::<i64>().unwrap_or(0).max(0);
    let title = parts[4].trim().to_string();
    let artist = parts[5].trim().to_string();
    let album = parts[6].trim().to_string();

    Some(MacPlayerSnapshot {
        player: reported_player_name.to_string(),
        app_name: app_name.to_string(),
        title: title.clone(),
        artist,
        album,
        album_art_url: String::new(),
        is_playing,
        position_ms,
        length_ms,
        volume,
        can_play: true,
        can_pause: true,
        can_go_next: true,
        can_go_previous: true,
        can_seek: length_ms > 0 && !title.is_empty(),
    })
}

fn execute_mac_player_action(player: &str, action: &str) {
    // 1. Send directly to Control Center's active Now Playing app via MediaRemote
    let mr_cmd = match action {
        "Play" => Some(MR_CMD_PLAY),
        "Pause" => Some(MR_CMD_PAUSE),
        "PlayPause" => Some(MR_CMD_TOGGLE_PLAY_PAUSE),
        "Stop" => Some(MR_CMD_STOP),
        "Next" => Some(MR_CMD_NEXT_TRACK),
        "Previous" => Some(MR_CMD_PREVIOUS_TRACK),
        _ => None,
    };

    if let Some(cmd_id) = mr_cmd {
        if send_control_center_mr_command(Some(cmd_id), None) {
            return;
        }
    }

    // 2. Fallback to AppleScript if specifically targeting Spotify / Music
    if player == PLAYER_SPOTIFY || player == PLAYER_APPLE_MUSIC {
        let apple_cmd = match action {
            "PlayPause" => "playpause",
            "Play" => "play",
            "Pause" | "Stop" => "pause",
            "Next" => "next track",
            "Previous" => "previous track",
            _ => return,
        };
        let script = format!(r#"tell application "{}" to {}"#, player, apple_cmd);
        let _ = run_osascript(&script);
        return;
    }

    // 3. Hardware media key fallback
    let media_key = match action {
        "PlayPause" | "Play" | "Pause" | "Stop" => Some(NX_KEYTYPE_PLAY),
        "Next" => Some(NX_KEYTYPE_NEXT),
        "Previous" => Some(NX_KEYTYPE_PREVIOUS),
        _ => None,
    };

    if let Some(key) = media_key {
        send_macos_media_key(key);
    }
}

fn set_mac_player_position_ms(player: &str, pos_ms: i64) {
    if send_control_center_mr_command(None, Some(pos_ms.max(0))) {
        return;
    }

    let secs = (pos_ms as f64) / 1000.0;
    if player == PLAYER_SPOTIFY || player == PLAYER_APPLE_MUSIC {
        let script = format!(
            r#"tell application "{}" to set player position to {:.3}"#,
            player, secs
        );
        let _ = run_osascript(&script);
    }
}

fn seek_mac_player_offset_us(player: &str, offset_us: i64) {
    if let Some(cc) = query_control_center_now_playing(false) {
        let target_ms = (cc.position_ms + (offset_us / 1000)).clamp(0, cc.duration_ms.max(0));
        if send_control_center_mr_command(None, Some(target_ms)) {
            return;
        }
    }

    let offset_secs = (offset_us as f64) / 1_000_000.0;
    if player == PLAYER_SPOTIFY || player == PLAYER_APPLE_MUSIC {
        let script = format!(
            r#"tell application "{}" to set player position to ((player position) + ({:.3}))"#,
            player, offset_secs
        );
        let _ = run_osascript(&script);
    } else if offset_us > 0 {
        send_macos_media_key(NX_KEYTYPE_FAST);
    } else if offset_us < 0 {
        send_macos_media_key(NX_KEYTYPE_REWIND);
    }
}

#[cfg(target_os = "macos")]
fn get_default_output_audio_device_id() -> Option<u32> {
    const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
    const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

    unsafe {
        let mut default_device_id: u32 = 0;
        let mut data_size = std::mem::size_of::<u32>() as u32;
        let default_dev_addr = AudioObjectPropertyAddress {
            m_selector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let status = AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &default_dev_addr,
            0,
            std::ptr::null(),
            &mut data_size,
            &mut default_device_id as *mut u32 as *mut c_void,
        );
        if status == 0 && default_device_id != 0 {
            Some(default_device_id)
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn set_mac_player_volume(_player: &str, volume: i32) {
    const K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME: u32 =
        u32::from_be_bytes(*b"vmvc");
    const K_AUDIO_DEVICE_PROPERTY_MUTE: u32 = u32::from_be_bytes(*b"mute");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

    let clamped = volume.clamp(0, 100);
    let scalar: f32 = (clamped as f32) / 100.0;

    if let Some(dev_id) = get_default_output_audio_device_id() {
        unsafe {
            let vol_addr = AudioObjectPropertyAddress {
                m_selector: K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
                m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
                m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            };
            let mute_addr = AudioObjectPropertyAddress {
                m_selector: K_AUDIO_DEVICE_PROPERTY_MUTE,
                m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
                m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            };

            let mut mute_val: u32 = if clamped == 0 { 1 } else { 0 };
            let _ = AudioObjectSetPropertyData(
                dev_id,
                &mute_addr,
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                &mut mute_val as *mut u32 as *const c_void,
            );

            let status = AudioObjectSetPropertyData(
                dev_id,
                &vol_addr,
                0,
                std::ptr::null(),
                std::mem::size_of::<f32>() as u32,
                &scalar as *const f32 as *const c_void,
            );
            if status == 0 {
                return;
            }
        }
    }

    let script = format!(r#"set volume output volume {}"#, clamped);
    let _ = run_osascript(&script);
}

#[cfg(not(target_os = "macos"))]
fn set_mac_player_volume(_player: &str, _volume: i32) {}

#[cfg(target_os = "macos")]
fn get_macos_system_volume() -> Option<i32> {
    const K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME: u32 =
        u32::from_be_bytes(*b"vmvc");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

    if let Some(dev_id) = get_default_output_audio_device_id() {
        unsafe {
            let mut scalar: f32 = 0.0;
            let mut data_size = std::mem::size_of::<f32>() as u32;
            let vol_addr = AudioObjectPropertyAddress {
                m_selector: K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
                m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
                m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            };
            let status = AudioObjectGetPropertyData(
                dev_id,
                &vol_addr,
                0,
                std::ptr::null(),
                &mut data_size,
                &mut scalar as *mut f32 as *mut c_void,
            );
            if status == 0 {
                return Some(((scalar * 100.0).round() as i32).clamp(0, 100));
            }
        }
    }

    let out = run_osascript("output volume of (get volume settings)")?;
    out.trim().parse::<i32>().ok()
}

#[cfg(not(target_os = "macos"))]
fn get_macos_system_volume() -> Option<i32> {
    Some(50)
}

fn run_osascript(script: &str) -> Option<String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(&['\r', '\n'][..])
            .to_string(),
    )
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct AudioObjectPropertyAddress {
    m_selector: u32,
    m_scope: u32,
    m_element: u32,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreAudio", kind = "framework")]
#[link(name = "AudioToolbox", kind = "framework")]
#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        in_object_id: u32,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        in_object_id: u32,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        in_data_size: u32,
        in_data: *const c_void,
    ) -> i32;
    fn objc_getClass(name: *const libc::c_char) -> *mut c_void;
    fn sel_registerName(name: *const libc::c_char) -> *const c_void;
    fn objc_msgSend();
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

/// Checks via macOS `CoreAudio.framework` (`kAudioDevicePropertyDeviceIsRunningSomewhere`)
/// whether the default output audio device is actively rendering audio from any application.
#[cfg(target_os = "macos")]
pub fn is_macos_audio_output_running() -> bool {
    const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
    const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut");
    const K_AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE: u32 = u32::from_be_bytes(*b"gone");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

    unsafe {
        let mut default_device_id: u32 = 0;
        let mut data_size = std::mem::size_of::<u32>() as u32;
        let default_dev_addr = AudioObjectPropertyAddress {
            m_selector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let status = AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &default_dev_addr,
            0,
            std::ptr::null(),
            &mut data_size,
            &mut default_device_id as *mut u32 as *mut c_void,
        );
        if status != 0 || default_device_id == 0 {
            return false;
        }

        let mut is_running: u32 = 0;
        let mut running_size = std::mem::size_of::<u32>() as u32;
        let running_addr = AudioObjectPropertyAddress {
            m_selector: K_AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE,
            m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let run_status = AudioObjectGetPropertyData(
            default_device_id,
            &running_addr,
            0,
            std::ptr::null(),
            &mut running_size,
            &mut is_running as *mut u32 as *mut c_void,
        );
        run_status == 0 && is_running != 0
    }
}

#[cfg(not(target_os = "macos"))]
pub fn is_macos_audio_output_running() -> bool {
    false
}

/// Posts a system-wide macOS media key event (`NX_SUBTYPE_AUX_CONTROL_BUTTONS`) via `NSEvent` + `CGEventPost`,
/// matching `kdeconnect-kde`'s `mpriscontrolplugin-macos.mm`.
#[cfg(target_os = "macos")]
fn send_macos_media_key(key: i32) {
    unsafe {
        if !CHECKED_AX_TRUST.swap(true, Ordering::SeqCst) {
            let trusted = AXIsProcessTrustedWithOptions(std::ptr::null());
            if !trusted {
                log::warn!(
                    "[mpris-host] Accessibility permission not yet granted to Velquo; prompting user via System Settings if needed"
                );
                let _ = Command::new("/usr/bin/osascript")
                    .arg("-e")
                    .arg(r#"tell application "System Events" to key code 0 using {}"#)
                    .output();
            }
        }

        let ns_event_cls = objc_getClass(c"NSEvent".as_ptr());
        if ns_event_cls.is_null() {
            return;
        }

        let other_event_sel = sel_registerName(
            c"otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:"
                .as_ptr(),
        );
        let cg_event_sel = sel_registerName(c"CGEvent".as_ptr());

        type OtherEventFn = unsafe extern "C" fn(
            *mut c_void,
            *const c_void,
            usize,
            NSPoint,
            usize,
            f64,
            isize,
            *mut c_void,
            i16,
            isize,
            isize,
        ) -> *mut c_void;

        type GetCGEventFn = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;

        let make_other_event: OtherEventFn = std::mem::transmute(objc_msgSend as *const c_void);
        let get_cg_event: GetCGEventFn = std::mem::transmute(objc_msgSend as *const c_void);

        // NSEventTypeSystemDefined = 14, subtype = 8 (NX_SUBTYPE_AUX_CONTROL_BUTTONS)
        for down in [true, false] {
            let flags: usize = if down { 0xa00 } else { 0xb00 };
            let data1: isize = ((key as isize) << 16) | (flags as isize);
            let ns_ev = make_other_event(
                ns_event_cls,
                other_event_sel,
                14,
                NSPoint { x: 0.0, y: 0.0 },
                flags,
                0.0,
                0,
                std::ptr::null_mut(),
                8,
                data1,
                -1,
            );
            if !ns_ev.is_null() {
                let cg_ev = get_cg_event(ns_ev, cg_event_sel);
                if !cg_ev.is_null() {
                    // kCGHIDEventTap = 0
                    CGEventPost(0, cg_ev);
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn send_macos_media_key(_key: i32) {}
