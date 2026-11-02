use crate::kdeconnect::daemon::*;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

impl KdeConnectDaemon {
    pub(super) fn trusted_devices_file(config_dir: &Path) -> PathBuf {
        config_dir.join("trusted_devices.json")
    }

    pub(super) fn load_trusted_devices(config_dir: &Path) -> HashMap<String, TrustedDeviceRecord> {
        let path = Self::trusted_devices_file(config_dir);
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, TrustedDeviceRecord>>(&data) {
                log::info!(
                    "[config] Loaded {} trusted device(s) from {:?}",
                    map.len(),
                    path
                );
                return map;
            }
        }
        HashMap::new()
    }

    pub(super) fn save_trusted_devices(&self) {
        let path = Self::trusted_devices_file(&self.config_dir);
        if let Ok(guard) = self.trusted_devices.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                if let Err(e) = fs::write(&path, json) {
                    log::error!("[config] Failed to save trusted devices: {}", e);
                } else {
                    log::info!(
                        "[config] Saved {} trusted device(s) to {:?}",
                        guard.len(),
                        path
                    );
                }
            }
        }
        let trusted_list = self.get_trusted_devices_menu_list();
        if let Ok(guard) = self.app_handle.lock() {
            if let Some(app) = guard.as_ref() {
                crate::refresh_tray_menu(app, trusted_list);
            }
        }
    }

    pub fn get_trusted_devices_menu_list(&self) -> Vec<(String, String, String)> {
        let mut list: Vec<(String, String, String)> = self
            .trusted_devices
            .lock()
            .map(|guard| {
                guard
                    .values()
                    .map(|rec| {
                        (
                            rec.device_id.clone(),
                            rec.device_name.clone(),
                            rec.device_type.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        list.sort_by(|a, b| a.1.cmp(&b.1));
        list
    }

    pub(super) fn app_settings_file(config_dir: &Path) -> PathBuf {
        config_dir.join("app_settings.json")
    }

    pub(super) fn load_app_settings(config_dir: &Path) -> AppSettings {
        let path = Self::app_settings_file(config_dir);
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&data) {
                return settings;
            }
        }
        AppSettings::default()
    }

    pub(super) fn save_app_settings(&self) {
        let path = Self::app_settings_file(&self.config_dir);
        if let Ok(guard) = self.app_settings.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                let _ = fs::write(&path, json);
            }
        }
    }

    pub fn is_ignore_whatsapp_conversations_enabled(&self) -> bool {
        self.app_settings
            .lock()
            .map(|s| s.ignore_whatsapp_conversations)
            .unwrap_or(true)
    }

    pub fn set_ignore_whatsapp_conversations(&self, enabled: bool) -> Result<()> {
        {
            let mut settings = self.app_settings.lock().unwrap();
            settings.ignore_whatsapp_conversations = enabled;
        }
        self.save_app_settings();

        if enabled {
            if let Ok(mut devices) = self.devices.lock() {
                for entry in devices.values_mut() {
                    entry.view.notifications.retain(|n| {
                        let is_wa = n.app_name.eq_ignore_ascii_case("whatsapp")
                            || n.app_name.eq_ignore_ascii_case("whatsapp business")
                            || n.id.contains("com.whatsapp");
                        !(is_wa && n.otp_code.is_none())
                    });
                }
            }
        }

        self.notify_ui();
        Ok(())
    }
}
