mod incoming;
mod outgoing;
mod payload;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::current_millis;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

/// Prefix for in-progress downloads. The leading dot keeps Finder, Dock stacks
/// and Spotlight from surfacing the file while it is still being written, and
/// the `.part` suffix keeps QuickLook from trying to thumbnail a half-written
/// image.
const STAGING_PREFIX: &str = ".velquo-staging-";
const STAGING_SUFFIX: &str = ".part";

/// How long a received-file batch waits after the last file before notifying.
/// Android sends one `share.request` packet per file, so a short quiet period
/// is what groups a multi-file share into one notification.
const RECEIVED_FILE_BATCH_IDLE_MS: i64 = 1_200;

/// Serializes "pick a unique name, then rename into place" so two simultaneous
/// transfers of `photo.jpg` cannot pick the same destination.
static DOWNLOAD_PATH_LOCK: Mutex<()> = Mutex::new(());

/// Completed files waiting to be announced together for one device.
#[derive(Debug)]
pub(crate) struct ReceivedFileBatch {
    device_name: String,
    paths: Vec<PathBuf>,
    generation: u64,
}

impl ReceivedFileBatch {
    fn new(device_name: &str) -> Self {
        Self {
            device_name: device_name.to_string(),
            paths: Vec::new(),
            generation: 0,
        }
    }

    /// Adds a completed file and bumps the generation, invalidating timers from
    /// earlier files in the same batch.
    fn push(&mut self, device_name: &str, path: PathBuf) -> u64 {
        self.device_name = device_name.to_string();
        self.paths.push(path);
        self.generation += 1;
        self.generation
    }
}

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

    /// Path to a hidden staging file in the device's save folder. Downloads are
    /// streamed here and only renamed to their final name once complete, so
    /// Finder/QuickLook never observe a partially written file.
    pub(super) fn staging_path(&self, device_id: &str) -> PathBuf {
        self.resolve_device_save_dir(device_id).join(format!(
            "{STAGING_PREFIX}{}{STAGING_SUFFIX}",
            Uuid::new_v4()
        ))
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

    /// Buffers a completed incoming file so the whole share gesture produces a
    /// single notification. A timer keyed on the batch generation flushes it
    /// once no further file has arrived for [`RECEIVED_FILE_BATCH_IDLE_MS`].
    pub(super) fn record_received_file(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        path: PathBuf,
    ) {
        let generation = {
            let mut batches = self.received_file_batches.lock().unwrap();
            let batch = batches
                .entry(device_id.to_string())
                .or_insert_with(|| ReceivedFileBatch::new(device_name));
            batch.push(device_name, path)
        };

        let daemon = Arc::clone(self);
        let device_id = device_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(RECEIVED_FILE_BATCH_IDLE_MS as u64)).await;
            daemon.flush_received_files(&device_id, generation);
        });
    }

    /// Notifies the completed files for `device_id` when `generation` is still
    /// the newest one. While another transfer from that device is in progress,
    /// waits again instead of announcing a partial batch.
    fn flush_received_files(self: &Arc<Self>, device_id: &str, generation: u64) {
        let pending_transfer = self.has_pending_incoming_file(device_id);

        let batch = {
            let mut batches = self.received_file_batches.lock().unwrap();
            match batches.get(device_id) {
                Some(batch) if batch.generation == generation => {}
                _ => return,
            }
            if pending_transfer {
                None
            } else {
                batches.remove(device_id)
            }
        };

        if pending_transfer {
            let daemon = Arc::clone(self);
            let device_id = device_id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(RECEIVED_FILE_BATCH_IDLE_MS as u64)).await;
                daemon.flush_received_files(&device_id, generation);
            });
            return;
        }

        let Some(batch) = batch else {
            return;
        };
        if batch.paths.is_empty() {
            return;
        }

        let paths: Vec<String> = batch
            .paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        log::info!(
            "[share-in] Announcing {} received file(s) from '{}'",
            paths.len(),
            batch.device_name
        );
        Self::notify_files_received(device_id, &batch.device_name, &paths);
    }

    fn has_pending_incoming_file(&self, device_id: &str) -> bool {
        self.transfers.lock().unwrap().iter().any(|transfer| {
            transfer.device_id == device_id
                && transfer.direction == "Incoming"
                && transfer.kind == "File"
                && transfer.status == "InProgress"
        })
    }
}

/// Picks a non-colliding path for `filename` inside `base_dir`, appending
/// ` (1)`, ` (2)`, … when the name is taken.
fn unique_path_in_dir(base_dir: &Path, filename: &str) -> PathBuf {
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

/// Moves a fully written staging file into its final name inside the same
/// directory.
///
/// The staging file lives in the destination folder, so this is a single
/// atomic `rename(2)` on the same filesystem: no byte is copied and the final
/// name only appears once the content is complete. That keeps Finder, Dock and
/// QuickLook from generating thumbnails of a half-written file.
pub(super) async fn finalize_download(staged: &Path, filename: &str) -> Result<PathBuf> {
    let staged = staged.to_path_buf();
    let filename = filename.to_string();

    tokio::task::spawn_blocking(move || {
        let base_dir = staged
            .parent()
            .ok_or_else(|| anyhow!("Staged download '{}' has no parent", staged.display()))?
            .to_path_buf();

        // Held only inside the blocking task so the future stays `Send`.
        let _guard = DOWNLOAD_PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let final_path = unique_path_in_dir(&base_dir, &filename);

        fs::rename(&staged, &final_path).with_context(|| {
            format!(
                "Failed to move staged download into place at '{}'",
                final_path.display()
            )
        })?;
        Ok(final_path)
    })
    .await
    .context("Download finalization task panicked")?
}

/// Removes staging files left behind by a crash or a hard quit.
///
/// Only called at startup, when no transfer can be in flight, so it is safe to
/// sweep every staged file in the folder.
pub(crate) fn sweep_stale_staging_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(STAGING_PREFIX) || !name.ends_with(STAGING_SUFFIX) {
            continue;
        }
        if fs::remove_file(entry.path()).is_ok() {
            log::info!(
                "[share] Removed stale staging file '{}'",
                entry.path().display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("velquo-share-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unique_path_in_dir_appends_a_counter() {
        let dir = temp_dir();
        assert_eq!(unique_path_in_dir(&dir, "photo.jpg"), dir.join("photo.jpg"));

        fs::write(dir.join("photo.jpg"), b"first").unwrap();
        assert_eq!(
            unique_path_in_dir(&dir, "photo.jpg"),
            dir.join("photo (1).jpg")
        );

        fs::write(dir.join("photo (1).jpg"), b"second").unwrap();
        assert_eq!(
            unique_path_in_dir(&dir, "photo.jpg"),
            dir.join("photo (2).jpg")
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn finalize_download_renames_in_place() {
        let dir = temp_dir();
        let staged = dir.join(format!(
            "{STAGING_PREFIX}{}{STAGING_SUFFIX}",
            Uuid::new_v4()
        ));
        fs::write(&staged, b"complete image bytes").unwrap();

        let saved = finalize_download(&staged, "photo.jpg").await.unwrap();

        assert_eq!(saved, dir.join("photo.jpg"));
        assert_eq!(fs::read(&saved).unwrap(), b"complete image bytes");
        assert!(!staged.exists(), "staging file should be gone");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sweep_removes_only_staging_files() {
        let dir = temp_dir();
        let staged = dir.join(format!(
            "{STAGING_PREFIX}{}{STAGING_SUFFIX}",
            Uuid::new_v4()
        ));
        fs::write(&staged, b"orphan").unwrap();
        let real = dir.join("keep-me.jpg");
        fs::write(&real, b"real").unwrap();

        sweep_stale_staging_files(&dir);

        assert!(!staged.exists(), "orphaned staging file should be removed");
        assert!(real.exists(), "completed downloads must be left alone");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn received_file_batch_groups_files_and_bumps_generation() {
        let mut batch = ReceivedFileBatch::new("Pixel 8");
        assert_eq!(
            batch.push("Pixel 8", PathBuf::from("/tmp/a.jpg")),
            1,
            "first file starts a new generation"
        );
        assert_eq!(
            batch.push("Pixel 8", PathBuf::from("/tmp/b.jpg")),
            2,
            "later files invalidate earlier flush timers"
        );
        assert_eq!(batch.paths.len(), 2);
        assert_eq!(batch.device_name, "Pixel 8");
    }
}
