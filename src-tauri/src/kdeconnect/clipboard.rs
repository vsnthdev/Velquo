use std::ffi::{c_void, CStr};
use std::io::Write;
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    fn objc_getClass(name: *const libc::c_char) -> *mut c_void;
    fn sel_registerName(name: *const libc::c_char) -> *const c_void;
    fn objc_msgSend();
}

/// Returns `[NSPasteboard generalPasteboard].changeCount` in `< 1µs` via Objective-C FFI.
/// Only increments when the user (or an application) modifies the macOS system clipboard.
#[cfg(target_os = "macos")]
pub fn get_macos_pasteboard_change_count() -> isize {
    unsafe {
        let cls = objc_getClass(c"NSPasteboard".as_ptr());
        if cls.is_null() {
            return 0;
        }
        let gen_sel = sel_registerName(c"generalPasteboard".as_ptr());
        let cnt_sel = sel_registerName(c"changeCount".as_ptr());

        type MsgSendPtr = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
        type MsgSendInt = unsafe extern "C" fn(*mut c_void, *const c_void) -> isize;

        let get_pb: MsgSendPtr = std::mem::transmute(objc_msgSend as *const c_void);
        let get_cnt: MsgSendInt = std::mem::transmute(objc_msgSend as *const c_void);

        let pb = get_pb(cls, gen_sel);
        if pb.is_null() {
            return 0;
        }
        get_cnt(pb, cnt_sel)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_macos_pasteboard_change_count() -> isize {
    0
}

/// Checks if the current macOS pasteboard item is marked as concealed/secret
/// (`org.nspasteboard.ConcealedType` used by 1Password, Bitwarden, Apple Passwords, etc.).
#[cfg(target_os = "macos")]
pub fn is_macos_pasteboard_concealed() -> bool {
    unsafe {
        let pb_cls = objc_getClass(c"NSPasteboard".as_ptr());
        if pb_cls.is_null() {
            return false;
        }
        let gen_sel = sel_registerName(c"generalPasteboard".as_ptr());
        let types_sel = sel_registerName(c"types".as_ptr());
        let count_sel = sel_registerName(c"count".as_ptr());
        let obj_at_sel = sel_registerName(c"objectAtIndex:".as_ptr());
        let utf8_sel = sel_registerName(c"UTF8String".as_ptr());

        type MsgSendPtr = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
        type MsgSendCount = unsafe extern "C" fn(*mut c_void, *const c_void) -> usize;
        type MsgSendIdx = unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void;
        type MsgSendUtf8 = unsafe extern "C" fn(*mut c_void, *const c_void) -> *const libc::c_char;

        let get_pb: MsgSendPtr = std::mem::transmute(objc_msgSend as *const c_void);
        let get_types: MsgSendPtr = std::mem::transmute(objc_msgSend as *const c_void);
        let get_count: MsgSendCount = std::mem::transmute(objc_msgSend as *const c_void);
        let get_obj: MsgSendIdx = std::mem::transmute(objc_msgSend as *const c_void);
        let get_utf8: MsgSendUtf8 = std::mem::transmute(objc_msgSend as *const c_void);

        let pb = get_pb(pb_cls, gen_sel);
        if pb.is_null() {
            return false;
        }
        let types_arr = get_types(pb, types_sel);
        if types_arr.is_null() {
            return false;
        }
        let n = get_count(types_arr, count_sel);
        for i in 0..n {
            let ns_str = get_obj(types_arr, obj_at_sel, i);
            if !ns_str.is_null() {
                let c_str = get_utf8(ns_str, utf8_sel);
                if !c_str.is_null() {
                    if let Ok(s) = CStr::from_ptr(c_str).to_str() {
                        if s.eq_ignore_ascii_case("org.nspasteboard.ConcealedType")
                            || s.eq_ignore_ascii_case("com.agilebits.onepassword")
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(not(target_os = "macos"))]
pub fn is_macos_pasteboard_concealed() -> bool {
    false
}

/// Reads the current plain-text string from the macOS clipboard via `/usr/bin/pbpaste`.
/// Returns `None` if the clipboard is empty, non-text, or marked as a concealed password.
pub fn read_macos_clipboard_text() -> Option<String> {
    if is_macos_pasteboard_concealed() {
        log::debug!("[clipboard] Skipping concealed/password clipboard item");
        return None;
    }

    let output = Command::new("/usr/bin/pbpaste")
        .env("LANG", "en_US.UTF-8")
        .env("LC_CTYPE", "UTF-8")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    if text.is_empty() {
        return None;
    }
    Some(text)
}

/// Writes `text` to the macOS clipboard via `/usr/bin/pbcopy` and returns the new `changeCount`.
pub fn write_macos_clipboard_text(text: &str) -> Option<isize> {
    let mut child = Command::new("/usr/bin/pbcopy")
        .env("LANG", "en_US.UTF-8")
        .env("LC_CTYPE", "UTF-8")
        .stdin(Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).ok()?;
    }

    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }

    Some(get_macos_pasteboard_change_count())
}
