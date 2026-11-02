mod battery;
mod pairing;
mod telephony;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::macos_media::{handle_incoming_mpris_request, DEFAULT_MAC_PLAYER};
use crate::kdeconnect::packet::*;
use crate::kdeconnect::presenter_input::{handle_mousepad_packet, handle_presenter_packet};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

impl KdeConnectDaemon {
    pub(super) async fn handle_incoming_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        peer_cert_der: &[u8],
        paired: &AtomicBool,
        packet: NetworkPacket,
    ) {
        if packet.packet_type == "velquo.keepalive" || packet.packet_type == "soduto.keepalive" {
            return;
        }

        // Pairing is the only plugin allowed before trust is established. This
        // check is lock-free because it runs for every incoming packet,
        // including high-frequency mousepad/presenter traffic.
        if packet.packet_type != PACKET_TYPE_PAIR && !paired.load(Ordering::Acquire) {
            log::trace!(
                "[packet-in] Dropping '{}' from unpaired device '{}' ({})",
                packet.packet_type,
                device_name,
                device_id
            );
            return;
        }

        if packet.packet_type != PACKET_TYPE_MOUSEPAD_REQUEST
            && packet.packet_type != PACKET_TYPE_PRESENTER
        {
            log::info!(
                "[packet-in] From '{}' ({}): type='{}' body={} payloadSize={:?} transferInfo={:?}",
                device_name,
                device_id,
                packet.packet_type,
                serde_json::to_string(&packet.body).unwrap_or_default(),
                packet.payload_size,
                packet.payload_transfer_info
            );
        }

        match packet.packet_type.as_str() {
            PACKET_TYPE_PAIR => {
                self.handle_pair_packet(device_id, device_name, peer_cert_der, &packet);
            }
            PACKET_TYPE_PING => {
                let msg = packet.get_str("message").unwrap_or("Ping!");
                log::info!("[ping] Received Ping from '{}': \"{}\"", device_name, msg);
                Self::show_macos_notification(device_name, msg, Some(device_id), None, false);
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
                if let Some(subs) = packet
                    .body
                    .get("signalStrengths")
                    .and_then(|v| v.as_object())
                {
                    let mut sim_list: Vec<SimSignalInfo> = subs
                        .iter()
                        .filter_map(|(slot_key, sub_val)| {
                            let net_type = sub_val
                                .get("networkType")
                                .and_then(|v| v.as_str())?
                                .to_string();
                            let sig_strength =
                                sub_val.get("signalStrength").and_then(|v| v.as_i64())? as i32;
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
                    // Pairing may change between packet receipt and this task;
                    // re-check before executing any MPRIS action.
                    if !daemon.is_device_paired(&dev_id) {
                        log::debug!(
                            "[mpris-host] Ignoring request from unpaired device '{}' ({})",
                            dev_name,
                            dev_id
                        );
                        return;
                    }

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

                    if let Ok(responses) = tokio::task::spawn_blocking(move || {
                        handle_incoming_mpris_request(&pkt_clone)
                    })
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
}
