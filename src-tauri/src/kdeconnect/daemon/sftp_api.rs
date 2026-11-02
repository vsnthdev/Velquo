use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use crate::kdeconnect::sftp::*;
use anyhow::{anyhow, Result};
use std::path::Path;

impl KdeConnectDaemon {
    pub async fn sftp_browse(
        &self,
        device_id: &str,
        path: Option<String>,
    ) -> Result<SftpBrowseResponse> {
        if !self.is_device_paired(device_id) {
            return Err(anyhow!("Device is not paired"));
        }

        self.sftp_manager
            .browse(device_id, path, &self.config_dir, || {
                self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing())
            })
            .await
    }

    pub async fn sftp_download_file(
        &self,
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
            .download_file(device_id, remote_path, &dest_path, &self.config_dir, || {
                self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing())
            })
            .await?;

        Ok(saved.to_string_lossy().to_string())
    }

    pub async fn sftp_delete_entry(
        &self,
        device_id: &str,
        remote_path: &str,
        is_dir: bool,
    ) -> Result<()> {
        if !self.is_device_paired(device_id) {
            return Err(anyhow!("Device is not paired"));
        }

        self.sftp_manager
            .delete_entry(device_id, remote_path, is_dir, &self.config_dir, || {
                self.send_packet_to_device(device_id, &NetworkPacket::sftp_start_browsing())
            })
            .await
    }
}
