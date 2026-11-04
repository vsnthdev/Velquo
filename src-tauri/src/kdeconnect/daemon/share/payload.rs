use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::current_millis;
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

impl KdeConnectDaemon {
    pub(super) async fn serve_outgoing_payload(
        &self,
        device_id: &str,
        transfer_id: &str,
        listener: &TcpListener,
        file_path: &Path,
        file_size: u64,
    ) -> Result<()> {
        let (expected_ip, expected_cert) = self.payload_peer_identity(device_id)?;

        // Accept only the paired device's IP. Other hosts on the LAN can reach
        // the payload port, so keep waiting instead of failing the transfer.
        let accept = async {
            loop {
                let (stream, addr) = listener.accept().await?;
                if addr.ip() == expected_ip {
                    return Ok::<_, std::io::Error>((stream, addr));
                }
                log::warn!(
                    "[share-out] Ignoring payload connection from unexpected IP {} (expected {})",
                    addr.ip(),
                    expected_ip
                );
                drop(stream);
            }
        };
        let (tcp_stream, peer_addr) = tokio::time::timeout(Duration::from_secs(30), accept)
            .await
            .context("Timed out waiting for peer to connect to payload port")??;

        log::info!(
            "[share-out] Peer {} connected to payload socket, starting TLS Server handshake...",
            peer_addr
        );

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(10),
            self.tls_acceptor.accept(tcp_stream),
        )
        .await
        .context("Payload TLS Server handshake timed out")?
        .context("Payload TLS Server handshake failed")?;

        let peer_cert = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first());
        Self::verify_pinned_payload_cert(device_id, peer_cert, &expected_cert)
            .map_err(|e| anyhow!("Rejected payload peer for '{}': {}", device_id, e))?;

        log::info!(
            "[share-out] Payload TLS Server handshake succeeded with {}, streaming {} bytes...",
            peer_addr,
            file_size
        );

        let mut file = tokio::fs::File::open(file_path).await?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut sent: u64 = 0;
        let mut last_notify_ms = 0i64;

        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            tls_stream.write_all(&buf[..n]).await?;
            sent += n as u64;

            let now = current_millis();
            if now - last_notify_ms >= 120 || sent >= file_size {
                last_notify_ms = now;
                self.update_transfer_progress(transfer_id, sent);
            }
        }

        tls_stream.flush().await?;
        let _ = tls_stream.shutdown().await;
        self.update_transfer_progress(transfer_id, sent);
        Ok(())
    }
}
