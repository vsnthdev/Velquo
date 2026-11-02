use crate::kdeconnect::daemon::*;
use crate::kdeconnect::macos_media::get_cached_mac_album_art;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use rustls::pki_types::ServerName;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

impl KdeConnectDaemon {
    pub async fn serve_mac_album_art_to_device(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        player: &str,
        album_art_url: &str,
    ) {
        let url_owned = album_art_url.to_string();
        let maybe_bytes = tokio::task::spawn_blocking(move || get_cached_mac_album_art(&url_owned))
            .await
            .ok()
            .flatten();

        let Some(art_bytes) = maybe_bytes else {
            log::debug!(
                "[mpris-host] No cached Mac album art found for '{}' requested by '{}'",
                album_art_url,
                device_name
            );
            return;
        };

        let mut payload_listener = None;
        let mut payload_port = MIN_PAYLOAD_PORT;
        for port in MIN_PAYLOAD_PORT..=MAX_PAYLOAD_PORT {
            if let Ok(listener) =
                TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await
            {
                payload_port = port;
                payload_listener = Some(listener);
                break;
            }
        }

        let Some(listener) = payload_listener else {
            log::warn!(
                "[mpris-host] No free payload port ({}..={}) to stream Mac album art",
                MIN_PAYLOAD_PORT,
                MAX_PAYLOAD_PORT
            );
            return;
        };

        let mut body = serde_json::Map::new();
        body.insert("transferringAlbumArt".into(), serde_json::json!(true));
        body.insert("player".into(), serde_json::json!(player));
        body.insert("albumArtUrl".into(), serde_json::json!(album_art_url));

        let mut transfer_info = serde_json::Map::new();
        transfer_info.insert("port".into(), serde_json::json!(payload_port));

        let mut pkt = NetworkPacket::new(PACKET_TYPE_MPRIS, body);
        pkt.payload_size = Some(art_bytes.len() as i64);
        pkt.payload_transfer_info = Some(transfer_info);

        if let Err(e) = self.send_packet_to_device(device_id, &pkt) {
            log::warn!(
                "[mpris-host] Failed to send transferringAlbumArt packet to '{}': {}",
                device_name,
                e
            );
            return;
        }

        log::info!(
            "[mpris-host] Streaming Mac album art ({} bytes) for '{}' on port {} to '{}'",
            art_bytes.len(),
            player,
            payload_port,
            device_name
        );

        if let Ok(Ok((tcp_stream, _peer_addr))) =
            tokio::time::timeout(Duration::from_secs(10), listener.accept()).await
        {
            if let Ok(Ok(mut tls_stream)) =
                tokio::time::timeout(Duration::from_secs(8), self.tls_acceptor.accept(tcp_stream))
                    .await
            {
                let _ = tls_stream.write_all(&art_bytes).await;
                let _ = tls_stream.flush().await;
                let _ = tls_stream.shutdown().await;
                log::info!(
                    "[mpris-host] Finished streaming Mac album art ({} bytes) to '{}'",
                    art_bytes.len(),
                    device_name
                );
            }
        }
    }

    /// Reads the host MacBook's battery percentage (0..=100) and charging status directly via `IOKit.framework` (`IOPSCopyPowerSourcesInfo`).
    #[cfg(target_os = "macos")]
    pub(super) async fn download_album_art_payload(
        &self,
        peer_ip: IpAddr,
        port: u16,
        expected_bytes: usize,
    ) -> Result<String> {
        let target = SocketAddr::new(peer_ip, port);
        let tcp = tokio::time::timeout(Duration::from_secs(8), TcpStream::connect(target))
            .await
            .context("Album art TCP connection timed out")?
            .context("Album art TCP connection failed")?;

        let server_name = ServerName::try_from("kdeconnect.local")
            .unwrap_or_else(|_| ServerName::IpAddress(peer_ip.into()));

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(8),
            self.tls_connector.connect(server_name, tcp),
        )
        .await
        .context("Album art TLS handshake timed out")?
        .context("Album art TLS handshake failed")?;

        let mut bytes = Vec::with_capacity(expected_bytes.min(5 * 1024 * 1024));
        let mut buf = [0u8; 16 * 1024];
        loop {
            let n = tls_stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..n]);
            if expected_bytes > 0 && bytes.len() >= expected_bytes {
                break;
            }
            if bytes.len() > 8 * 1024 * 1024 {
                return Err(anyhow!("Album art exceeded max size limit"));
            }
        }

        if bytes.is_empty() {
            return Err(anyhow!("Received empty album art payload"));
        }

        let mime = if bytes.starts_with(b"\x89PNG") {
            "image/png"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            "image/jpeg"
        };

        let encoded = BASE64_STANDARD.encode(&bytes);
        Ok(format!("data:{};base64,{}", mime, encoded))
    }
}
