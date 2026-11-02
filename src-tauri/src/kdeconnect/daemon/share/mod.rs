mod incoming;
mod outgoing;
mod payload;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::current_millis;
use std::fs;
use std::path::{Path, PathBuf};

impl KdeConnectDaemon {
    pub(super) fn resolve_device_save_dir(&self, device_id: &str) -> PathBuf {
        let configured = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .map(|d| d.view.save_location.trim().to_string())
                .filter(|s| !s.is_empty())
        }
        .or_else(|| {
            let trusted = self.trusted_devices.lock().unwrap();
            trusted
                .get(device_id)
                .and_then(|r| r.save_location.clone())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });

        if let Some(dir_str) = configured {
            let dir = PathBuf::from(dir_str);
            if fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }
        let _ = fs::create_dir_all(&self.downloads_dir);
        self.downloads_dir.clone()
    }

    pub(super) fn unique_download_path(&self, device_id: &str, filename: &str) -> PathBuf {
        let base_dir = self.resolve_device_save_dir(device_id);
        let candidate = base_dir.join(filename);
        if !candidate.exists() {
            return candidate;
        }
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default();

        for i in 1..10000 {
            let next = base_dir.join(format!("{} ({}){}", stem, i, ext));
            if !next.exists() {
                return next;
            }
        }
        base_dir.join(format!("{}-{}", current_millis(), filename))
    }

    pub(super) fn prune_finished_transfers(list: &mut Vec<TransferItem>) {
        let mut finished_count = 0usize;
        list.retain(|t| {
            if t.status == "InProgress" {
                true
            } else {
                finished_count += 1;
                finished_count <= 30
            }
        });
    }

    pub(super) fn upsert_transfer(&self, item: TransferItem) {
        let mut guard = self.transfers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|t| t.id == item.id) {
            *existing = item;
        } else {
            guard.insert(0, item);
        }
        Self::prune_finished_transfers(&mut guard);
        drop(guard);
        self.notify_ui();
    }

    pub(super) fn update_transfer_progress(&self, transfer_id: &str, bytes: u64) {
        let mut guard = self.transfers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|t| t.id == transfer_id) {
            existing.transferred_bytes = bytes;
        }
        drop(guard);
        self.notify_ui();
    }

    pub(super) fn finish_transfer(
        &self,
        transfer_id: &str,
        status: &str,
        saved_path: Option<String>,
        error: Option<String>,
    ) {
        let mut guard = self.transfers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|t| t.id == transfer_id) {
            existing.status = status.to_string();
            if status == "Completed" && existing.total_bytes > 0 {
                existing.transferred_bytes = existing.total_bytes;
            }
            if let Some(p) = saved_path {
                existing.saved_path = Some(p);
            }
            existing.error = error;
            existing.timestamp_ms = current_millis();
        }
        Self::prune_finished_transfers(&mut guard);
        drop(guard);
        self.notify_ui();
    }
}
