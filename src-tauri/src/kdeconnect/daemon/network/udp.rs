use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::{current_millis, current_secs};
use crate::kdeconnect::packet::{NetworkPacket, PACKET_TYPE_IDENTITY, PROTOCOL_VERSION};
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};

pub(super) fn bind_udp_discovery_socket() -> Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .context("Failed to create IPv4 UDP socket")?;
    let _ = sock.set_reuse_address(true);
    let _ = sock.set_reuse_port(true);
    let _ = sock.set_broadcast(true);
    sock.set_nonblocking(true)?;

    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, UDP_PORT);
    sock.bind(&addr.into())
        .with_context(|| format!("Failed to bind UDP port {}", UDP_PORT))?;

    let std_sock: std::net::UdpSocket = sock.into();
    let tokio_sock = UdpSocket::from_std(std_sock)?;
    Ok(tokio_sock)
}

pub(super) fn spawn_udp_receive_loop(daemon: Arc<KdeConnectDaemon>, udp: Arc<UdpSocket>) {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 16384];
        loop {
            match udp.recv_from(&mut buf).await {
                Ok((len, sender_addr)) => {
                    let data = &buf[..len];
                    daemon.handle_udp_datagram(data, sender_addr);
                }
                Err(e) => {
                    log::warn!("[udp] recv_from error: {}", e);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        }
    });
}

impl KdeConnectDaemon {
    pub async fn broadcast_udp_discovery(&self) {
        let tcp_port = *self.tcp_port.lock().unwrap();
        let packet = NetworkPacket::udp_discovery(&self.host, tcp_port);
        let bytes = match packet.serialize_line() {
            Ok(b) => b,
            Err(e) => {
                log::error!("[udp] Failed to serialize UDP discovery packet: {}", e);
                return;
            }
        };

        let interfaces = Self::get_local_ipv4s();
        log::debug!(
            "[udp] Broadcasting UDP discovery packet ({} bytes) across {} interface(s)",
            bytes.len(),
            interfaces.len()
        );

        // Broadcast once from each local interface IP to its subnet broadcast address
        let mut sent_any = false;
        for (source_ip, subnet_bcast) in &interfaces {
            if let Ok(sock) = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
                let _ = sock.set_broadcast(true);
                let bind_addr = SocketAddrV4::new(*source_ip, 0);
                if sock.bind(&bind_addr.into()).is_ok() {
                    let std_sock: std::net::UdpSocket = sock.into();
                    let dest_subnet = SocketAddrV4::new(*subnet_bcast, UDP_PORT);
                    if std_sock.send_to(&bytes, dest_subnet).is_ok() {
                        sent_any = true;
                        log::debug!(
                            "[udp] Sent discovery broadcast from {} -> {}",
                            source_ip,
                            dest_subnet
                        );
                    }
                }
            }
        }

        // Fallback to global broadcast only if no subnet broadcast succeeded
        if !sent_any {
            let maybe_sock = self.udp_socket.lock().unwrap().clone();
            if let Some(sock) = maybe_sock {
                let _ = sock
                    .send_to(&bytes, SocketAddrV4::new(Ipv4Addr::BROADCAST, UDP_PORT))
                    .await;
            }
        }
    }

    pub async fn probe_ip(&self, ip_str: &str) -> Result<()> {
        let ip: IpAddr = ip_str
            .parse()
            .with_context(|| format!("Invalid IP address: {}", ip_str))?;
        let tcp_port = *self.tcp_port.lock().unwrap();
        let packet = NetworkPacket::udp_discovery(&self.host, tcp_port);
        let bytes = packet.serialize_line()?;

        let dest = SocketAddr::new(ip, UDP_PORT);
        log::info!("[udp] Sending directed UDP discovery probe to {}", dest);
        let maybe_sock = self.udp_socket.lock().unwrap().clone();
        if let Some(sock) = maybe_sock {
            sock.send_to(&bytes, dest).await?;
        }
        Ok(())
    }

    pub(super) fn handle_udp_datagram(self: &Arc<Self>, data: &[u8], sender_addr: SocketAddr) {
        let ip = match sender_addr.ip() {
            IpAddr::V4(v4) => IpAddr::V4(v4),
            IpAddr::V6(v6) => {
                if let Some(mapped) = v6.to_ipv4_mapped() {
                    IpAddr::V4(mapped)
                } else {
                    IpAddr::V6(v6)
                }
            }
        };

        // Ignore our own local IPs
        let local_ips = Self::get_local_ipv4s();
        if let IpAddr::V4(v4) = ip {
            if v4.is_loopback() || local_ips.iter().any(|(local, _)| *local == v4) {
                return;
            }
        }

        let packet = match NetworkPacket::parse_line(data) {
            Ok(p) => p,
            Err(e) => {
                log::debug!(
                    "[udp] Ignoring non-JSON or malformed UDP datagram from {}: {}",
                    sender_addr,
                    e
                );
                return;
            }
        };

        if packet.packet_type != PACKET_TYPE_IDENTITY {
            return;
        }

        let Some(device_id) = packet.get_str("deviceId").map(|s| s.to_string()) else {
            return;
        };

        if device_id == self.host.device_id {
            return;
        }

        let protocol_version = packet
            .get_i64("protocolVersion")
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;
        let tcp_port = packet
            .get_i64("tcpPort")
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(MIN_TCP_PORT);
        let device_name = packet
            .get_str("deviceName")
            .unwrap_or("Unknown")
            .to_string();

        log::info!(
            "[udp] Received UDP identity broadcast from '{}' (id={}, ip={}, tcpPort={}, proto=v{})",
            device_name,
            device_id,
            ip,
            tcp_port,
            protocol_version
        );

        let daemon = Arc::clone(self);
        tokio::spawn(async move {
            daemon
                .on_device_discovered(ip, tcp_port, device_id, protocol_version, Some(packet))
                .await;
        });
    }

    pub async fn on_device_discovered(
        self: &Arc<Self>,
        ip: IpAddr,
        tcp_port: u16,
        device_id: String,
        protocol_version: i32,
        udp_packet: Option<NetworkPacket>,
    ) {
        if !(MIN_TCP_PORT..=MAX_TCP_PORT).contains(&tcp_port) {
            log::warn!(
                "[discovery] Ignoring device {} with out-of-range TCP port {}",
                device_id,
                tcp_port
            );
            return;
        }

        // Check if this device already has an active, healthy TLS connection at this IP
        // (Matches Soduto's ConnectionProviderDelegate.isNewConnectionNeeded)
        {
            let mut devices = self.devices.lock().unwrap();
            if let Some(entry) = devices.get_mut(&device_id) {
                if entry.view.reachable
                    && entry.sender.is_some()
                    && entry.view.address == ip.to_string()
                {
                    entry.view.last_seen_secs = current_secs();
                    log::debug!(
                        "[discovery] Device '{}' ({}) is already connected and online at {}, skipping redundant outbound connection",
                        entry.view.device_name,
                        device_id,
                        ip
                    );
                    return;
                }
            }
        }

        // Rate-limit connection attempts to same device within 1500ms
        let now = current_millis();
        {
            let mut attempts = self.last_discovery_attempt_ms.lock().unwrap();
            if let Some(last) = attempts.get(&device_id) {
                if now - *last < 1500 {
                    log::debug!(
                        "[discovery] Skipping duplicate discovery of {} ({}ms since last attempt)",
                        device_id,
                        now - *last
                    );
                    return;
                }
            }
            attempts.insert(device_id.clone(), now);
        }

        let target_addr = SocketAddr::new(ip, tcp_port);
        log::info!(
            "[tcp-out] Connecting to discovered device {} at {} (proto=v{})...",
            device_id,
            target_addr,
            protocol_version
        );

        match tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(target_addr)).await {
            Ok(Ok(tcp_stream)) => {
                if let Err(e) = self
                    .handle_outbound_tcp(
                        tcp_stream,
                        target_addr,
                        device_id.clone(),
                        protocol_version,
                        udp_packet,
                    )
                    .await
                {
                    log::warn!(
                        "[tcp-out] Outbound session with {} ({}) ended: {:?}",
                        device_id,
                        target_addr,
                        e
                    );
                }
            }
            Ok(Err(e)) => {
                log::warn!(
                    "[tcp-out] TCP connect to {} failed ({}), sending reverse UDP discovery packet",
                    target_addr,
                    e
                );
                let _ = self.probe_ip(&ip.to_string()).await;
            }
            Err(_) => {
                log::warn!(
                    "[tcp-out] TCP connect to {} timed out, sending reverse UDP discovery packet",
                    target_addr
                );
                let _ = self.probe_ip(&ip.to_string()).await;
            }
        }
    }
}
