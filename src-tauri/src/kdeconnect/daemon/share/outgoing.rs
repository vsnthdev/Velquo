use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Context, Result};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use uuid::Uuid;

impl KdeConnectDaemon {
    pub fn send_url_to_device(&self, device_id: &str, url: &str) -> Result<String> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("URL cannot be empty"));
        }

        let lower = trimmed.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(anyhow!("URL must start with http:// or https://"));
        }

        let device_name = {
            let devices = self.devices.lock().unwrap();
            let entry = devices
                .get(device_id)
                .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
            if entry.view.pair_state != PairState::Paired {
                return Err(anyhow!("Device '{}' is not paired", entry.view.device_name));
            }
            if !entry.view.reachable {
                return Err(anyhow!("Device '{}' is offline", entry.view.device_name));
            }
            entry.view.device_name.clone()
        };

        let pkt = NetworkPacket::share_url(trimmed);
        self.send_packet_to_device(device_id, &pkt)?;

        log::info!(
            "[share-out] Shared URL to '{}' ({}) ({} chars)",
            device_name,
            device_id,
            trimmed.len()
        );

        let transfer_id = Uuid::new_v4().to_string();
        self.upsert_transfer(TransferItem {
            id: transfer_id.clone(),
            device_id: device_id.to_string(),
            device_name,
            direction: "Outgoing".to_string(),
            kind: "Url".to_string(),
            filename: trimmed.to_string(),
            total_bytes: 0,
            transferred_bytes: 0,
            status: "Completed".to_string(),
            saved_path: None,
            text_content: None,
            url_content: Some(trimmed.to_string()),
            error: None,
            timestamp_ms: current_millis(),
        });

        Ok(transfer_id)
    }

    /// Sends one or more local files sequentially in a single KDE Connect batch (`CompositeUploadJob` + `UploadJob`)
    pub async fn send_files_to_device(
        self: &Arc<Self>,
        device_id: &str,
        file_paths: Vec<PathBuf>,
    ) -> Result<Vec<String>> {
        if file_paths.is_empty() {
            return Ok(Vec::new());
        }

        let device_name = {
            let devices = self.devices.lock().unwrap();
            let entry = devices
                .get(device_id)
                .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
            if entry.view.pair_state != PairState::Paired {
                return Err(anyhow!("Device '{}' is not paired", entry.view.device_name));
            }
            if !entry.view.reachable {
                return Err(anyhow!("Device '{}' is offline", entry.view.device_name));
            }
            entry.view.device_name.clone()
        };

        let mut batch_items: Vec<(String, PathBuf, String, u64)> =
            Vec::with_capacity(file_paths.len());
        let mut total_payload_size: u64 = 0;

        for file_path in file_paths {
            let metadata = tokio::fs::metadata(&file_path)
                .await
                .with_context(|| format!("Cannot read file metadata for {:?}", file_path))?;
            if !metadata.is_file() {
                continue;
            }
            let file_size = metadata.len();
            let filename = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shared_file")
                .to_string();
            let transfer_id = Uuid::new_v4().to_string();
            total_payload_size = total_payload_size.saturating_add(file_size);
            batch_items.push((transfer_id, file_path, filename, file_size));
        }

        if batch_items.is_empty() {
            return Err(anyhow!("No valid regular files to send"));
        }

        let number_of_files = batch_items.len();

        // Bind a single TCP listener on 1739..=1764 for the sequential batch upload (`CompositeUploadJob`)
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
        let listener = payload_listener.ok_or_else(|| {
            anyhow!(
                "No available payload port in range {}..={}",
                MIN_PAYLOAD_PORT,
                MAX_PAYLOAD_PORT
            )
        })?;

        log::info!(
            "[share-out] Bound payload TCP server on port {} for batch uploading {} file(s) ({} total bytes) to '{}'",
            payload_port,
            number_of_files,
            total_payload_size,
            device_name
        );

        let base_ms = current_millis();
        let mut transfer_ids = Vec::with_capacity(number_of_files);

        // Insert all batch items under a single lock so item 0 appears at index 0 and notify_ui() fires once
        {
            let mut guard = self.transfers.lock().unwrap();
            for (idx, (transfer_id, file_path, filename, file_size)) in
                batch_items.iter().enumerate().rev()
            {
                guard.insert(
                    0,
                    TransferItem {
                        id: transfer_id.clone(),
                        device_id: device_id.to_string(),
                        device_name: device_name.clone(),
                        direction: "Outgoing".to_string(),
                        kind: "File".to_string(),
                        filename: filename.clone(),
                        total_bytes: *file_size,
                        transferred_bytes: 0,
                        status: "InProgress".to_string(),
                        saved_path: Some(file_path.to_string_lossy().to_string()),
                        text_content: None,
                        url_content: None,
                        error: None,
                        timestamp_ms: base_ms + (number_of_files.saturating_sub(idx) as i64),
                    },
                );
            }
            Self::prune_finished_transfers(&mut guard);
        }
        self.notify_ui();

        for (tid, _, _, _) in &batch_items {
            transfer_ids.push(tid.clone());
        }

        let daemon = Arc::clone(self);
        let dev_id = device_id.to_string();
        let dev_name = device_name.clone();
        tokio::spawn(async move {
            for (idx, (tid, path_buf, fname, file_size)) in batch_items.into_iter().enumerate() {
                let now_ms = current_millis();
                let share_packet = NetworkPacket::share_file_header(
                    &fname,
                    file_size,
                    now_ms,
                    now_ms,
                    payload_port,
                    number_of_files,
                    total_payload_size,
                );

                log::info!(
                    "[share-out] Sending file {}/{} ({} bytes) on port {} to '{}'",
                    idx + 1,
                    number_of_files,
                    file_size,
                    payload_port,
                    dev_name
                );

                if let Err(e) = daemon.send_packet_to_device(&dev_id, &share_packet) {
                    log::error!(
                        "[share-out] Failed to send share header {}/{} to '{}': {:?}",
                        idx + 1,
                        number_of_files,
                        dev_name,
                        e
                    );
                    daemon.finish_transfer(&tid, "Failed", None, Some(e.to_string()));
                    continue;
                }

                match daemon
                    .serve_outgoing_payload(&dev_id, &tid, &listener, &path_buf, file_size)
                    .await
                {
                    Ok(()) => {
                        log::info!(
                            "[share-out] Successfully uploaded {}/{} ({} bytes) to '{}'",
                            idx + 1,
                            number_of_files,
                            file_size,
                            dev_name
                        );
                        daemon.finish_transfer(
                            &tid,
                            "Completed",
                            Some(path_buf.to_string_lossy().to_string()),
                            None,
                        );
                    }
                    Err(e) => {
                        log::error!(
                            "[share-out] Upload {}/{} to '{}' failed: {:?}",
                            idx + 1,
                            number_of_files,
                            dev_name,
                            e
                        );
                        daemon.finish_transfer(&tid, "Failed", None, Some(e.to_string()));
                    }
                }
            }
        });

        Ok(transfer_ids)
    }

    /// Sends a single local file to a paired KDE Connect device
    pub async fn send_file_to_device(
        self: &Arc<Self>,
        device_id: &str,
        file_path: &Path,
    ) -> Result<String> {
        let mut ids = self
            .send_files_to_device(device_id, vec![file_path.to_path_buf()])
            .await?;
        ids.pop()
            .ok_or_else(|| anyhow!("Failed to enqueue file transfer"))
    }
}
