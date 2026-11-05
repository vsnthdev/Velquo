use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Context, Result};
use rustls::pki_types::ServerName;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

impl KdeConnectDaemon {
    pub(crate) async fn handle_share_request(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        peer_ip: IpAddr,
        packet: NetworkPacket,
    ) {
        if !self.is_device_paired(device_id) {
            log::debug!(
                "[share-in] Ignoring packet from unpaired device '{}' ({})",
                device_name,
                device_id
            );
            return;
        }

        // Case 1: File transfer via auxiliary TLS payload socket
        if let Some(port) = packet.payload_port() {
            let raw_filename = packet
                .get_str("filename")
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("kdeconnect-{}.bin", current_millis()));
            let clean_name = Path::new(&raw_filename)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shared_file")
                .to_string();
            let total_bytes = packet.payload_size.unwrap_or(0).max(0) as u64;

            let transfer_id = Uuid::new_v4().to_string();
            let dest_path = self.unique_download_path(device_id, &clean_name);

            log::info!(
                "[share-in] Starting file download ({} bytes) from '{}' at {}:{}",
                total_bytes,
                device_name,
                peer_ip,
                port
            );

            self.upsert_transfer(TransferItem {
                id: transfer_id.clone(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                direction: "Incoming".to_string(),
                kind: "File".to_string(),
                filename: clean_name.clone(),
                total_bytes,
                transferred_bytes: 0,
                status: "InProgress".to_string(),
                saved_path: None,
                text_content: None,
                url_content: None,
                error: None,
                timestamp_ms: current_millis(),
            });

            let daemon = Arc::clone(self);
            let dev_id = device_id.to_string();
            let dev_name = device_name.to_string();
            tokio::spawn(async move {
                match daemon
                    .download_payload_file(
                        &transfer_id,
                        &dev_id,
                        &dev_name,
                        peer_ip,
                        port,
                        total_bytes,
                        &dest_path,
                    )
                    .await
                {
                    Ok(saved) => {
                        log::info!("[share-in] Completed file download from '{}'", dev_name);
                        daemon.finish_transfer(
                            &transfer_id,
                            "Completed",
                            Some(saved.to_string_lossy().to_string()),
                            None,
                        );
                    }
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&dest_path).await;
                        log::error!("[share-in] File download failed: {:?}", e);
                        daemon.finish_transfer(&transfer_id, "Failed", None, Some(e.to_string()));
                    }
                }
            });
            return;
        }

        // Case 2: Shared Text
        if let Some(text) = packet.get_str("text") {
            log::info!(
                "[share-in] Received shared text from '{}' ({} chars)",
                device_name,
                text.len()
            );

            if text.trim().is_empty() {
                return;
            }

            // Text shares are clipboard/open intents, not file downloads. Regular file
            // shares still use Case 1 above and keep the configured save folder.

            let filename = format!("Text: {}", text.chars().take(36).collect::<String>());
            let extracted_url = text
                .lines()
                .map(|l| l.trim())
                .rfind(|l| !l.is_empty())
                .filter(|cand| cand.starts_with("http://") || cand.starts_with("https://"))
                .map(|s| s.to_string());

            // A text share that is exactly one safe URL is an explicit "open this"
            // intent from the paired device. Anything else is clipboard content.
            if let Some(url) = single_safe_share_url(text) {
                log::info!(
                    "[share-in] Opening single shared URL from '{}' ({} chars)",
                    device_name,
                    url.len()
                );

                match open::that(url) {
                    Ok(()) => {
                        log::info!("[share-in] Auto-opened shared URL in default browser");
                        self.upsert_transfer(share_text_transfer(
                            device_id,
                            device_name,
                            "Url",
                            url.to_string(),
                            text,
                            Some(url),
                            "Completed",
                            None,
                        ));
                    }
                    Err(e) => {
                        log::warn!("[share-in] Failed to auto-open shared URL: {}", e);

                        // Never lose the link: fall back to copying it.
                        match self.copy_text_to_clipboard(text) {
                            Ok(()) => {
                                Self::show_macos_notification(
                                    "Could not open shared link",
                                    "The link was copied to your clipboard instead.",
                                    Some(device_id),
                                    None,
                                    false,
                                );
                                self.upsert_transfer(share_text_transfer(
                                    device_id,
                                    device_name,
                                    "Url",
                                    url.to_string(),
                                    text,
                                    Some(url),
                                    "Failed",
                                    Some(format!(
                                        "Failed to open link: {}; copied to clipboard",
                                        e
                                    )),
                                ));
                            }
                            Err(copy_err) => {
                                Self::show_macos_notification(
                                    "Could not open or copy shared link",
                                    &format!("From {}: {}", device_name, copy_err),
                                    Some(device_id),
                                    None,
                                    false,
                                );
                                self.upsert_transfer(share_text_transfer(
                                    device_id,
                                    device_name,
                                    "Url",
                                    url.to_string(),
                                    text,
                                    Some(url),
                                    "Failed",
                                    Some(format!(
                                        "Failed to open link: {}; failed to copy: {}",
                                        e, copy_err
                                    )),
                                ));
                            }
                        }
                    }
                }
                return;
            }

            match self.copy_text_to_clipboard(text) {
                Ok(()) => {
                    log::info!(
                        "[share-in] Copied shared text from '{}' to clipboard",
                        device_name
                    );
                    Self::show_macos_notification(
                        "Shared text copied to clipboard",
                        &format!("From {}", device_name),
                        Some(device_id),
                        None,
                        false,
                    );
                    self.upsert_transfer(share_text_transfer(
                        device_id,
                        device_name,
                        "Text",
                        filename,
                        text,
                        extracted_url.as_deref(),
                        "Completed",
                        None,
                    ));
                }
                Err(e) => {
                    log::error!(
                        "[share-in] Failed to copy shared text from '{}' to clipboard: {}",
                        device_name,
                        e
                    );
                    Self::show_macos_notification(
                        "Could not copy shared text",
                        &format!("From {}: {}", device_name, e),
                        Some(device_id),
                        None,
                        false,
                    );
                    self.upsert_transfer(share_text_transfer(
                        device_id,
                        device_name,
                        "Text",
                        filename,
                        text,
                        extracted_url.as_deref(),
                        "Failed",
                        Some(e.to_string()),
                    ));
                }
            }
            return;
        }

        // Case 3: Shared URL -> Automatically open in default macOS browser (matches kdeconnect-kde & Soduto)
        if let Some(url) = packet.get_str("url") {
            let trimmed_url = url.trim();
            log::info!(
                "[share-in] Received shared URL from '{}' ({} chars)",
                device_name,
                trimmed_url.len()
            );

            let is_safe_scheme = trimmed_url.starts_with("http://")
                || trimmed_url.starts_with("https://")
                || trimmed_url.starts_with("mailto:");

            let mut open_err: Option<String> = None;
            if is_safe_scheme {
                match open::that(trimmed_url) {
                    Ok(()) => {
                        log::info!("[share-in] Auto-opened shared URL in default browser");
                    }
                    Err(e) => {
                        log::warn!("[share-in] Failed to auto-open shared URL: {}", e);
                        open_err = Some(format!("Failed to auto-open: {}", e));
                    }
                }
            } else {
                log::warn!("[share-in] Blocked auto-opening URL with unsupported scheme");
            }

            self.upsert_transfer(TransferItem {
                id: Uuid::new_v4().to_string(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                direction: "Incoming".to_string(),
                kind: "Url".to_string(),
                filename: trimmed_url.to_string(),
                total_bytes: 0,
                transferred_bytes: 0,
                status: "Completed".to_string(),
                saved_path: None,
                text_content: None,
                url_content: Some(trimmed_url.to_string()),
                error: open_err,
                timestamp_ms: current_millis(),
            });
        }
    }

    async fn download_payload_file(
        &self,
        transfer_id: &str,
        device_id: &str,
        _device_name: &str,
        peer_ip: IpAddr,
        port: u16,
        expected_bytes: u64,
        dest_path: &Path,
    ) -> Result<PathBuf> {
        let (expected_ip, expected_cert) = self.payload_peer_identity(device_id)?;
        if peer_ip != expected_ip {
            return Err(anyhow!(
                "Payload peer IP mismatch for device {} (main link {}, payload {})",
                device_id,
                expected_ip,
                peer_ip
            ));
        }

        let target = SocketAddr::new(expected_ip, port);
        log::debug!("[share-in] Connecting to payload socket at {}...", target);
        let tcp = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(target))
            .await
            .context("Payload TCP connection timed out")?
            .context("Payload TCP connection failed")?;

        let server_name = ServerName::try_from("kdeconnect.local")
            .unwrap_or_else(|_| ServerName::IpAddress(peer_ip.into()));

        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(10),
            self.tls_connector.connect(server_name, tcp),
        )
        .await
        .context("Payload TLS handshake timed out")?
        .context("Payload TLS handshake failed")?;

        let peer_cert = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first());
        Self::verify_pinned_payload_cert(device_id, peer_cert, &expected_cert)?;

        log::info!(
            "[share-in] Payload TLS client connected to {}, streaming to {:?}",
            target,
            dest_path
        );

        let mut file = tokio::fs::File::create(dest_path)
            .await
            .context("Failed to create destination file")?;

        let mut buf = vec![0u8; 64 * 1024];
        let mut received: u64 = 0;
        let mut last_notify_ms = 0i64;

        loop {
            let n = tls_stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).await?;
            received += n as u64;

            let now = current_millis();
            if now - last_notify_ms >= 120 || (expected_bytes > 0 && received >= expected_bytes) {
                last_notify_ms = now;
                self.update_transfer_progress(transfer_id, received);
            }

            if expected_bytes > 0 && received >= expected_bytes {
                break;
            }
        }

        if expected_bytes > 0 && received < expected_bytes {
            return Err(anyhow!(
                "Transfer interrupted ({} of {} bytes received)",
                received,
                expected_bytes
            ));
        }

        file.flush().await?;
        self.update_transfer_progress(transfer_id, received);
        Ok(dest_path.to_path_buf())
    }
}

fn share_text_transfer(
    device_id: &str,
    device_name: &str,
    kind: &str,
    filename: String,
    text: &str,
    url_content: Option<&str>,
    status: &str,
    error: Option<String>,
) -> TransferItem {
    let size = text.len() as u64;
    TransferItem {
        id: Uuid::new_v4().to_string(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        direction: "Incoming".to_string(),
        kind: kind.to_string(),
        filename,
        total_bytes: size,
        transferred_bytes: size,
        status: status.to_string(),
        saved_path: None,
        text_content: Some(text.to_string()),
        url_content: url_content.map(str::to_string),
        error,
        timestamp_ms: current_millis(),
    }
}

/// Returns the URL only when the entire shared text is one safe URL token.
fn single_safe_share_url(text: &str) -> Option<&str> {
    let url = text.trim();
    if url.is_empty() || url.split_whitespace().count() != 1 {
        return None;
    }

    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .or_else(|| lower.strip_prefix("mailto:"))?;

    if rest.is_empty() {
        None
    } else {
        Some(url)
    }
}

#[cfg(test)]
mod tests {
    use super::single_safe_share_url;

    #[test]
    fn accepts_a_single_safe_url() {
        assert_eq!(
            single_safe_share_url(" https://example.com/a?b=1 "),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(
            single_safe_share_url("HTTP://example.com"),
            Some("HTTP://example.com")
        );
        assert_eq!(
            single_safe_share_url("mailto:a@example.com"),
            Some("mailto:a@example.com")
        );
    }

    #[test]
    fn rejects_urls_embedded_in_text() {
        assert_eq!(single_safe_share_url("Open https://example.com"), None);
        assert_eq!(
            single_safe_share_url("https://a.example\nhttps://b.example"),
            None
        );
        assert_eq!(single_safe_share_url("https://"), None);
        assert_eq!(single_safe_share_url("ftp://example.com"), None);
        assert_eq!(single_safe_share_url(""), None);
    }
}
