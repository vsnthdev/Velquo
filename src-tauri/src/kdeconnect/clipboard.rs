use objc2_foundation::NSString;
use std::ffi::{c_void, CStr};
use std::sync::Mutex;

static PASTEBOARD_LOCK: Mutex<()> = Mutex::new(());

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
#[link(name = "objc", kind = "dylib")]
extern "C" {
    fn objc_getClass(name: *const libc::c_char) -> *mut c_void;
    fn sel_registerName(name: *const libc::c_char) -> *const c_void;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

/// Returns `[NSPasteboard generalPasteboard].changeCount` in `< 1µs` via Objective-C FFI.
/// Serialized via `PASTEBOARD_LOCK` because `NSPasteboard.generalPasteboard` is not thread-safe.
#[cfg(target_os = "macos")]
pub fn get_macos_pasteboard_change_count() -> isize {
    let _guard = PASTEBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let res = (|| {
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
        })();
        objc_autoreleasePoolPop(pool);
        res
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_macos_pasteboard_change_count() -> isize {
    0
}

#[cfg(target_os = "macos")]
unsafe fn is_pb_concealed_unlocked(pb: *mut c_void) -> bool {
    let types_sel = sel_registerName(c"types".as_ptr());
    let count_sel = sel_registerName(c"count".as_ptr());
    let obj_at_sel = sel_registerName(c"objectAtIndex:".as_ptr());
    let utf8_sel = sel_registerName(c"UTF8String".as_ptr());

    type MsgSendPtr = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
    type MsgSendCount = unsafe extern "C" fn(*mut c_void, *const c_void) -> usize;
    type MsgSendIdx = unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void;
    type MsgSendUtf8 = unsafe extern "C" fn(*mut c_void, *const c_void) -> *const libc::c_char;

    let get_types: MsgSendPtr = std::mem::transmute(objc_msgSend as *const c_void);
    let get_count: MsgSendCount = std::mem::transmute(objc_msgSend as *const c_void);
    let get_obj: MsgSendIdx = std::mem::transmute(objc_msgSend as *const c_void);
    let get_utf8: MsgSendUtf8 = std::mem::transmute(objc_msgSend as *const c_void);

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

/// Reads the current plain-text UTF-8 string directly from `[NSPasteboard generalPasteboard]` in `< 5µs`.
/// Returns `None` if the clipboard is empty, non-text, or marked as a concealed password.
#[cfg(target_os = "macos")]
pub fn read_macos_clipboard_text() -> Option<String> {
    let _guard = PASTEBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let res = (|| {
            let pb_cls = objc_getClass(c"NSPasteboard".as_ptr());
            if pb_cls.is_null() {
                return None;
            }
            let gen_sel = sel_registerName(c"generalPasteboard".as_ptr());
            type MsgSendPtr = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
            let get_pb: MsgSendPtr = std::mem::transmute(objc_msgSend as *const c_void);

            let pb = get_pb(pb_cls, gen_sel);
            if pb.is_null() {
                return None;
            }

            if is_pb_concealed_unlocked(pb) {
                log::debug!("[clipboard] Skipping concealed/password clipboard item");
                return None;
            }

            let str_for_type_sel = sel_registerName(c"stringForType:".as_ptr());
            let utf8_sel = sel_registerName(c"UTF8String".as_ptr());

            type MsgSendStrForType =
                unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> *mut c_void;
            type MsgSendUtf8 =
                unsafe extern "C" fn(*mut c_void, *const c_void) -> *const libc::c_char;

            let get_str_for_type: MsgSendStrForType =
                std::mem::transmute(objc_msgSend as *const c_void);
            let get_utf8: MsgSendUtf8 = std::mem::transmute(objc_msgSend as *const c_void);

            let pb_type = NSString::from_str("public.utf8-plain-text");
            let pb_type_ptr: *const NSString = &*pb_type;
            let ns_val = get_str_for_type(pb, str_for_type_sel, pb_type_ptr.cast());
            if ns_val.is_null() {
                return None;
            }

            let c_str = get_utf8(ns_val, utf8_sel);
            if c_str.is_null() {
                return None;
            }

            let text = CStr::from_ptr(c_str).to_string_lossy().into_owned();
            if text.is_empty() {
                return None;
            }
            Some(text)
        })();
        objc_autoreleasePoolPop(pool);
        res
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_macos_clipboard_text() -> Option<String> {
    None
}

/// Writes `text` directly to `[NSPasteboard generalPasteboard]` in `< 5µs` and returns the new `changeCount`.
#[cfg(target_os = "macos")]
pub fn write_macos_clipboard_text(text: &str) -> Option<isize> {
    let _guard = PASTEBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let res = (|| {
            let pb_cls = objc_getClass(c"NSPasteboard".as_ptr());
            if pb_cls.is_null() {
                return None;
            }
            let gen_sel = sel_registerName(c"generalPasteboard".as_ptr());
            let clear_sel = sel_registerName(c"clearContents".as_ptr());
            let set_str_sel = sel_registerName(c"setString:forType:".as_ptr());
            let cnt_sel = sel_registerName(c"changeCount".as_ptr());

            type MsgSendPtr = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
            type MsgSendInt = unsafe extern "C" fn(*mut c_void, *const c_void) -> isize;
            type MsgSendSetStr = unsafe extern "C" fn(
                *mut c_void,
                *const c_void,
                *const c_void,
                *const c_void,
            ) -> bool;

            let get_pb: MsgSendPtr = std::mem::transmute(objc_msgSend as *const c_void);
            let clear_pb: MsgSendInt = std::mem::transmute(objc_msgSend as *const c_void);
            let set_str: MsgSendSetStr = std::mem::transmute(objc_msgSend as *const c_void);
            let get_cnt: MsgSendInt = std::mem::transmute(objc_msgSend as *const c_void);

            let pb = get_pb(pb_cls, gen_sel);
            if pb.is_null() {
                return None;
            }

            let _ = clear_pb(pb, clear_sel);

            let ns_text = NSString::from_str(text);
            let ns_type = NSString::from_str("public.utf8-plain-text");
            let ns_text_ptr: *const NSString = &*ns_text;
            let ns_type_ptr: *const NSString = &*ns_type;

            let ok = set_str(pb, set_str_sel, ns_text_ptr.cast(), ns_type_ptr.cast());
            if !ok {
                return None;
            }

            Some(get_cnt(pb, cnt_sel))
        })();
        objc_autoreleasePoolPop(pool);
        res
    }
}

#[cfg(not(target_os = "macos"))]
pub fn write_macos_clipboard_text(_text: &str) -> Option<isize> {
    None
}
