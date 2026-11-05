use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// If the phone silently drops an album-art request, allow a fresh retry after this long.
const ALBUM_ART_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

fn is_album_art_request_fresh(requested_at: Option<&Instant>, now: Instant) -> bool {
    requested_at.is_some_and(|requested_at| {
        now.saturating_duration_since(*requested_at) < ALBUM_ART_REQUEST_TIMEOUT
    })
}

impl KdeConnectDaemon {
    pub(super) fn recompute_active_mpris(entry: &mut DeviceEntry) {
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
}

pub(super) fn handle_player_metadata_update(
    daemon: &Arc<KdeConnectDaemon>,
    device_id: &str,
    device_name: &str,
    packet: &NetworkPacket,
) {
    let Some(player) = packet.get_str("player") else {
        return;
    };
    let mut art_to_request: Option<(String, String)> = None;

    {
        let mut devices = daemon.devices.lock().unwrap();
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
        // Only expose artwork that actually belongs to the current `album_art_url`.
        // Retaining the previous track's cover here would make the UI treat it as native
        // artwork for the new track (skipping the iTunes fallback), and if the phone never
        // answers `mpris_request_album_art` the stale cover would stay stuck forever.
        let album_art_data = if let Some(ref url) = album_art_url {
            if let Some(cached) = entry.album_art_cache.get(url) {
                Some(cached.clone())
            } else {
                let now = Instant::now();
                let request_is_fresh =
                    is_album_art_request_fresh(entry.album_art_in_flight.get(url), now);

                if !request_is_fresh {
                    entry.album_art_in_flight.insert(url.clone(), now);
                    art_to_request = Some((player.to_string(), url.clone()));
                }

                // Drop expired requests so the map cannot grow across tracks.
                entry
                    .album_art_in_flight
                    .retain(|_, requested_at| is_album_art_request_fresh(Some(requested_at), now));

                let same_art_url = prev_player
                    .as_ref()
                    .is_some_and(|p| p.album_art_url.as_deref() == Some(url.as_str()));
                if same_art_url {
                    prev_player.as_ref().and_then(|p| p.album_art_data.clone())
                } else {
                    None
                }
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
        let can_go_previous = packet.get_bool("canGoPrevious").unwrap_or_else(|| {
            prev_player
                .as_ref()
                .map(|p| p.can_go_previous)
                .unwrap_or(true)
        });
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
                "[mpris] Player update on '{}' ({}): isPlaying={}, hasTitle={}, hasArt={}",
                device_name,
                player,
                is_playing,
                !title.is_empty(),
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
        KdeConnectDaemon::recompute_active_mpris(entry);
    }

    if let Some((req_player, req_url)) = art_to_request {
        log::info!(
            "[mpris] Requesting album art from '{}' for player '{}' ({} chars)",
            device_name,
            req_player,
            req_url.len()
        );
        if let Err(e) = daemon.send_packet_to_device(
            device_id,
            &NetworkPacket::mpris_request_album_art(&req_player, &req_url),
        ) {
            log::warn!(
                "[mpris] Failed to request album art for player '{}' from '{}': {}",
                req_player,
                device_name,
                e
            );
            // Do not leave the URL marked as in-flight: the next metadata update should
            // be able to retry immediately. Also clear the failed URL and any retained
            // artwork so the UI stops waiting for native art and falls back right away.
            if let Ok(mut devices) = daemon.devices.lock() {
                if let Some(entry) = devices.get_mut(device_id) {
                    entry.album_art_in_flight.remove(&req_url);
                    for p_info in entry.mpris_players.values_mut() {
                        if p_info.album_art_url.as_deref() == Some(req_url.as_str()) {
                            p_info.album_art_url = None;
                            p_info.album_art_data = None;
                        }
                    }
                    KdeConnectDaemon::recompute_active_mpris(entry);
                }
            }
        }
    }
    daemon.notify_ui();
}

#[cfg(test)]
mod tests {
    use super::{is_album_art_request_fresh, ALBUM_ART_REQUEST_TIMEOUT};
    use std::time::{Duration, Instant};

    #[test]
    fn album_art_request_is_fresh_within_timeout() {
        let now = Instant::now();
        assert!(is_album_art_request_fresh(Some(&now), now));
    }

    #[test]
    fn album_art_request_expires_after_timeout() {
        let now = Instant::now();
        let expired = now - ALBUM_ART_REQUEST_TIMEOUT - Duration::from_millis(1);
        assert!(!is_album_art_request_fresh(Some(&expired), now));
    }

    #[test]
    fn missing_album_art_request_is_not_fresh() {
        assert!(!is_album_art_request_fresh(None, Instant::now()));
    }
}
