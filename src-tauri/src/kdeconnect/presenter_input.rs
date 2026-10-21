use crate::kdeconnect::packet::NetworkPacket;
use std::ffi::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> *mut c_void;
    fn CGEventSetType(event: *mut c_void, event_type: u32);
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventCreateScrollWheelEvent(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
    ) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventKeyboardSetUnicodeString(
        event: *mut c_void,
        string_length: usize,
        unicode_string: *const u16,
    );
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGWarpMouseCursorPosition(new_cursor_position: CGPoint) -> i32;
    fn CFRelease(cf: *const c_void);
}

// CoreGraphics event constants
#[cfg(target_os = "macos")]
const K_CG_HID_EVENT_TAP: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_SESSION_EVENT_TAP: u32 = 1;

#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
#[cfg(target_os = "macos")]
const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_UP: u32 = 26;

#[cfg(target_os = "macos")]
const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_BUTTON_CENTER: u32 = 2;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_EVENT_CLICK_STATE: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_SCROLL_EVENT_UNIT_PIXEL: u32 = 0;

// macOS Carbon virtual keycodes matching `kdeconnect-kde/plugins/mousepad/macosremoteinput.mm`
#[cfg(target_os = "macos")]
const K_VK_COMMAND: u16 = 0x37;
#[cfg(target_os = "macos")]
const K_VK_SHIFT: u16 = 0x38;
#[cfg(target_os = "macos")]
const K_VK_OPTION: u16 = 0x3A;
#[cfg(target_os = "macos")]
const K_VK_CONTROL: u16 = 0x3B;

#[cfg(target_os = "macos")]
const SPECIAL_KEYS_MAP: [Option<u16>; 33] = [
    None,        // 0: Invalid
    Some(0x33),  // 1: kVK_Delete (Backspace)
    Some(0x30),  // 2: kVK_Tab
    Some(0x24),  // 3: kVK_Return
    Some(0x7B),  // 4: kVK_LeftArrow
    Some(0x7E),  // 5: kVK_UpArrow
    Some(0x7C),  // 6: kVK_RightArrow
    Some(0x7D),  // 7: kVK_DownArrow
    Some(0x74),  // 8: kVK_PageUp (Previous Slide)
    Some(0x79),  // 9: kVK_PageDown (Next Slide)
    Some(0x73),  // 10: kVK_Home
    Some(0x77),  // 11: kVK_End
    Some(0x24),  // 12: kVK_Return
    Some(0x75),  // 13: kVK_ForwardDelete
    Some(0x35),  // 14: kVK_Escape (Exit Presentation)
    None,        // 15: SysReq
    None,        // 16: ScrollLock
    Some(0x3B),  // 17: kVK_Control
    Some(0x3A),  // 18: kVK_Option
    Some(0x38),  // 19: kVK_Shift
    Some(0x37),  // 20: kVK_Command
    Some(0x7A),  // 21: kVK_F1
    Some(0x78),  // 22: kVK_F2
    Some(0x63),  // 23: kVK_F3
    Some(0x76),  // 24: kVK_F4
    Some(0x60),  // 25: kVK_F5 (Start Presentation)
    Some(0x61),  // 26: kVK_F6
    Some(0x62),  // 27: kVK_F7
    Some(0x64),  // 28: kVK_F8
    Some(0x65),  // 29: kVK_F9
    Some(0x6D),  // 30: kVK_F10
    Some(0x67),  // 31: kVK_F11
    Some(0x6F),  // 32: kVK_F12
];

pub struct PresenterInputState {
    pub x_norm: f64,
    pub y_norm: f64,
    pub last_presenter_move: Option<Instant>,
    pub left_button_held: bool,
}

impl Default for PresenterInputState {
    fn default() -> Self {
        Self {
            x_norm: 0.5,
            y_norm: 0.5,
            last_presenter_move: None,
            left_button_held: false,
        }
    }
}

static STATE: Mutex<PresenterInputState> = Mutex::new(PresenterInputState {
    x_norm: 0.5,
    y_norm: 0.5,
    last_presenter_move: None,
    left_button_held: false,
});

/// Handles incoming `kdeconnect.presenter` gyroscope pointer packets (`dx`, `dy`, `stop`).
pub fn handle_presenter_packet(packet: &NetworkPacket) {
    if packet.get_bool("stop").unwrap_or(false) {
        if let Ok(mut state) = STATE.lock() {
            state.last_presenter_move = None;
        }
        return;
    }

    let dx = packet
        .body
        .get("dx")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let dy = packet
        .body
        .get("dy")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    #[cfg(target_os = "macos")]
    unsafe {
        let display_id = CGMainDisplayID();
        let screen_w = (CGDisplayPixelsWide(display_id) as f64).max(1.0);
        let screen_h = (CGDisplayPixelsHigh(display_id) as f64).max(1.0);
        let ratio = screen_w / screen_h;

        let (target_x, target_y) = {
            let mut state = STATE.lock().unwrap();
            let now = Instant::now();
            // Reset to screen center if starting a new pointer gesture (matches presenterplugin.cpp 500ms timeout)
            if state
                .last_presenter_move
                .map(|t| now.duration_since(t) > Duration::from_millis(500))
                .unwrap_or(true)
            {
                state.x_norm = 0.5;
                state.y_norm = 0.5;
            }
            state.last_presenter_move = Some(now);
            state.x_norm = (state.x_norm + dx).clamp(0.0, 1.0);
            state.y_norm = (state.y_norm + dy * ratio).clamp(0.0, 1.0);
            (state.x_norm * screen_w, state.y_norm * screen_h)
        };

        let pt = CGPoint {
            x: target_x,
            y: target_y,
        };
        let _ = CGWarpMouseCursorPosition(pt);
        let ev = CGEventCreateMouseEvent(
            std::ptr::null(),
            K_CG_EVENT_MOUSE_MOVED,
            pt,
            K_CG_MOUSE_BUTTON_LEFT,
        );
        if !ev.is_null() {
            CGEventPost(K_CG_HID_EVENT_TAP, ev);
            CFRelease(ev);
        }
    }
}

/// Handles incoming `kdeconnect.mousepad.request` packets (Presentation slide buttons + remote trackpad/keyboard).
pub fn handle_mousepad_packet(packet: &NetworkPacket) {
    #[cfg(target_os = "macos")]
    unsafe {
        let dx = packet
            .body
            .get("dx")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let dy = packet
            .body
            .get("dy")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let is_single_click = packet.get_bool("singleclick").unwrap_or(false);
        let is_double_click = packet.get_bool("doubleclick").unwrap_or(false);
        let is_middle_click = packet.get_bool("middleclick").unwrap_or(false);
        let is_right_click = packet.get_bool("rightclick").unwrap_or(false);
        let is_single_hold = packet.get_bool("singlehold").unwrap_or(false);
        let is_single_release = packet.get_bool("singlerelease").unwrap_or(false);
        let is_scroll = packet.get_bool("scroll").unwrap_or(false);
        let key_str = packet.get_str("key").unwrap_or("");
        let special_key = packet.get_i64("specialKey").unwrap_or(0) as usize;
        let valid_special_key =
            special_key > 0 && special_key < SPECIAL_KEYS_MAP.len() && SPECIAL_KEYS_MAP[special_key].is_some();

        let cur_pt = get_current_cursor_pos();

        if is_single_click
            || is_double_click
            || is_middle_click
            || is_right_click
            || is_single_hold
            || is_single_release
            || is_scroll
            || !key_str.is_empty()
            || valid_special_key
        {
            if is_single_click {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_DOWN,
                    cur_pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CGEventSetType(ev, K_CG_EVENT_LEFT_MOUSE_UP);
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
                if let Ok(mut s) = STATE.lock() {
                    s.left_button_held = false;
                }
            } else if is_double_click {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_DOWN,
                    cur_pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CGEventSetType(ev, K_CG_EVENT_LEFT_MOUSE_UP);
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CGEventSetIntegerValueField(ev, K_CG_MOUSE_EVENT_CLICK_STATE, 2);
                    CGEventSetType(ev, K_CG_EVENT_LEFT_MOUSE_DOWN);
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CGEventSetType(ev, K_CG_EVENT_LEFT_MOUSE_UP);
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
                if let Ok(mut s) = STATE.lock() {
                    s.left_button_held = false;
                }
            } else if is_middle_click {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_OTHER_MOUSE_DOWN,
                    cur_pt,
                    K_CG_MOUSE_BUTTON_CENTER,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CGEventSetType(ev, K_CG_EVENT_OTHER_MOUSE_UP);
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            } else if is_right_click {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_RIGHT_MOUSE_DOWN,
                    cur_pt,
                    K_CG_MOUSE_BUTTON_RIGHT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CGEventSetType(ev, K_CG_EVENT_RIGHT_MOUSE_UP);
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            } else if is_single_hold {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_DOWN,
                    cur_pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
                if let Ok(mut s) = STATE.lock() {
                    s.left_button_held = true;
                }
            } else if is_single_release {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_UP,
                    cur_pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
                if let Ok(mut s) = STATE.lock() {
                    s.left_button_held = false;
                }
            } else if is_scroll {
                let ev = CGEventCreateScrollWheelEvent(
                    std::ptr::null(),
                    K_CG_SCROLL_EVENT_UNIT_PIXEL,
                    2,
                    dy as i32,
                    -(dx as i32),
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            } else if !key_str.is_empty() || valid_special_key {
                let ctrl = packet.get_bool("ctrl").unwrap_or(false);
                let alt = packet.get_bool("alt").unwrap_or(false);
                let shift = packet.get_bool("shift").unwrap_or(false);
                let super_key = packet.get_bool("super").unwrap_or(false);

                if ctrl {
                    post_key_event(K_VK_CONTROL, true);
                }
                if alt {
                    post_key_event(K_VK_OPTION, true);
                }
                if shift {
                    post_key_event(K_VK_SHIFT, true);
                }
                if super_key {
                    post_key_event(K_VK_COMMAND, true);
                }

                if valid_special_key {
                    if let Some(vk) = SPECIAL_KEYS_MAP[special_key] {
                        log::info!(
                            "[presenter-input] Posting macOS specialKey={} (vk=0x{:02X})",
                            special_key,
                            vk
                        );
                        post_key_event(vk, true);
                        post_key_event(vk, false);
                    }
                } else {
                    let utf16: Vec<u16> = key_str.encode_utf16().collect();
                    for unit in utf16 {
                        let ev = CGEventCreateKeyboardEvent(std::ptr::null(), 0, true);
                        if !ev.is_null() {
                            CGEventKeyboardSetUnicodeString(ev, 1, &unit);
                            CGEventPost(K_CG_SESSION_EVENT_TAP, ev);
                            CFRelease(ev);
                        }
                    }
                }

                if super_key {
                    post_key_event(K_VK_COMMAND, false);
                }
                if shift {
                    post_key_event(K_VK_SHIFT, false);
                }
                if alt {
                    post_key_event(K_VK_OPTION, false);
                }
                if ctrl {
                    post_key_event(K_VK_CONTROL, false);
                }
            }
        } else if dx != 0.0 || dy != 0.0 {
            let display_id = CGMainDisplayID();
            let max_w = (CGDisplayPixelsWide(display_id) as f64 - 1.0).max(1.0);
            let max_h = (CGDisplayPixelsHigh(display_id) as f64 - 1.0).max(1.0);

            let new_pt = CGPoint {
                x: (cur_pt.x + dx).clamp(0.0, max_w),
                y: (cur_pt.y + dy).clamp(0.0, max_h),
            };

            let is_dragging = STATE.lock().map(|s| s.left_button_held).unwrap_or(false);
            if is_dragging {
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_DRAGGED,
                    new_pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            } else {
                let _ = CGWarpMouseCursorPosition(new_pt);
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_MOUSE_MOVED,
                    new_pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn get_current_cursor_pos() -> CGPoint {
    let ev = CGEventCreate(std::ptr::null());
    if ev.is_null() {
        return CGPoint { x: 0.0, y: 0.0 };
    }
    let pt = CGEventGetLocation(ev);
    CFRelease(ev);
    pt
}

#[cfg(target_os = "macos")]
unsafe fn post_key_event(vk: u16, down: bool) {
    let ev = CGEventCreateKeyboardEvent(std::ptr::null(), vk, down);
    if !ev.is_null() {
        CGEventPost(K_CG_HID_EVENT_TAP, ev);
        CFRelease(ev);
    }
}
