use crate::kdeconnect::cert::*;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::sync::{mpsc, oneshot};

impl KdeConnectDaemon {
    pub(super) async fn run_authenticated_link<S>(
        self: &Arc<Self>,
        buf_reader: BufReader<S>,
        raw_fd: RawFd,
        peer_addr: SocketAddr,
        identity_packet: NetworkPacket,
        peer_cert_der: Vec<u8>,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let device_id = identity_packet
            .get_str("deviceId")
            .ok_or_else(|| anyhow!("Identity packet missing deviceId"))?
            .to_string();
        let device_name = identity_packet
            .get_str("deviceName")
            .unwrap_or("KDE Connect Device")
            .to_string();
        let device_type = identity_packet
            .get_str("deviceType")
            .unwrap_or("phone")
            .to_string();
        let protocol_version = identity_packet
            .get_i64("protocolVersion")
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;
        let incoming_caps = identity_packet.get_string_list("incomingCapabilities");
        let outgoing_caps = identity_packet.get_string_list("outgoingCapabilities");

        if let Some(cert_cn) = extract_cert_common_name(&peer_cert_der) {
            if cert_cn != device_id {
                log::warn!(
                    "[tls] Certificate CN '{}' differs from identity packet deviceId '{}'",
                    cert_cn,
                    device_id
                );
            }
        }

        // Verify pinned certificate if this device is already trusted/paired
        let default_downloads_str = self.downloads_dir.to_string_lossy().to_string();
        let (
            is_trusted,
            saved_sync_clipboard,
            saved_allow_presenter,
            saved_sync_notifications,
            saved_auto_copy_otp,
            saved_pause_media_on_calls,
            saved_save_location,
            saved_run_commands,
        ) = {
            let trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get(&device_id) {
                if rec.cert_der != peer_cert_der {
                    log::error!(
                        "[tls] SECURITY ALERT: Device {} ('{}') presented a different certificate than pinned! Aborting connection.",
                        device_id,
                        device_name
                    );
                    return Err(anyhow!(
                        "Certificate mismatch for paired device {}",
                        device_id
                    ));
                }
                let loc = rec
                    .save_location
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| default_downloads_str.clone());
                (
                    true,
                    rec.sync_clipboard,
                    rec.allow_presenter,
                    rec.sync_notifications,
                    rec.auto_copy_otp,
                    rec.pause_media_on_calls,
                    loc,
                    rec.run_commands.clone(),
                )
            } else {
                (
                    false,
                    true,
                    true,
                    true,
                    true,
                    true,
                    default_downloads_str,
                    Vec::new(),
                )
            }
        };

        let cert_fp = sha256_fingerprint(&peer_cert_der);
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (abort_tx, abort_rx) = oneshot::channel::<()>();

        let (generation, paired_flag) = {
            let mut devices = self.devices.lock().unwrap();
            let mut prev_abort: Option<oneshot::Sender<()>> = None;
            let next_gen = if let Some(existing) = devices.get_mut(&device_id) {
                prev_abort = existing.abort_tx.take();
                existing.connection_generation + 1
            } else {
                1
            };

            if let Some(old_abort) = prev_abort {
                let _ = old_abort.send(());
            }

            let existing_pair_state = devices
                .get(&device_id)
                .map(|d| d.view.pair_state)
                .unwrap_or(PairState::NotPaired);
            let pair_state = if is_trusted {
                PairState::Paired
            } else {
                match existing_pair_state {
                    // A trusted record is the source of truth. Never allow a stale
                    // `Paired` view to survive after trust was removed.
                    PairState::Requested | PairState::RequestedByPeer => existing_pair_state,
                    _ => PairState::NotPaired,
                }
            };
            let paired_flag = Arc::new(AtomicBool::new(pair_state == PairState::Paired));

            let verification_key = devices
                .get(&device_id)
                .and_then(|d| d.view.verification_key.clone())
                .or_else(|| {
                    compute_verification_key(&self.host.cert_der, &peer_cert_der, current_secs())
                });

            let (
                prev_bat_charge,
                prev_bat_charging,
                prev_net_type,
                prev_sig_str,
                prev_sim_signals,
                prev_now_playing,
                prev_mpris_players,
                prev_mpris_order,
                prev_active_mpris,
                prev_art_cache,
            ) = devices
                .get(&device_id)
                .map(|d| {
                    (
                        d.view.battery_charge,
                        d.view.battery_charging,
                        d.view.cellular_network_type.clone(),
                        d.view.cellular_signal_strength,
                        d.view.sim_signals.clone(),
                        d.view.now_playing.clone(),
                        d.mpris_players.clone(),
                        d.mpris_player_order.clone(),
                        d.active_mpris_player.clone(),
                        d.album_art_cache.clone(),
                    )
                })
                .unwrap_or((
                    None,
                    None,
                    None,
                    None,
                    Vec::new(),
                    None,
                    HashMap::new(),
                    Vec::new(),
                    None,
                    HashMap::new(),
                ));

            let prev_notifications = devices
                .get(&device_id)
                .map(|d| d.view.notifications.clone())
                .unwrap_or_default();

            devices.insert(
                device_id.clone(),
                DeviceEntry {
                    view: DeviceView {
                        device_id: device_id.clone(),
                        device_name: device_name.clone(),
                        device_type,
                        protocol_version,
                        address: peer_addr.ip().to_string(),
                        tcp_port: peer_addr.port(),
                        reachable: true,
                        pair_state,
                        verification_key,
                        cert_fingerprint: Some(cert_fp),
                        incoming_capabilities: incoming_caps.clone(),
                        outgoing_capabilities: outgoing_caps.clone(),
                        last_seen_secs: current_secs(),
                        battery_charge: prev_bat_charge,
                        battery_charging: prev_bat_charging,
                        cellular_network_type: prev_net_type,
                        cellular_signal_strength: prev_sig_str,
                        sim_signals: prev_sim_signals,
                        now_playing: prev_now_playing,
                        sync_clipboard: saved_sync_clipboard,
                        allow_presenter: saved_allow_presenter,
                        sync_notifications: saved_sync_notifications,
                        auto_copy_otp: saved_auto_copy_otp,
                        pause_media_on_calls: saved_pause_media_on_calls,
                        save_location: saved_save_location,
                        notifications: prev_notifications,
                        run_commands: saved_run_commands,
                    },
                    peer_cert_der: Some(peer_cert_der.clone()),
                    sender: Some(tx),
                    abort_tx: Some(abort_tx),
                    connection_generation: next_gen,
                    mpris_players: prev_mpris_players,
                    mpris_player_order: prev_mpris_order,
                    active_mpris_player: prev_active_mpris,
                    album_art_cache: prev_art_cache,
                    album_art_in_flight: HashMap::new(),
                    paired: Arc::clone(&paired_flag),
                },
            );
            (next_gen, paired_flag)
        };

        log::info!(
            "[device] Device '{}' ({}) is now ONLINE at {} (paired={}, caps_in={:?})",
            device_name,
            device_id,
            peer_addr.ip(),
            is_trusted,
            incoming_caps
        );

        // Request initial battery and connectivity status if paired
        if is_trusted {
            self.request_device_telemetry(&device_id);
        }
        self.notify_ui();
        super::link_loop::run_link_loop(
            self,
            buf_reader,
            raw_fd,
            peer_addr,
            generation,
            paired_flag,
            device_id,
            device_name,
            peer_cert_der,
            rx,
            abort_rx,
        )
        .await
    }
}
