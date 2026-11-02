use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use uuid::Uuid;

impl KdeConnectDaemon {
    pub(super) fn serialize_command_list_json(commands: &[RunCommandItem]) -> String {
        let mut map = serde_json::Map::new();
        for item in commands {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), serde_json::json!(item.name));
            obj.insert("command".into(), serde_json::json!(item.command));
            map.insert(item.key.clone(), serde_json::Value::Object(obj));
        }
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn send_host_runcommands_to_device(&self, device_id: &str) {
        let cmds_json = {
            let devices = self.devices.lock().unwrap();
            let Some(entry) = devices.get(device_id) else {
                return;
            };
            if entry.view.pair_state != PairState::Paired {
                return;
            }
            Self::serialize_command_list_json(&entry.view.run_commands)
        };
        let pkt = NetworkPacket::runcommand_list(&cmds_json);
        let _ = self.send_packet_to_device(device_id, &pkt);
    }

    pub fn add_device_run_command(&self, device_id: &str, name: &str, command: &str) -> Result<()> {
        let clean_name = name.trim();
        let clean_cmd = command.trim();
        if clean_name.is_empty() || clean_cmd.is_empty() {
            return Err(anyhow!("Command name and command string cannot be empty"));
        }

        let item = RunCommandItem {
            key: Uuid::new_v4().to_string(),
            name: clean_name.to_string(),
            command: clean_cmd.to_string(),
        };

        let mut is_online_and_paired = false;
        let mut updated_list = Vec::new();
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.run_commands.push(item);
                updated_list = entry.view.run_commands.clone();
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[runcommand] Added command '{}' ('{}') for '{}' ({})",
                    clean_name,
                    clean_cmd,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.run_commands = updated_list;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if is_online_and_paired {
            self.send_host_runcommands_to_device(device_id);
        }

        Ok(())
    }

    pub fn remove_device_run_command(&self, device_id: &str, key: &str) -> Result<()> {
        let mut is_online_and_paired = false;
        let mut updated_list = Vec::new();
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry.view.run_commands.retain(|c| c.key != key);
                updated_list = entry.view.run_commands.clone();
                is_online_and_paired =
                    entry.view.reachable && entry.view.pair_state == PairState::Paired;
                log::info!(
                    "[runcommand] Removed command key='{}' for '{}' ({})",
                    key,
                    entry.view.device_name,
                    device_id
                );
            }
        }

        {
            let mut trusted = self.trusted_devices.lock().unwrap();
            if let Some(rec) = trusted.get_mut(device_id) {
                rec.run_commands = updated_list;
            }
        }
        self.save_trusted_devices();
        self.notify_ui();

        if is_online_and_paired {
            self.send_host_runcommands_to_device(device_id);
        }

        Ok(())
    }

    pub(super) fn handle_runcommand_request(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        let is_paired = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .map(|d| d.view.pair_state == PairState::Paired)
                .unwrap_or(false)
        };
        if !is_paired {
            return;
        }

        if packet.get_bool("requestCommandList").unwrap_or(false) {
            self.send_host_runcommands_to_device(device_id);
            return;
        }

        if packet.body.contains_key("setup") {
            log::info!(
                "[runcommand] Device '{}' ({}) requested Run Command setup window",
                device_name,
                device_id
            );
            if let Ok(guard) = self.app_handle.lock() {
                if let Some(app) = guard.as_ref() {
                    #[cfg(target_os = "macos")]
                    {
                        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                        let _ = app.show();
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit(
                        "velquo://open-runcommand-setup",
                        serde_json::json!({ "deviceId": device_id }),
                    );
                }
            }
            return;
        }

        let Some(key) = packet.get_str("key") else {
            return;
        };

        let maybe_cmd = {
            let devices = self.devices.lock().unwrap();
            devices
                .get(device_id)
                .and_then(|d| d.view.run_commands.iter().find(|c| c.key == key).cloned())
        };

        let Some(cmd_item) = maybe_cmd else {
            log::warn!(
                "[runcommand] Device '{}' requested unknown command key '{}'",
                device_name,
                key
            );
            return;
        };

        log::info!(
            "[runcommand] Executing command '{}' (`{}`) triggered by '{}'",
            cmd_item.name,
            cmd_item.command,
            device_name
        );

        let daemon = Arc::clone(self);
        let dev_id = device_id.to_string();
        let cmd_str = cmd_item.command.clone();
        tauri::async_runtime::spawn(async move {
            let run_id = (current_millis() & 0x7FFF_FFFF) as u32;
            let mut started_body = serde_json::Map::new();
            started_body.insert("commandStarted".into(), serde_json::json!(true));
            started_body.insert("command".into(), serde_json::json!(cmd_str));
            started_body.insert("id".into(), serde_json::json!(run_id));
            let _ = daemon.send_packet_to_device(
                &dev_id,
                &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, started_body),
            );

            let cmd_for_exec = cmd_str.clone();
            let output_res = tokio::task::spawn_blocking(move || {
                std::process::Command::new("/bin/sh")
                    .args(["-c", &cmd_for_exec])
                    .output()
            })
            .await;

            match output_res {
                Ok(Ok(output)) => {
                    let stdout_lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(|s| s.to_string())
                        .collect();
                    let stderr_lines: Vec<String> = String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .map(|s| s.to_string())
                        .collect();
                    if !stdout_lines.is_empty() || !stderr_lines.is_empty() {
                        let mut out_body = serde_json::Map::new();
                        out_body.insert("commandOutput".into(), serde_json::json!(true));
                        out_body.insert("stdout".into(), serde_json::json!(stdout_lines));
                        out_body.insert("stderr".into(), serde_json::json!(stderr_lines));
                        out_body.insert("id".into(), serde_json::json!(run_id));
                        let _ = daemon.send_packet_to_device(
                            &dev_id,
                            &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, out_body),
                        );
                    }
                    let exit_code = output.status.code().unwrap_or(0);
                    let mut fin_body = serde_json::Map::new();
                    fin_body.insert("commandFinished".into(), serde_json::json!(true));
                    fin_body.insert("success".into(), serde_json::json!(output.status.success()));
                    fin_body.insert("exitCode".into(), serde_json::json!(exit_code));
                    fin_body.insert("id".into(), serde_json::json!(run_id));
                    let _ = daemon.send_packet_to_device(
                        &dev_id,
                        &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, fin_body),
                    );
                }
                _ => {
                    let mut fin_body = serde_json::Map::new();
                    fin_body.insert("commandFinished".into(), serde_json::json!(true));
                    fin_body.insert("success".into(), serde_json::json!(false));
                    fin_body.insert("exitCode".into(), serde_json::json!(1));
                    fin_body.insert("id".into(), serde_json::json!(run_id));
                    let _ = daemon.send_packet_to_device(
                        &dev_id,
                        &NetworkPacket::new(PACKET_TYPE_RUNCOMMAND_OUTPUT, fin_body),
                    );
                }
            }
        });
    }
}
