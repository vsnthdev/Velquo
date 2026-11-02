use crate::kdeconnect::daemon::*;
use crate::kdeconnect::macos_media::{
    get_mac_player_list, last_broadcast_sig_store, mac_media_notify, query_mac_player_status,
    start_macos_media_bridge,
};
use crate::kdeconnect::packet::*;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

impl KdeConnectDaemon {
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
}

pub(crate) fn spawn_media_broadcaster(daemon: Arc<KdeConnectDaemon>) {
    start_macos_media_bridge();
    let media_notify = mac_media_notify();
    tokio::spawn(async move {
        loop {
            media_notify.notified().await;

            let online_paired_ids: Vec<String> = {
                let devices = daemon.devices.lock().unwrap();
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
                    let _ = daemon.send_packet_to_device(id, &list_pkt);
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
                    let _ = daemon.send_packet_to_device(id, &pkt);
                }
            }
        }
    });
}
