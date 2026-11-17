use crate::kdeconnect::daemon::*;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

impl KdeConnectDaemon {
    pub fn is_homebrew_install(&self) -> bool {
        self.config_dir.join("HomebrewInstall").is_file()
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("velquo-{}-{}", label, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn homebrew_install_marker_detection_follows_file_presence() {
        let config_dir = unique_temp_dir("homebrew-marker");
        let downloads_dir = config_dir.join("downloads");
        let log_path = config_dir.join("app.log");
        let daemon = KdeConnectDaemon::new(config_dir.clone(), downloads_dir, log_path).unwrap();

        assert!(!daemon.is_homebrew_install());

        let marker_dir_path = config_dir.join("HomebrewInstall");
        fs::create_dir_all(&marker_dir_path).unwrap();
        assert!(!daemon.is_homebrew_install());
        fs::remove_dir_all(&marker_dir_path).unwrap();

        fs::write(&marker_dir_path, b"").unwrap();
        assert!(daemon.is_homebrew_install());

        let _ = fs::remove_dir_all(&config_dir);
    }
}
