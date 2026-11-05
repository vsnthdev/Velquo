use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::Result;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::sync::{mpsc, oneshot};

pub(super) async fn run_link_loop<S>(
    daemon: &Arc<KdeConnectDaemon>,
    buf_reader: BufReader<S>,
    raw_fd: RawFd,
    peer_addr: SocketAddr,
    generation: u64,
    paired_flag: Arc<AtomicBool>,
    device_id: String,
    device_name: String,
    peer_cert_der: Vec<u8>,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut abort_rx: oneshot::Receiver<()>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (read_half, mut write_half) = tokio::io::split(buf_reader);
    let mut line_reader = BufReader::new(read_half);
    let mut line = String::new();
    let mut read_scratch = Vec::new();
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(2));
    heartbeat_interval.tick().await; // consume initial immediate tick
    let mut unacked_since: Option<Instant> = None;

    loop {
        tokio::select! {
            _ = &mut abort_rx => {
                log::debug!(
                    "[device] Connection gen {} for '{}' ({}) superseded by newer session, closing old link",
                    generation,
                    device_name,
                    device_id
                );
                break;
            }
            _ = heartbeat_interval.tick() => {
                let unacked = KdeConnectDaemon::get_socket_unacked_bytes(raw_fd);
                if unacked > 0 {
                    if let Some(since) = unacked_since {
                        if since.elapsed() >= Duration::from_millis(1800) {
                            log::warn!(
                                "[heartbeat] Device '{}' ({}) has {} unacknowledged TCP bytes after {:?} — peer is offline, closing link",
                                device_name,
                                device_id,
                                unacked,
                                since.elapsed()
                            );
                            break;
                        }
                    } else {
                        unacked_since = Some(Instant::now());
                    }
                } else {
                    unacked_since = None;
                    let is_currently_paired = daemon
                        .devices
                        .lock()
                        .ok()
                        .and_then(|d| d.get(&device_id).map(|e| e.view.pair_state == PairState::Paired))
                        .unwrap_or(false);

                    // Only send application-layer keepalive packets when PAIRED!
                    // In KDE Connect, sending any non-`kdeconnect.pair` packet to an unpaired device
                    // causes the remote peer to respond with `{"type":"kdeconnect.pair","body":{"pair":false}}`.
                    if is_currently_paired {
                        if let Ok(hb_bytes) = NetworkPacket::keepalive().serialize_line() {
                            if let Err(e) = write_half.write_all(&hb_bytes).await {
                                log::warn!("[heartbeat] Write error to '{}': {}", device_name, e);
                                break;
                            }
                            if let Err(e) = write_half.flush().await {
                                log::warn!("[heartbeat] Flush error to '{}': {}", device_name, e);
                                break;
                            }
                            unacked_since = Some(Instant::now());
                        }
                    }
                }
            }
            maybe_out = rx.recv() => {
                match maybe_out {
                    Some(bytes) => {
                        if let Err(e) = write_half.write_all(&bytes).await {
                            log::warn!("[device] Write error to '{}': {}", device_name, e);
                            break;
                        }
                        let _ = write_half.flush().await;
                        if unacked_since.is_none() {
                            unacked_since = Some(Instant::now());
                        }
                    }
                    None => break,
                }
            }
            read_res = super::line::read_line_limited(
                &mut line_reader,
                &mut read_scratch,
                &mut line,
                MAX_PACKET_LINE_BYTES,
            ) => {
                match read_res {
                    Ok(0) => {
                        log::info!("[device] Device '{}' ({}) closed the TLS connection", device_name, device_id);
                        break;
                    }
                    Ok(_) => {
                        unacked_since = None;
                        let raw_line = line.clone();
                        line.clear();
                        if let Ok(mut devices) = daemon.devices.lock() {
                            if let Some(entry) = devices.get_mut(&device_id) {
                                entry.view.last_seen_secs = current_secs();
                            }
                        }
                        match NetworkPacket::parse_line(raw_line.as_bytes()) {
                            Ok(pkt) => {
                                daemon.handle_incoming_packet(
                                    &device_id,
                                    &device_name,
                                    peer_addr.ip(),
                                    &peer_cert_der,
                                    &paired_flag,
                                    pkt,
                                )
                                .await;
                            }
                            Err(e) => {
                                log::warn!("[device] Failed to parse packet from '{}': {} | raw={}", device_name, e, raw_line.trim());
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("[device] Read error from '{}' ({}): {}", device_name, device_id, e);
                        break;
                    }
                }
            }
        }
    }

    // Mark offline if this was the latest active connection generation
    let mut became_offline = false;
    {
        let mut devices = daemon.devices.lock().unwrap();
        if let Some(entry) = devices.get_mut(&device_id) {
            if entry.connection_generation == generation {
                entry.view.reachable = false;
                entry.view.notifications.clear();
                // Keep Now Playing metadata and cached album art across transient reconnects.
                // `link_init` reuses these fields, so a reconnect can show the last known
                // track immediately instead of flashing an empty state while MPRIS data
                // is fetched again. Pending artwork fetches belong to the old connection.
                entry.album_art_in_flight.clear();
                entry.sender = None;
                entry.abort_tx = None;
                became_offline = true;
                log::info!(
                    "[device] Device '{}' ({}) marked OFFLINE",
                    device_name,
                    device_id
                );
            }
        }
    }
    daemon.notify_ui();

    if became_offline {
        daemon.sftp_manager.clear_device(&device_id);
        // Send a discovery broadcast so the device can reconnect immediately as soon as its Wi-Fi returns
        let daemon = Arc::clone(daemon);
        tokio::spawn(async move {
            daemon.broadcast_udp_discovery().await;
        });
    }

    Ok(())
}
