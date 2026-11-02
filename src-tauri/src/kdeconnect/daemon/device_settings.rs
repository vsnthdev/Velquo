use crate::kdeconnect::daemon::*;
use crate::kdeconnect::macos_media::disown_call_paused_media;
use crate::kdeconnect::packet::*;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

impl KdeConnectDaemon {
    pub fn set_device_pause_media_on_calls(&self, device_id: &str, enabled: bool) -> Result<()> {
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.pause_media_on_calls = enabled;
                log::info!(
                    "[pause-music] Set pause_media_on_calls={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        if !enabled {
            let prefix = format!("{}:", device_id);
            let mut state = self.call_media_state.lock().unwrap();
            let prev_len = state.active_events.len();
            state.active_events.retain(|ev| !ev.starts_with(&prefix));
            if state.active_events.len() != prev_len {
                state.seq = state.seq.wrapping_add(1);
                if state.active_events.is_empty() {
                    state.paused_sources.clear();
                    state.paused_at = None;
                    state.paused_at_secs = None;
                    disown_call_paused_media("pause_media_on_calls disabled for device");
                }
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.pause_media_on_calls = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }

    pub fn set_device_auto_copy_otp(&self, device_id: &str, enabled: bool) -> Result<()> {
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.auto_copy_otp = enabled;
                log::info!(
                    "[otp] Set auto_copy_otp={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.auto_copy_otp = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }

    pub fn set_device_notifications_sync(&self, device_id: &str, enabled: bool) -> Result<()> {
        let mut is_online_and_paired = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.sync_notifications = enabled;
                if !enabled {
                    entry.view.notifications.clear();
                }
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[notifications] Set sync_notifications={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.sync_notifications = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if enabled && is_online_and_paired {
            let _ = self.send_packet_to_device(device_id, &NetworkPacket::notification_request());
        }

        Ok(())
    }

    pub fn set_device_presenter(&self, device_id: &str, enabled: bool) -> Result<()> {
        let mut is_online_and_paired = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.allow_presenter = enabled;
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[presenter-input] Set allow_presenter={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.allow_presenter = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if is_online_and_paired {
            let _ = self
                .send_packet_to_device(device_id, &NetworkPacket::mousepad_keyboard_state(enabled));
        }

        Ok(())
    }

    pub fn set_device_clipboard_sync(&self, device_id: &str, enabled: bool) -> Result<()> {
        let mut is_online_and_paired = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.sync_clipboard = enabled;
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[clipboard] Set sync_clipboard={} for '{}' ({})",
                    enabled,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.sync_clipboard = enabled;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if enabled && is_online_and_paired {
            self.send_host_clipboard_connect_to_device(device_id);
        }

        Ok(())
    }

    pub fn set_device_save_location(&self, device_id: &str, save_location: &str) -> Result<()> {
        let trimmed = save_location.trim();
        let target_dir = if trimmed.is_empty() {
            self.downloads_dir.clone()
        } else {
            let p = PathBuf::from(trimmed);
            fs::create_dir_all(&p)
                .with_context(|| format!("Cannot create or access directory {:?}", p))?;
            p
        };
        let target_str = target_dir.to_string_lossy().to_string();

        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.save_location = target_str.clone();
                log::info!(
                    "[settings] Set save_location='{}' for '{}' ({})",
                    target_str,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.save_location = Some(target_str);
            }
        }
        self.save_trusted_devices();
        self.notify_ui();
        Ok(())
    }

    pub fn ring_device(&self, device_id: &str) -> Result<()> {
        let pkt = NetworkPacket::find_my_phone();
        self.send_packet_to_device(device_id, &pkt)
    }
}
