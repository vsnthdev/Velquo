pub mod kdeconnect;
pub mod logger;

use crate::kdeconnect::daemon::{DeviceView, HostInfo, KdeConnectDaemon, TransferItem};
use crate::kdeconnect::packet::NetworkPacket;
use crate::logger::{AppLogger, LogEntry};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub host: HostInfo,
    pub devices: Vec<DeviceView>,
    pub transfers: Vec<TransferItem>,
    pub logs: Vec<LogEntry>,
}

#[tauri::command]
fn get_app_snapshot(daemon: State<'_, Arc<KdeConnectDaemon>>) -> AppSnapshot {
    AppSnapshot {
        host: daemon.get_host_info(),
        devices: daemon.get_devices_list(),
        transfers: daemon.get_transfers_list(),
        logs: AppLogger::get_entries(),
    }
}

#[tauri::command]
async fn broadcast_discovery(daemon: State<'_, Arc<KdeConnectDaemon>>) -> Result<(), String> {
    log::info!("[ui] Manual discovery broadcast triggered");
    daemon.broadcast_udp_discovery().await;
    Ok(())
}

#[tauri::command]
async fn probe_device_ip(
    ip: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!("[ui] Probing IP address: {}", ip);
    daemon.probe_ip(&ip).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn request_pairing(
    device_id: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!("[ui] Requesting pairing with device_id={}", device_id);
    daemon
        .request_pairing(&device_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn accept_pairing(
    device_id: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!("[ui] Accepting pairing with device_id={}", device_id);
    daemon.accept_pairing(&device_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn reject_pairing(
    device_id: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!("[ui] Rejecting/cancelling pairing with device_id={}", device_id);
    daemon
        .unpair_or_reject(&device_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn unpair_device(
    device_id: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!("[ui] Unpairing device_id={}", device_id);
    daemon
        .unpair_or_reject(&device_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn send_ping(
    device_id: String,
    message: Option<String>,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Sending ping to device_id={} message={:?}",
        device_id,
        message
    );
    let pkt = NetworkPacket::ping(message.as_deref());
    daemon
        .send_packet_to_device(&device_id, &pkt)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn ring_device(
    device_id: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Sending Find My Phone (ring) request to device_id={}",
        device_id
    );
    let pkt = NetworkPacket::find_my_phone();
    daemon
        .send_packet_to_device(&device_id, &pkt)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn send_media_action(
    device_id: String,
    player: String,
    action: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Sending MPRIS action '{}' to player '{}' on device_id={}",
        action,
        player,
        device_id
    );
    daemon
        .send_mpris_action(&device_id, &player, &action)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn seek_media(
    device_id: String,
    player: String,
    position_ms: i64,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Seeking player '{}' on device_id={} to {}ms",
        player,
        device_id,
        position_ms
    );
    daemon
        .send_mpris_seek(&device_id, &player, position_ms)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_file(
    device_id: String,
    file_path: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<String, String> {
    log::info!(
        "[ui] Request to send file '{}' to device_id={}",
        file_path,
        device_id
    );
    let path = PathBuf::from(&file_path);
    daemon
        .send_file_to_device(&device_id, &path)
        .await
        .map_err(|e| {
            log::error!("[ui] send_file failed: {:?}", e);
            e.to_string()
        })
}

#[tauri::command]
async fn send_files(
    device_id: String,
    file_paths: Vec<String>,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<Vec<String>, String> {
    log::info!(
        "[ui] Request to send batch of {} file(s) to device_id={}",
        file_paths.len(),
        device_id
    );
    let paths: Vec<PathBuf> = file_paths.into_iter().map(PathBuf::from).collect();
    daemon
        .send_files_to_device(&device_id, paths)
        .await
        .map_err(|e| {
            log::error!("[ui] send_files failed: {:?}", e);
            e.to_string()
        })
}

#[tauri::command]
fn send_url(
    device_id: String,
    url: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<String, String> {
    log::info!(
        "[ui] Request to share URL '{}' to device_id={}",
        url,
        device_id
    );
    daemon
        .send_url_to_device(&device_id, &url)
        .map_err(|e| {
            log::error!("[ui] send_url failed: {:?}", e);
            e.to_string()
        })
}

#[tauri::command]
fn set_device_clipboard_sync(
    device_id: String,
    enabled: bool,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Setting clipboard sync enabled={} for device_id={}",
        enabled,
        device_id
    );
    daemon
        .set_device_clipboard_sync(&device_id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_device_save_location(
    device_id: String,
    save_location: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Setting save_location='{}' for device_id={}",
        save_location,
        device_id
    );
    daemon
        .set_device_save_location(&device_id, &save_location)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_device_presenter(
    device_id: String,
    enabled: bool,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Setting allow_presenter={} for device_id={}",
        enabled,
        device_id
    );
    daemon
        .set_device_presenter(&device_id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_logs() {
    AppLogger::clear_entries();
    log::info!("[ui] Logs cleared");
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    log::info!("[ui] Quitting application");
    app.exit(0);
}

fn open_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_minimized = window.is_minimized().unwrap_or(false);

        // Once the window is already opened and not minimized, ignore further tray clicks
        if is_visible && !is_minimized {
            return;
        }

        #[cfg(target_os = "macos")]
        {
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
            let _ = app.show();
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        log::info!("[tray] Opened main window and showed icon in macOS Dock");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_file_path = AppLogger::init();
    log::info!("================================================================");
    log::info!(
        "Starting Velquo (macOS Apple Silicon aarch64) - KDE Connect Protocol v8"
    );
    log::info!("Log file path: {:?}", log_file_path);
    log::info!("================================================================");

    let config_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("dev.vsnth.velquo");
    let downloads_dir = dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Downloads")
    });

    let daemon = KdeConnectDaemon::new(config_dir, downloads_dir, log_file_path)
        .expect("Failed to initialize KdeConnectDaemon");

    let daemon_for_setup = Arc::clone(&daemon);

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(daemon)
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            broadcast_discovery,
            probe_device_ip,
            request_pairing,
            accept_pairing,
            reject_pairing,
            unpair_device,
            send_ping,
            ring_device,
            send_media_action,
            seek_media,
            send_file,
            send_files,
            send_url,
            set_device_clipboard_sync,
            set_device_save_location,
            set_device_presenter,
            clear_logs,
            quit_app,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            {
                // Window starts open, so show in macOS Dock (Regular activation policy)
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
            }

            AppLogger::set_app_handle(app.handle().clone());
            daemon_for_setup.set_app_handle(app.handle().clone());
            daemon_for_setup.start();

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                    let _ = apply_vibrancy(&window, NSVisualEffectMaterial::Sidebar, None, None);
                }
                let _ = window.set_resizable(false);
                let _ = window.set_maximizable(false);
                let _ = window.set_minimizable(true);
                let _ = window.set_closable(true);
                let _ = window.show();
                let _ = window.set_focus();
            }

            // Right-click menu only has 1 option: "Quit"
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&quit_item])?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
                .expect("Failed to load embedded tray.png");

            let _tray = TrayIconBuilder::with_id("main_tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Velquo")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| {
                    if event.id.as_ref() == "quit" {
                        log::info!("[tray] Quit clicked from menu bar context menu");
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        open_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            log::info!("[tray] macOS Menu Bar tray icon initialized");
            Ok(())
        })
        .on_window_event(|window, event| {
            // Clicking the red close button (✕) hides the window, removes the Dock icon, and keeps the menu bar icon active
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    log::info!(
                        "[window] Close button clicked — hiding window and Dock icon (daemon stays active in menu bar)"
                    );
                    let _ = window.hide();
                    api.prevent_close();
                    #[cfg(target_os = "macos")]
                    {
                        let _ = window
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Velquo Tauri application");
}
