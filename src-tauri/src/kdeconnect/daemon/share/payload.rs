use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::current_millis;
use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

impl KdeConnectDaemon {
    pub(super) async fn serve_outgoing_payload(
        &self,
        transfer_id: &str,
        listener: &TcpListener,
        file_path: &Path,
        file_size: u64,
    ) -> Result<()> {
        let (tcp_stream, peer_addr) =
            tokio::time::timeout(Duration::from_secs(30), listener.accept())
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
