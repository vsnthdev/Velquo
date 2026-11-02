use crate::kdeconnect::daemon::*;
use crate::kdeconnect::macos_media::{
    disown_call_paused_media, pause_mac_media_for_call, resume_mac_media_after_call,
};
use crate::kdeconnect::packet::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

impl KdeConnectDaemon {
    pub(super) fn handle_telephony_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        if !self.is_device_pause_media_on_calls_allowed(device_id) {
            return;
        }

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
                // Only run `pause_mac_media_for_call` if we haven't already paused Mac media for this call
                // (prevents a `ringing` -> `talking` transition from sending a second pause!)
                let already_paused_for_this_call = {
                    let mut state = self.call_media_state.lock().unwrap();
                    if state.is_pause_expired() {
                        state.active_events.clear();
                        state.paused_sources.clear();
                        state.paused_at = None;
                        state.paused_at_secs = None;
                        disown_call_paused_media("previous call pause expired (> 1 hour)");
                    }
                    state
                        .active_events
                        .insert(format!("{}:{}", device_id, event));
                    state.seq = state.seq.wrapping_add(1);
                    !state.paused_sources.is_empty()
                };
                if !already_paused_for_this_call {
                    let daemon = Arc::clone(self);
                    tokio::spawn(async move {
                        if let Ok(newly_paused) =
                            tokio::task::spawn_blocking(pause_mac_media_for_call).await
                        {
                            if !newly_paused.is_empty() {
                                let mut state = daemon.call_media_state.lock().unwrap();
                                if state.paused_sources.is_empty() {
                                    state.paused_at = Some(Instant::now());
                                    state.paused_at_secs = Some(current_secs());
                                }
                                for src in newly_paused {
                                    state.paused_sources.insert(src);
                                }
                            }
                        }
                    });
                }
            }
            "ringing" | "talking" if is_cancel => {
                let (all_calls_ended, snapshot_seq) = {
                    let mut state = self.call_media_state.lock().unwrap();
                    state
                        .active_events
                        .remove(&format!("{}:{}", device_id, event));
                    if event == "talking" {
                        state
                            .active_events
                            .remove(&format!("{}:ringing", device_id));
                    }
                    state.seq = state.seq.wrapping_add(1);
                    (state.active_events.is_empty(), state.seq)
                };

                if all_calls_ended {
                    self.schedule_call_media_resume(snapshot_seq);
                }
            }
            "missedCall" if !is_cancel => {
                let (all_calls_ended, snapshot_seq) = {
                    let mut state = self.call_media_state.lock().unwrap();
                    state
                        .active_events
                        .remove(&format!("{}:ringing", device_id));
                    state.seq = state.seq.wrapping_add(1);
                    (state.active_events.is_empty(), state.seq)
                };

                if all_calls_ended {
                    self.schedule_call_media_resume(snapshot_seq);
                }
            }
            _ => {}
        }
    }

    pub(super) fn schedule_call_media_resume(self: &Arc<Self>, snapshot_seq: u64) {
        let daemon = Arc::clone(self);
        tokio::spawn(async move {
            // Brief 450ms debounce so a `ringing (isCancel=true)` -> `talking` transition
            // when answering a call does not momentarily unpause media.
            tokio::time::sleep(Duration::from_millis(450)).await;
            let (sources_to_resume, elapsed) = {
                let mut state = daemon.call_media_state.lock().unwrap();
                if !state.active_events.is_empty() || state.seq != snapshot_seq {
                    return;
                }
                let elapsed = state.pause_elapsed();
                state.paused_at = None;
                state.paused_at_secs = None;
                let sources: Vec<String> = state.paused_sources.drain().collect();
                (sources, elapsed)
            };

            if sources_to_resume.is_empty() {
                return;
            }

            if let Some(paused_duration) = elapsed {
                if paused_duration > MAX_CALL_PAUSE_RESUME_DURATION {
                    log::info!(
                        "[pause-music] Skipping auto-resume after phone call ended: media was paused for {}s (> 1 hour)",
                        paused_duration.as_secs()
                    );
                    disown_call_paused_media("call duration exceeded 1 hour");
                    return;
                }
            }

            let _ = tokio::task::spawn_blocking(move || {
                resume_mac_media_after_call(&sources_to_resume);
            })
            .await;
        });
    }
}
