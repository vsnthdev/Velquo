use crate::kdeconnect::cert::*;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::Result;

impl KdeConnectDaemon {
    pub(super) fn handle_pair_packet(
        &self,
        device_id: &str,
        device_name: &str,
        peer_cert_der: &[u8],
        packet: &NetworkPacket,
    ) {
        let wants_pair = packet.get_bool("pair").unwrap_or(false);
        let timestamp = packet.get_i64("timestamp").unwrap_or_else(current_secs);
        let existing_ssh_host_key = self.trusted_devices.lock().ok().and_then(|trusted| {
            trusted
                .get(device_id)
                .and_then(|record| record.ssh_host_key_fingerprint.clone())
        });

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
                    entry.set_pair_state(PairState::Paired);
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
                        pause_media_on_calls: entry.view.pause_media_on_calls,
                        save_location: Some(entry.view.save_location.clone()),
                        run_commands: entry.view.run_commands.clone(),
                        ssh_host_key_fingerprint: existing_ssh_host_key.clone(),
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
                    entry.set_pair_state(PairState::RequestedByPeer);
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
            entry.set_pair_state(PairState::NotPaired);
            entry.view.verification_key = None;
            entry.view.now_playing = None;
            entry.view.notifications.clear();
            entry.view.battery_charge = None;
            entry.view.battery_charging = None;
            entry.view.sim_signals.clear();
            entry.mpris_players.clear();
            entry.mpris_player_order.clear();
            entry.active_mpris_player = None;
            entry.album_art_cache.clear();
            entry.album_art_in_flight.clear();
            drop(devices);
            self.teardown_sftp(device_id);
            self.trusted_devices.lock().unwrap().remove(device_id);
            self.save_trusted_devices();
        }

        self.notify_ui();
    }

    pub fn request_pairing(&self, device_id: &str) -> Result<()> {
        let ts = current_secs();
        let pkt = NetworkPacket::pair_request(ts);
        self.send_packet_to_device(device_id, &pkt)?;

        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.set_pair_state(PairState::Requested);
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
        let existing_ssh_host_key = self.trusted_devices.lock().ok().and_then(|trusted| {
            trusted
                .get(device_id)
                .and_then(|record| record.ssh_host_key_fingerprint.clone())
        });

        let mut trusted_record = None;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.set_pair_state(PairState::Paired);
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
                        pause_media_on_calls: entry.view.pause_media_on_calls,
                        save_location: Some(entry.view.save_location.clone()),
                        run_commands: entry.view.run_commands.clone(),
                        ssh_host_key_fingerprint: existing_ssh_host_key.clone(),
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

    pub fn unpair_or_reject(&self, device_id: &str) -> Result<()> {
        let pkt = NetworkPacket::unpair();
        let _ = self.send_packet_to_device(device_id, &pkt);

        if let Ok(mut devices) = self.devices.lock() {
            let should_remove = if let Some(entry) = devices.get_mut(device_id) {
                entry.set_pair_state(PairState::NotPaired);
                entry.view.verification_key = None;
                entry.view.now_playing = None;
                entry.view.battery_charge = None;
                entry.view.battery_charging = None;
                entry.view.sim_signals.clear();
                entry.mpris_players.clear();
                entry.mpris_player_order.clear();
                entry.active_mpris_player = None;
                entry.album_art_cache.clear();
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
        self.teardown_sftp(device_id);
        self.trusted_devices.lock().unwrap().remove(device_id);
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }
}
