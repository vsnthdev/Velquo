use crate::kdeconnect::clipboard::*;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

impl KdeConnectDaemon {
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
    /// explicit whole-word or multi-word OTP/2FA verification context.
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

    pub(super) fn handle_clipboard_packet(
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
}

pub(super) fn spawn_clipboard_monitor(daemon: Arc<KdeConnectDaemon>) {
    tokio::spawn(async move {
        let mut clip_interval = tokio::time::interval(Duration::from_millis(500));
        clip_interval.tick().await;
        loop {
            clip_interval.tick().await;
            let current_count = get_macos_pasteboard_change_count();
            let count_changed = {
                let state = daemon.clipboard_state.lock().unwrap();
                current_count != state.last_change_count
            };
            if !count_changed {
                continue;
            }

            {
                let mut state = daemon.clipboard_state.lock().unwrap();
                state.last_change_count = current_count;
            }

            if !daemon.has_online_clipboard_sync_device() {
                continue;
            }

            if let Some(text) = read_macos_clipboard_text() {
                let should_broadcast = {
                    let mut state = daemon.clipboard_state.lock().unwrap();
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
                    daemon.broadcast_clipboard_to_synced_devices(&pkt);
                }
            }
        }
    });
}
