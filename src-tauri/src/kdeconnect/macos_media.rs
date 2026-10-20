use crate::kdeconnect::packet::{NetworkPacket, PACKET_TYPE_MPRIS};
use serde_json::{json, Map};
use std::ffi::c_void;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DEFAULT_MAC_PLAYER: &str = "macOS Media";
pub const PLAYER_SPOTIFY: &str = "Spotify";
pub const PLAYER_APPLE_MUSIC: &str = "Music";

// IOKit/hidsystem/ev_keymap.h media key constants
const NX_KEYTYPE_PLAY: i32 = 16;
const NX_KEYTYPE_NEXT: i32 = 17;
const NX_KEYTYPE_PREVIOUS: i32 = 18;
const NX_KEYTYPE_FAST: i32 = 19;
const NX_KEYTYPE_REWIND: i32 = 20;

static CHECKED_AX_TRUST: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacPlayerSnapshot {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
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
    pub fn to_mpris_packet(&self, include_now_playing: bool, include_volume: bool) -> NetworkPacket {
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

/// Queries which macOS media players are currently available and returns the ordered `playerList`.
/// Always includes `"macOS Media"` first so system-wide media playback (Chrome, Safari, Arc, Spotify, Music, VLC, etc.) is controllable.
pub fn get_mac_player_list() -> Vec<String> {
    let mut players = vec![DEFAULT_MAC_PLAYER.to_string()];

    let script = r#"
set runningList to ""
tell application "System Events"
    if exists (processes where name is "Spotify") then
        set runningList to runningList & "Spotify,"
    end if
    if exists (processes where name is "Music") then
        set runningList to runningList & "Music,"
    end if
end tell
return runningList
"#;

    if let Some(out) = run_osascript(script) {
        for part in out.split(',') {
            let trimmed = part.trim();
            if (trimmed == PLAYER_SPOTIFY || trimmed == PLAYER_APPLE_MUSIC)
                && !players.iter().any(|p| p == trimmed)
            {
                players.push(trimmed.to_string());
            }
        }
    }

    players
}

/// Queries the current playback metadata, position, duration, and volume for the specified Mac player.
pub fn query_mac_player_status(requested_player: &str) -> MacPlayerSnapshot {
    match requested_player {
        PLAYER_SPOTIFY => {
            if let Some(snap) = query_app_player(PLAYER_SPOTIFY, PLAYER_SPOTIFY) {
                return snap;
            }
        }
        PLAYER_APPLE_MUSIC => {
            if let Some(snap) = query_app_player(PLAYER_APPLE_MUSIC, PLAYER_APPLE_MUSIC) {
                return snap;
            }
        }
        _ => {
            // For "macOS Media", prefer whichever native music app is actively playing (or has a loaded track)
            let spotify_snap = query_app_player(PLAYER_SPOTIFY, DEFAULT_MAC_PLAYER);
            let music_snap = query_app_player(PLAYER_APPLE_MUSIC, DEFAULT_MAC_PLAYER);

            if let Some(ref s) = spotify_snap {
                if s.is_playing {
                    return s.clone();
                }
            }
            if let Some(ref m) = music_snap {
                if m.is_playing {
                    return m.clone();
                }
            }
            if let Some(s) = spotify_snap {
                if !s.title.is_empty() {
                    return s;
                }
            }
            if let Some(m) = music_snap {
                if !m.title.is_empty() {
                    return m;
                }
            }
        }
    }

    let sys_vol = get_macos_system_volume().unwrap_or(50);
    MacPlayerSnapshot {
        player: if requested_player.is_empty() {
            DEFAULT_MAC_PLAYER.to_string()
        } else {
            requested_player.to_string()
        },
        title: String::new(),
        artist: String::new(),
        album: String::new(),
        is_playing: false,
        position_ms: 0,
        length_ms: 0,
        volume: sys_vol,
        can_play: true,
        can_pause: true,
        can_go_next: true,
        can_go_previous: true,
        can_seek: true,
    }
}

/// Handles an incoming `kdeconnect.mpris.request` packet from a paired phone and returns any response packets to send back.
pub fn handle_incoming_mpris_request(packet: &NetworkPacket) -> Vec<NetworkPacket> {
    let mut responses = Vec::new();

    if packet.get_bool("requestPlayerList").unwrap_or(false) {
        let player_list = get_mac_player_list();
        let mut body = Map::new();
        body.insert("playerList".into(), json!(player_list));
        body.insert("supportAlbumArtPayload".into(), json!(false));
        responses.push(NetworkPacket::new(PACKET_TYPE_MPRIS, body));
    }

    let Some(player) = packet.get_str("player") else {
        return responses;
    };

    let known_players = get_mac_player_list();
    if !known_players.iter().any(|p| p == player) {
        let mut body = Map::new();
        body.insert("playerList".into(), json!(known_players));
        body.insert("supportAlbumArtPayload".into(), json!(false));
        responses.push(NetworkPacket::new(PACKET_TYPE_MPRIS, body));
        return responses;
    }

    let mut did_mutate = false;

    // 1. Playback actions: PlayPause, Play, Pause, Stop, Next, Previous
    if let Some(action) = packet.get_str("action") {
        log::info!(
            "[mpris-host] Executing action '{}' on Mac player '{}'",
            action,
            player
        );
        execute_mac_player_action(player, action);
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
        did_mutate = true;
    }

    // 4. Volume control (`setVolume` 0..=100)
    if let Some(vol) = packet.get_i64("setVolume") {
        let clamped = vol.clamp(0, 100) as i32;
        log::info!(
            "[mpris-host] Setting volume to {}% on Mac player '{}'",
            clamped,
            player
        );
        set_mac_player_volume(player, clamped);
        did_mutate = true;
    }

    let req_now_playing = packet.get_bool("requestNowPlaying").unwrap_or(false) || did_mutate;
    let req_volume = packet.get_bool("requestVolume").unwrap_or(false)
        || packet.body.contains_key("setVolume");

    if req_now_playing || req_volume {
        let snap = query_mac_player_status(player);
        responses.push(snap.to_mpris_packet(req_now_playing, req_volume));
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
        title: title.clone(),
        artist,
        album,
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
    let target_app = match player {
        PLAYER_SPOTIFY => Some(PLAYER_SPOTIFY),
        PLAYER_APPLE_MUSIC => Some(PLAYER_APPLE_MUSIC),
        _ => None,
    };

    if let Some(app) = target_app {
        let apple_cmd = match action {
            "PlayPause" => "playpause",
            "Play" => "play",
            "Pause" | "Stop" => "pause",
            "Next" => "next track",
            "Previous" => "previous track",
            _ => return,
        };
        let script = format!(r#"tell application "{}" to {}"#, app, apple_cmd);
        let _ = run_osascript(&script);
        return;
    }

    // For "macOS Media", if Spotify or Apple Music is actively playing, control it directly via AppleScript
    for app in [PLAYER_SPOTIFY, PLAYER_APPLE_MUSIC] {
        if let Some(snap) = query_app_player(app, app) {
            if snap.is_playing {
                execute_mac_player_action(app, action);
                return;
            }
        }
    }

    // Otherwise dispatch the native macOS NX_SUBTYPE_AUX_CONTROL_BUTTONS media key event
    // (matches kdeconnect-kde's mpriscontrolplugin-macos.mm `sendMediaKey`, controlling Chrome, Safari, Arc, VLC, etc.)
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
    let secs = (pos_ms as f64) / 1000.0;
    let target_app = match player {
        PLAYER_SPOTIFY => Some(PLAYER_SPOTIFY),
        PLAYER_APPLE_MUSIC => Some(PLAYER_APPLE_MUSIC),
        _ => {
            if query_app_player(PLAYER_SPOTIFY, PLAYER_SPOTIFY)
                .map(|s| s.is_playing || !s.title.is_empty())
                .unwrap_or(false)
            {
                Some(PLAYER_SPOTIFY)
            } else if query_app_player(PLAYER_APPLE_MUSIC, PLAYER_APPLE_MUSIC)
                .map(|s| s.is_playing || !s.title.is_empty())
                .unwrap_or(false)
            {
                Some(PLAYER_APPLE_MUSIC)
            } else {
                None
            }
        }
    };

    if let Some(app) = target_app {
        let script = format!(
            r#"tell application "{}" to set player position to {:.3}"#,
            app, secs
        );
        let _ = run_osascript(&script);
    }
}

fn seek_mac_player_offset_us(player: &str, offset_us: i64) {
    let offset_secs = (offset_us as f64) / 1_000_000.0;
    let target_app = match player {
        PLAYER_SPOTIFY => Some(PLAYER_SPOTIFY),
        PLAYER_APPLE_MUSIC => Some(PLAYER_APPLE_MUSIC),
        _ => {
            if query_app_player(PLAYER_SPOTIFY, PLAYER_SPOTIFY)
                .map(|s| s.is_playing || !s.title.is_empty())
                .unwrap_or(false)
            {
                Some(PLAYER_SPOTIFY)
            } else if query_app_player(PLAYER_APPLE_MUSIC, PLAYER_APPLE_MUSIC)
                .map(|s| s.is_playing || !s.title.is_empty())
                .unwrap_or(false)
            {
                Some(PLAYER_APPLE_MUSIC)
            } else {
                None
            }
        }
    };

    if let Some(app) = target_app {
        let script = format!(
            r#"tell application "{}" to set player position to ((player position) + ({:.3}))"#,
            app, offset_secs
        );
        let _ = run_osascript(&script);
    } else if offset_us > 0 {
        send_macos_media_key(NX_KEYTYPE_FAST);
    } else if offset_us < 0 {
        send_macos_media_key(NX_KEYTYPE_REWIND);
    }
}

fn set_mac_player_volume(player: &str, volume: i32) {
    match player {
        PLAYER_SPOTIFY | PLAYER_APPLE_MUSIC => {
            let script = format!(
                r#"tell application "{}" to set sound volume to {}"#,
                player, volume
            );
            let _ = run_osascript(&script);
        }
        _ => {
            let script = format!(r#"set volume output volume {}"#, volume);
            let _ = run_osascript(&script);
        }
    }
}

fn get_macos_system_volume() -> Option<i32> {
    let out = run_osascript("output volume of (get volume settings)")?;
    out.trim().parse::<i32>().ok()
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
    Some(String::from_utf8_lossy(&output.stdout).trim_end_matches(&['\r', '\n'][..]).to_string())
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    fn objc_getClass(name: *const libc::c_char) -> *mut c_void;
    fn sel_registerName(name: *const libc::c_char) -> *const c_void;
    fn objc_msgSend();
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
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
