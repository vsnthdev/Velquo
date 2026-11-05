use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use crate::kdeconnect::sftp::*;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
enum SshHostKeyCheck {
    Matched,
    Pinned,
    Mismatch(String),
}

fn check_ssh_host_key(record: &mut TrustedDeviceRecord, observed: &str) -> SshHostKeyCheck {
    match record.ssh_host_key_fingerprint.as_deref() {
        Some(expected) if expected == observed => SshHostKeyCheck::Matched,
        Some(expected) => SshHostKeyCheck::Mismatch(expected.to_string()),
        None => {
            record.ssh_host_key_fingerprint = Some(observed.to_string());
            SshHostKeyCheck::Pinned
        }
    }
}

impl KdeConnectDaemon {
    fn ssh_host_key_verifier(self: &Arc<Self>, device_id: &str) -> SshHostKeyVerifier {
        let daemon = Arc::clone(self);
        let device_id = device_id.to_string();

        Arc::new(move |fingerprint: &str| {
            let mut trusted = match daemon.trusted_devices.lock() {
                Ok(trusted) => trusted,
                Err(_) => return false,
            };
            let Some(record) = trusted.get_mut(&device_id) else {
                return false;
            };

            match check_ssh_host_key(record, fingerprint) {
                SshHostKeyCheck::Matched => true,
                SshHostKeyCheck::Mismatch(expected) => {
                    log::warn!(
                        "[sftp] SSH host key mismatch for '{}' (expected {}, got {})",
                        device_id,
                        expected,
                        fingerprint
                    );
                    false
                }
                SshHostKeyCheck::Pinned => {
                    log::info!(
                        "[sftp] Pinned SSH host key for '{}': {}",
                        device_id,
                        fingerprint
                    );
                    drop(trusted);
                    daemon.save_trusted_devices();
                    true
                }
            }
        })
    }

    pub async fn sftp_browse(
        self: &Arc<Self>,
        device_id: &str,
        path: Option<String>,
    ) -> Result<SftpBrowseResponse> {
        if !self.is_device_paired(device_id) {
            return Err(anyhow!("Device is not paired"));
        }

        self.sftp_manager
            .browse(
                device_id,
                path,
                &self.config_dir,
                self.ssh_host_key_verifier(device_id),
                || self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing()),
            )
            .await
    }

    pub async fn sftp_download_file(
        self: &Arc<Self>,
        device_id: &str,
        remote_path: &str,
        file_name: &str,
    ) -> Result<String> {
        if !self.is_device_paired(device_id) {
            return Err(anyhow!("Device is not paired"));
        }

        let clean_name = Path::new(file_name)
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("downloaded_file")
            .to_string();
        let dest_path = self.unique_download_path(device_id, &clean_name);

        let saved = self
            .sftp_manager
            .download_file(
                device_id,
                remote_path,
                &dest_path,
                &self.config_dir,
                self.ssh_host_key_verifier(device_id),
                || self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing()),
            )
            .await?;

        Ok(saved.to_string_lossy().to_string())
    }

    pub async fn sftp_delete_entry(
        self: &Arc<Self>,
        device_id: &str,
        remote_path: &str,
        is_dir: bool,
    ) -> Result<()> {
        if !self.is_device_paired(device_id) {
            return Err(anyhow!("Device is not paired"));
        }

        self.sftp_manager
            .delete_entry(
                device_id,
                remote_path,
                is_dir,
                &self.config_dir,
                self.ssh_host_key_verifier(device_id),
                || self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing()),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fingerprint: Option<&str>) -> TrustedDeviceRecord {
        TrustedDeviceRecord {
            device_id: "device".to_string(),
            device_name: "Phone".to_string(),
            device_type: "phone".to_string(),
            protocol_version: 8,
            cert_der: Vec::new(),
            sync_clipboard: true,
            allow_presenter: true,
            sync_notifications: true,
            auto_copy_otp: true,
            pause_media_on_calls: true,
            save_location: None,
            run_commands: Vec::new(),
            ssh_host_key_fingerprint: fingerprint.map(str::to_string),
        }
    }

    #[test]
    fn pins_host_key_on_first_use() {
        let mut record = record(None);
        assert_eq!(
            check_ssh_host_key(&mut record, "SHA256:first"),
            SshHostKeyCheck::Pinned
        );
        assert_eq!(
            record.ssh_host_key_fingerprint.as_deref(),
            Some("SHA256:first")
        );
    }

    #[test]
    fn accepts_matching_host_key() {
        let mut record = record(Some("SHA256:first"));
        assert_eq!(
            check_ssh_host_key(&mut record, "SHA256:first"),
            SshHostKeyCheck::Matched
        );
    }

    #[test]
    fn rejects_changed_host_key() {
        let mut record = record(Some("SHA256:first"));
        assert_eq!(
            check_ssh_host_key(&mut record, "SHA256:second"),
            SshHostKeyCheck::Mismatch("SHA256:first".to_string())
        );
        assert_eq!(
            record.ssh_host_key_fingerprint.as_deref(),
            Some("SHA256:first")
        );
    }
}
