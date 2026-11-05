use crate::kdeconnect::cert::*;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Context, Result};
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

impl KdeConnectDaemon {
    pub(crate) async fn handle_outbound_tcp(
        self: &Arc<Self>,
        mut tcp_stream: TcpStream,
        peer_addr: SocketAddr,
        expected_device_id: String,
        protocol_version: i32,
        udp_identity_fallback: Option<NetworkPacket>,
    ) -> Result<()> {
        Self::configure_tcp_socket(&tcp_stream);
        let raw_fd = tcp_stream.as_raw_fd();

        let conn_packet = if protocol_version >= 8 {
            NetworkPacket::tcp_connection_packet(&self.host, &expected_device_id, protocol_version)
        } else {
            NetworkPacket::full_identity(&self.host)
        };
        let conn_bytes = conn_packet.serialize_line()?;
        tcp_stream.write_all(&conn_bytes).await?;
        tcp_stream.flush().await?;
        log::debug!(
            "[tcp-out] Sent plaintext connection packet to {} ({} bytes), starting TLS Server handshake...",
            peer_addr,
            conn_bytes.len()
        );

        let mut tls_stream =
            tokio::time::timeout(Duration::from_secs(6), self.tls_acceptor.accept(tcp_stream))
                .await
                .context("TLS Server handshake timed out")?
                .context("TLS Server handshake failed")?;

        let peer_cert_der = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .map(|c| c.as_ref().to_vec())
            .ok_or_else(|| {
                anyhow!("Peer did not present a client X.509 certificate during TLS handshake")
            })?;

        log::info!(
            "[tls] Outbound TCP -> TLS Server handshake succeeded with {} (cert SHA256={})",
            peer_addr,
            sha256_fingerprint(&peer_cert_der)
        );

        // In Protocol v8, both sides exchange their full identity packet over the encrypted TLS stream
        let my_identity_bytes = NetworkPacket::full_identity(&self.host).serialize_line()?;
        tls_stream.write_all(&my_identity_bytes).await?;
        tls_stream.flush().await?;
        log::debug!("[tls] Sent encrypted full identity packet to {}", peer_addr);

        let mut buf_reader = BufReader::new(tls_stream);
        let mut first_line = String::new();

        let final_identity = if protocol_version >= 8 {
            let mut scratch = Vec::new();
            let n = tokio::time::timeout(
                Duration::from_secs(3),
                super::line::read_line_limited(
                    &mut buf_reader,
                    &mut scratch,
                    &mut first_line,
                    MAX_PACKET_LINE_BYTES,
                ),
            )
            .await
            .context("Timed out waiting for peer encrypted identity packet (v8)")??;
            if n == 0 {
                return Err(anyhow!(
                    "Peer closed TLS stream before sending encrypted identity"
                ));
            }
            let pkt = NetworkPacket::parse_line(first_line.as_bytes())?;
            log::info!(
                "[tls] Received encrypted v8 identity packet from '{}'",
                pkt.get_str("deviceName").unwrap_or("Unknown")
            );
            pkt
        } else {
            udp_identity_fallback.ok_or_else(|| anyhow!("Missing v7 identity packet"))?
        };

        self.run_authenticated_link(buf_reader, raw_fd, peer_addr, final_identity, peer_cert_der)
            .await
    }

    /// Inbound TCP Connection Flow (`LanLinkProvider::newTcpConnection` + `tcpPacketReceived` + `encrypted`):
    /// 1. Read peer's plaintext connection/identity packet from TCP socket
    /// 2. Act as TLS CLIENT (`TlsConnector`)
    /// 3. Send our encrypted full identity packet and read peer's encrypted identity packet (if v8)
    pub(crate) async fn handle_inbound_tcp(
        self: &Arc<Self>,
        tcp_stream: TcpStream,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        Self::configure_tcp_socket(&tcp_stream);
        let raw_fd = tcp_stream.as_raw_fd();
        let mut plain_reader = BufReader::new(tcp_stream);
        let mut line = String::new();
        let mut scratch = Vec::new();

        let n = tokio::time::timeout(
            Duration::from_secs(3),
            super::line::read_line_limited(
                &mut plain_reader,
                &mut scratch,
                &mut line,
                MAX_PACKET_LINE_BYTES,
            ),
        )
        .await
        .context("Timed out waiting for plaintext connection packet on inbound TCP")??;
        if n == 0 {
            return Err(anyhow!(
                "Inbound TCP closed before sending connection packet"
            ));
        }

        let initial_packet = NetworkPacket::parse_line(line.as_bytes())?;
        if initial_packet.packet_type != PACKET_TYPE_IDENTITY {
            return Err(anyhow!(
                "Expected kdeconnect.identity packet on inbound TCP, got {}",
                initial_packet.packet_type
            ));
        }

        let peer_device_id = initial_packet
            .get_str("deviceId")
            .ok_or_else(|| anyhow!("Inbound connection packet missing deviceId"))?
            .to_string();
        let peer_proto = initial_packet
            .get_i64("protocolVersion")
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;

        // Deduplicate burst inbound TCP connections from the same device within 800ms
        let now_ms = current_millis();
        {
            let mut attempts = self.last_discovery_attempt_ms.lock().unwrap();
            let key = format!("inbound:{}", peer_device_id);
            if let Some(last) = attempts.get(&key) {
                if now_ms - *last < 800 {
                    log::debug!(
                        "[tcp-in] Dropping duplicate burst inbound TCP connection from {} (deviceId={}, {}ms since previous)",
                        peer_addr,
                        peer_device_id,
                        now_ms - *last
                    );
                    return Ok(());
                }
            }
            attempts.insert(key, now_ms);
        }

        log::info!(
            "[tcp-in] Received plaintext connection packet from {} (deviceId={}, proto=v{}), starting TLS Client handshake...",
            peer_addr,
            peer_device_id,
            peer_proto
        );

        let raw_tcp = plain_reader.into_inner();
        let server_name = ServerName::try_from("kdeconnect.local")
            .unwrap_or_else(|_| ServerName::IpAddress(peer_addr.ip().into()));

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(6),
            self.tls_connector.connect(server_name, raw_tcp),
        )
        .await
        .context("TLS Client handshake timed out")?
        .context("TLS Client handshake failed")?;

        let peer_cert_der = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .map(|c| c.as_ref().to_vec())
            .ok_or_else(|| {
                anyhow!("Peer did not present a server X.509 certificate during TLS handshake")
            })?;

        log::info!(
            "[tls] Inbound TCP -> TLS Client handshake succeeded with {} (cert SHA256={})",
            peer_addr,
            sha256_fingerprint(&peer_cert_der)
        );

        if peer_proto >= 8 {
            let my_identity_bytes = NetworkPacket::full_identity(&self.host).serialize_line()?;
            tls_stream.write_all(&my_identity_bytes).await?;
            tls_stream.flush().await?;
            log::debug!(
                "[tls] Sent encrypted full identity packet to {} (inbound TCP)",
                peer_addr
            );

            let mut buf_reader = BufReader::new(tls_stream);
            let mut sec_line = String::new();
            let mut scratch = Vec::new();
            let n = tokio::time::timeout(
                Duration::from_secs(3),
                super::line::read_line_limited(
                    &mut buf_reader,
                    &mut scratch,
                    &mut sec_line,
                    MAX_PACKET_LINE_BYTES,
                ),
            )
            .await
            .context("Timed out waiting for peer encrypted v8 identity on inbound TCP")??;
            if n == 0 {
                return Err(anyhow!(
                    "Peer closed inbound TLS stream before sending encrypted identity"
                ));
            }
            let sec_pkt = NetworkPacket::parse_line(sec_line.as_bytes())?;
            log::info!(
                "[tls] Received encrypted v8 identity packet from '{}' (id={})",
                sec_pkt.get_str("deviceName").unwrap_or("Unknown"),
                sec_pkt.get_str("deviceId").unwrap_or("?")
            );
            self.run_authenticated_link(buf_reader, raw_fd, peer_addr, sec_pkt, peer_cert_der)
                .await
        } else {
            let buf_reader = BufReader::new(tls_stream);
            self.run_authenticated_link(
                buf_reader,
                raw_fd,
                peer_addr,
                initial_packet,
                peer_cert_der,
            )
            .await
        }
    }
}
