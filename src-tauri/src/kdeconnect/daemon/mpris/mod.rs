mod album_art;
pub(super) mod control;
mod metadata;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use std::net::IpAddr;
use std::sync::Arc;

impl KdeConnectDaemon {
    pub(super) fn handle_mpris_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        packet: &NetworkPacket,
    ) {
        if !self.is_device_paired(device_id) {
            log::debug!(
                "[mpris] Ignoring packet from unpaired device '{}' ({})",
                device_name,
                device_id
            );
            return;
        }

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
            let player = packet.get_str("player").unwrap_or("").to_string();
            let payload_size = packet.payload_size.unwrap_or(0).max(0) as usize;
            if payload_size > 8 * 1024 * 1024 {
                log::warn!(
                    "[mpris] Album art payload too large ({} bytes), ignoring",
                    payload_size
                );
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
                                entry.album_art_cache.clear();
                                entry
                                    .album_art_cache
                                    .insert(album_art_url.clone(), data_url.clone());

                                // Attach to any matching player entry
                                for p_info in entry.mpris_players.values_mut() {
                                    if p_info.album_art_url.as_deref()
                                        == Some(album_art_url.as_str())
                                    {
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
        metadata::handle_player_metadata_update(self, device_id, device_name, packet);
    }
}
