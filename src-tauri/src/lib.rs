pub mod kdeconnect;
pub mod logger;

use crate::kdeconnect::daemon::{DeviceView, HostInfo, KdeConnectDaemon, TransferItem};
use crate::kdeconnect::packet::NetworkPacket;
use crate::logger::{AppLogger, LogEntry};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{IconMenuItem, Menu, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};

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
    daemon
        .ring_device(&device_id)
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
fn set_device_notifications_sync(
    device_id: String,
    enabled: bool,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Setting sync_notifications={} for device_id={}",
        enabled,
        device_id
    );
    daemon
        .set_device_notifications_sync(&device_id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_device_auto_copy_otp(
    device_id: String,
    enabled: bool,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Setting auto_copy_otp={} for device_id={}",
        enabled,
        device_id
    );
    daemon
        .set_device_auto_copy_otp(&device_id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn dismiss_notification(
    device_id: String,
    notification_id: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Dismissing notification_id='{}' on device_id={}",
        notification_id,
        device_id
    );
    daemon
        .dismiss_device_notification(&device_id, &notification_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_text_to_clipboard(
    text: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    daemon
        .copy_text_to_clipboard(&text)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_ignore_whatsapp_conversations(
    enabled: bool,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Setting ignore_whatsapp_conversations={}",
        enabled
    );
    daemon
        .set_ignore_whatsapp_conversations(enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_device_run_command(
    device_id: String,
    name: String,
    command: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Adding run command '{}' ('{}') for device_id={}",
        name,
        command,
        device_id
    );
    daemon
        .add_device_run_command(&device_id, &name, &command)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_device_run_command(
    device_id: String,
    key: String,
    daemon: State<'_, Arc<KdeConnectDaemon>>,
) -> Result<(), String> {
    log::info!(
        "[ui] Removing run command key='{}' for device_id={}",
        key,
        device_id
    );
    daemon
        .remove_device_run_command(&device_id, &key)
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
        #[cfg(target_os = "macos")]
        {
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
            let _ = app.show();
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        log::info!("[tray] Focused and brought main window to foreground");
    }
}

fn render_svg_icon(svg: &str, size: u32) -> Option<tauri::image::Image<'static>> {
    let opt = resvg::usvg::Options::default();
    let tree = match resvg::usvg::Tree::from_str(svg, &opt) {
        Ok(t) => t,
        Err(e) => {
            log::error!("[tray] Failed to parse SVG icon: {}", e);
            return None;
        }
    };
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let tree_size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        size as f32 / tree_size.width(),
        size as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Convert tiny-skia premultiplied RGBA pixels into straight RGBA for tauri::image::Image
    let mut rgba = pixmap.take();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a > 0 && a < 255 {
            let af = a as f32 / 255.0;
            px[0] = ((px[0] as f32 / af).round() as u32).min(255) as u8;
            px[1] = ((px[1] as f32 / af).round() as u32).min(255) as u8;
            px[2] = ((px[2] as f32 / af).round() as u32).min(255) as u8;
        }
    }

    Some(tauri::image::Image::new_owned(rgba, size, size))
}

fn is_macos_dark_mode() -> bool {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|out| {
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .eq_ignore_ascii_case("Dark")
        })
        .unwrap_or(false)
}

fn phosphor_duotone_menu_icon(
    tone_path_d: &str,
    main_path_d: &str,
    is_dark: bool,
) -> Option<tauri::image::Image<'static>> {
    // Render the Phosphor `duotone` SVG directly in Rust using `resvg` in either white (Dark Mode)
    // or black (Light Mode). On macOS, `mark_menu_icons_as_template()` also marks the
    // resulting `NSImage` as a template so AppKit automatically preserves the 20% alpha duotone
    // fill and 100% alpha stroke across light/dark/hovered menu rows.
    let fill = if is_dark { "#ffffff" } else { "#18181b" };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="36" height="36"><path fill="{fill}" fill-opacity="0.2" d="{tone_path_d}"/><path fill="{fill}" d="{main_path_d}"/></svg>"##,
    );
    render_svg_icon(&svg, 36)
}

#[cfg(target_os = "macos")]
struct TrayTargetIvarsMirror {
    _id: objc2::rc::Retained<objc2_foundation::NSString>,
    menu: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSMenu>>>,
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu_on_left_click: std::cell::Cell<bool>,
    _menu_on_right_click: std::cell::Cell<bool>,
}

#[cfg(target_os = "macos")]
fn mark_menu_icons_as_template(tray: &tauri::tray::TrayIcon<tauri::Wry>) {
    let _ = tray.with_inner_tray_icon(|inner| {
        if let Some(status_item) = inner.ns_status_item() {
            unsafe {
                let mtm = objc2_foundation::MainThreadMarker::new_unchecked();
                if let Some(button) = status_item.button(mtm) {
                    let subviews = button.subviews();
                    for i in 0..subviews.count() {
                        let subview = subviews.objectAtIndex(i);
                        let cls: *const objc2::runtime::AnyClass =
                            objc2::msg_send![&*subview, class];
                        if !cls.is_null() {
                            let ivar = objc2::ffi::class_getInstanceVariable(cls, c"ivars".as_ptr());
                            if !ivar.is_null() {
                                let offset = objc2::ffi::ivar_getOffset(ivar);
                                let ivars_ptr = (&*subview as *const _ as *const u8).offset(offset)
                                    as *const TrayTargetIvarsMirror;
                                if let Some(ns_menu) = (*ivars_ptr).menu.borrow().as_ref() {
                                    let count = ns_menu.numberOfItems();
                                    let sel_vis = objc2::sel!(setPreferredImageVisibility:);
                                    for idx in 0..count {
                                        if let Some(item) = ns_menu.itemAtIndex(idx) {
                                            if let Some(img) = item.image() {
                                                img.setTemplate(true);
                                                let responds: bool = objc2::msg_send![
                                                    &*item,
                                                    respondsToSelector: sel_vis
                                                ];
                                                if responds {
                                                    // NSMenuItemImageVisibilityVisible = 1 (macOS 27+)
                                                    let vis: isize = 1;
                                                    let _: () = objc2::msg_send![
                                                        &*item,
                                                        setPreferredImageVisibility: vis
                                                    ];
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

fn device_type_duotone_menu_icon(
    device_type: &str,
    is_dark: bool,
) -> Option<tauri::image::Image<'static>> {
    let normalized = device_type.to_ascii_lowercase();
    let (tone_d, main_d) = match normalized.as_str() {
        "phone" | "smartphone" => (
            // Phosphor `DeviceMobile` (duotone)
            "M192,56V200H64V56Z",
            "M176,16H80A24,24,0,0,0,56,40V216a24,24,0,0,0,24,24h96a24,24,0,0,0,24-24V40A24,24,0,0,0,176,16ZM72,64H184V192H72Zm8-32h96a8,8,0,0,1,8,8v8H72V40A8,8,0,0,1,80,32Zm96,192H80a8,8,0,0,1-8-8v-8H184v8A8,8,0,0,1,176,224Z",
        ),
        "laptop" => (
            // Phosphor `Laptop` (duotone)
            "M216,72V176H40V72A16,16,0,0,1,56,56H200A16,16,0,0,1,216,72Z",
            "M232,168h-8V72a24,24,0,0,0-24-24H56A24,24,0,0,0,32,72v96H24a8,8,0,0,0-8,8v16a24,24,0,0,0,24,24H216a24,24,0,0,0,24-24V176A8,8,0,0,0,232,168ZM48,72a8,8,0,0,1,8-8H200a8,8,0,0,1,8,8v96H48ZM224,192a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8v-8H224ZM152,88a8,8,0,0,1-8,8H112a8,8,0,0,1,0-16h32A8,8,0,0,1,152,88Z",
        ),
        "tablet" => (
            // Phosphor `DeviceTablet` (duotone)
            "M208,64V192H48V64Z",
            "M192,24H64A24,24,0,0,0,40,48V208a24,24,0,0,0,24,24H192a24,24,0,0,0,24-24V48A24,24,0,0,0,192,24ZM56,72H200V184H56Zm8-32H192a8,8,0,0,1,8,8v8H56V48A8,8,0,0,1,64,40ZM192,216H64a8,8,0,0,1-8-8v-8H200v8A8,8,0,0,1,192,216Z",
        ),
        "tv" => (
            // Phosphor `Television` (duotone)
            "M152,72V208H40a8,8,0,0,1-8-8V80a8,8,0,0,1,8-8Z",
            "M216,64H147.31l34.35-34.34a8,8,0,1,0-11.32-11.32L128,60.69,85.66,18.34A8,8,0,0,0,74.34,29.66L108.69,64H40A16,16,0,0,0,24,80V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V80A16,16,0,0,0,216,64ZM40,80H144V200H40ZM216,200H160V80h56V200Zm-16-84a12,12,0,1,1-12-12A12,12,0,0,1,200,116Zm0,48a12,12,0,1,1-12-12A12,12,0,0,1,200,164Z",
        ),
        _ => (
            // Phosphor `Desktop` (duotone)
            "M224,64v88H32V64A16,16,0,0,1,48,48H208A16,16,0,0,1,224,64Z",
            "M208,40H48A24,24,0,0,0,24,64V176a24,24,0,0,0,24,24h72v16H96a8,8,0,0,0,0,16h64a8,8,0,0,0,0-16H136V200h72a24,24,0,0,0,24-24V64A24,24,0,0,0,208,40ZM48,56H208a8,8,0,0,1,8,8v80H40V64A8,8,0,0,1,48,56ZM208,184H48a8,8,0,0,1-8-8V160H216v16A8,8,0,0,1,208,184Z",
        ),
    };
    phosphor_duotone_menu_icon(tone_d, main_d, is_dark)
}

pub fn build_tray_menu(
    app: &AppHandle,
    trusted_devices: &[(String, String, String)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let is_dark = is_macos_dark_mode();

    // Phosphor `Browser` (duotone)
    let open_icon = phosphor_duotone_menu_icon(
        "M224,56V96H32V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z",
        "M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,16V88H40V56Zm0,144H40V104H216v96Z",
        is_dark,
    );
    // Phosphor `PlusCircle` (duotone)
    let add_icon = phosphor_duotone_menu_icon(
        "M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z",
        "M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm48-88a8,8,0,0,1-8,8H136v32a8,8,0,0,1-16,0V136H88a8,8,0,0,1,0-16h32V88a8,8,0,0,1,16,0v32h32A8,8,0,0,1,176,128Z",
        is_dark,
    );
    // Phosphor `Door` (duotone)
    let quit_icon = phosphor_duotone_menu_icon(
        "M200,40V224H56V40a8,8,0,0,1,8-8H192A8,8,0,0,1,200,40Z",
        "M232,216H208V40a16,16,0,0,0-16-16H64A16,16,0,0,0,48,40V216H24a8,8,0,0,0,0,16H232a8,8,0,0,0,0-16ZM64,40H192V216H64Zm104,92a12,12,0,1,1-12-12A12,12,0,0,1,168,132Z",
        is_dark,
    );

    // 1. Open window (`Browser` duotone icon)
    let open_item = IconMenuItem::with_id(
        app,
        "open_window",
        "Open window",
        true,
        open_icon,
        None::<&str>,
    )?;
    menu.append(&open_item)?;

    // 2. Separator
    let sep1 = PredefinedMenuItem::separator(app)?;
    menu.append(&sep1)?;

    // 3. Dynamically add all trusted devices with their dynamic deviceType duotone icon
    if !trusted_devices.is_empty() {
        for (device_id, device_name, device_type) in trusted_devices {
            let dev_icon = device_type_duotone_menu_icon(device_type, is_dark);
            let item = IconMenuItem::with_id(
                app,
                format!("device:{}", device_id),
                device_name,
                true,
                dev_icon,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }

        // 4. Separator after trusted devices
        let sep2 = PredefinedMenuItem::separator(app)?;
        menu.append(&sep2)?;
    }

    // 5. Pair new device (`PlusCircle` duotone icon)
    let pair_item = IconMenuItem::with_id(
        app,
        "pair_new_device",
        "Pair new device",
        true,
        add_icon,
        None::<&str>,
    )?;
    menu.append(&pair_item)?;

    // 6. Quit (`Door` duotone icon)
    let quit_item = IconMenuItem::with_id(app, "quit", "Quit", true, quit_icon, None::<&str>)?;
    menu.append(&quit_item)?;

    Ok(menu)
}

pub fn refresh_tray_menu(app: &AppHandle, trusted_devices: Vec<(String, String, String)>) {
    let app_clone = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app_clone.tray_by_id("main_tray") {
            if let Ok(menu) = build_tray_menu(&app_clone, &trusted_devices) {
                let _ = tray.set_menu(Some(menu));
                #[cfg(target_os = "macos")]
                mark_menu_icons_as_template(&tray);
            }
        }
    });
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
            set_device_notifications_sync,
            set_device_auto_copy_otp,
            dismiss_notification,
            copy_text_to_clipboard,
            set_ignore_whatsapp_conversations,
            add_device_run_command,
            remove_device_run_command,
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

            let initial_trusted = daemon_for_setup.get_trusted_devices_menu_list();
            let tray_menu = build_tray_menu(app.handle(), &initial_trusted)?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
                .expect("Failed to load embedded tray.png");

            let tray = TrayIconBuilder::with_id("main_tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Velquo")
                .menu(&tray_menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| {
                    let id = event.id.as_ref();
                    if id == "quit" {
                        log::info!("[tray] Quit clicked from menu bar menu");
                        app.exit(0);
                    } else if id == "open_window" {
                        open_main_window(app);
                    } else if id == "pair_new_device" {
                        open_main_window(app);
                        let _ = app.emit(
                            "velquo://tray-navigate",
                            serde_json::json!({ "view": "pair-device" }),
                        );
                    } else if let Some(device_id) = id.strip_prefix("device:") {
                        open_main_window(app);
                        let _ = app.emit(
                            "velquo://tray-navigate",
                            serde_json::json!({
                                "view": "device",
                                "deviceId": device_id,
                            }),
                        );
                    }
                })
                .build(app)?;

            #[cfg(target_os = "macos")]
            mark_menu_icons_as_template(&tray);

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
            } else if let tauri::WindowEvent::ThemeChanged(_) = event {
                let app = window.app_handle();
                if let Some(daemon) = app.try_state::<Arc<KdeConnectDaemon>>() {
                    let trusted = daemon.get_trusted_devices_menu_list();
                    refresh_tray_menu(app, trusted);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Velquo Tauri application");
}
