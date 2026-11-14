use crate::kdeconnect::daemon::KdeConnectDaemon;
use crate::open_main_window;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSData, NSDictionary, NSError, NSObject, NSSet, NSString,
};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationDefaultActionIdentifier, UNNotificationPresentationOptions,
    UNNotificationRequest, UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const NOTIFICATION_CATEGORY: &str = "velquo-phone-notification";
const DISMISS_ACTION: &str = "dismiss";

struct NotificationContext {
    app: AppHandle,
    daemon: Arc<KdeConnectDaemon>,
}

static NOTIFY_CTX: OnceLock<NotificationContext> = OnceLock::new();
static ACTIVE_DEVICE_ID: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static NOTIFICATIONS_AVAILABLE: AtomicBool = AtomicBool::new(false);
static NOTIFICATIONS_AUTHORIZED: AtomicBool = AtomicBool::new(false);

fn is_running_from_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .map(|path| path.to_string_lossy().contains(".app/Contents/MacOS/"))
        .unwrap_or(false)
}

fn active_device_store() -> &'static Mutex<Option<String>> {
    ACTIVE_DEVICE_ID.get_or_init(|| Mutex::new(None))
}

/// Tracks the device control page currently visible in the app. macOS notifications
/// for that same device are suppressed while the app is foreground because the user
/// can already see them in the in-app notification list.
pub fn set_active_notification_device(device_id: Option<&str>) {
    if let Ok(mut guard) = active_device_store().lock() {
        *guard = device_id.map(str::to_string);
    }
}

fn user_info_string(
    content: &objc2_user_notifications::UNNotificationContent,
    key: &str,
) -> Option<String> {
    let info = content.userInfo();
    let key = NSString::from_str(key);
    info.objectForKey(&key)
        .and_then(|value| value.downcast::<NSString>().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
}

fn main_window_is_focused() -> bool {
    NOTIFY_CTX
        .get()
        .and_then(|ctx| ctx.app.get_webview_window("main"))
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
}

fn should_suppress_notification(content: &objc2_user_notifications::UNNotificationContent) -> bool {
    let Some(device_id) = user_info_string(content, "deviceId") else {
        return false;
    };
    if user_info_string(content, "notificationId").is_none() || !main_window_is_focused() {
        return false;
    }

    active_device_store()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .as_deref()
        == Some(device_id.as_str())
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VelquoNotificationDelegate"]
    #[thread_kind = MainThreadOnly]
    struct VelquoNotificationDelegate;

    unsafe impl NSObjectProtocol for VelquoNotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for VelquoNotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            let content = notification.request().content();
            let options = if should_suppress_notification(&content) {
                UNNotificationPresentationOptions::empty()
            } else {
                UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::Sound
                    | UNNotificationPresentationOptions::List
            };
            completion_handler.call((options,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let request = response.notification().request();
            let content = request.content();
            let device_id = user_info_string(&content, "deviceId");
            let notification_id = user_info_string(&content, "notificationId");
            let action_id = response.actionIdentifier().to_string();

            if action_id == DISMISS_ACTION {
                if let (Some(device_id), Some(notification_id), Some(ctx)) =
                    (device_id, notification_id, NOTIFY_CTX.get())
                {
                    let _ = ctx
                        .daemon
                        .dismiss_device_notification(&device_id, &notification_id);
                }
            } else if action_id == unsafe { UNNotificationDefaultActionIdentifier.to_string() } {
                if let Some(ctx) = NOTIFY_CTX.get() {
                    open_main_window(&ctx.app);
                    if let Some(device_id) = device_id {
                        let _ = ctx.app.emit(
                            "velquo://focus-notification",
                            serde_json::json!({
                                "deviceId": device_id,
                                "notificationId": notification_id,
                            }),
                        );
                    }
                }
            }

            let identifier = request.identifier();
            let identifiers = NSArray::from_slice(&[&*identifier]);
            center.removeDeliveredNotificationsWithIdentifiers(&identifiers);
            completion_handler.call(());
        }
    }
);

impl VelquoNotificationDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

fn load_velquo_app_nsimage() -> Option<Retained<NSImage>> {
    let icon_bytes = include_bytes!("../icons/icon.icns");
    let ns_data = NSData::with_bytes(icon_bytes);
    NSImage::initWithData(NSImage::alloc(), &ns_data).or_else(|| {
        let fallback_bytes = include_bytes!("../icons/128x128@2x.png");
        let fallback_data = NSData::with_bytes(fallback_bytes);
        NSImage::initWithData(NSImage::alloc(), &fallback_data)
    })
}

pub fn apply_macos_dock_icon_on_main_thread() {
    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        if let Some(app_img) = load_velquo_app_nsimage() {
            let ns_app = NSApplication::sharedApplication(mtm);
            ns_app.setApplicationIconImage(Some(&app_img));
        }
    }
}

pub fn refresh_macos_dock_icon(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        apply_macos_dock_icon_on_main_thread();
    });
    // Also re-apply shortly after in case macOS finishes its asynchronous
    // Accessory -> Regular Dock tile transform on the next runloop tick.
    let app_clone = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        let _ = app_clone.run_on_main_thread(|| {
            apply_macos_dock_icon_on_main_thread();
        });
    });
}

pub fn init_macos_notifications(app: AppHandle, daemon: Arc<KdeConnectDaemon>) {
    let _ = NOTIFY_CTX.set(NotificationContext {
        app: app.clone(),
        daemon,
    });

    if !is_running_from_app_bundle() {
        log::warn!(
            "[macos-notify] UserNotifications require a bundled .app. Notifications are disabled for this unbundled dev binary. Launch a bundled debug/release build to test notifications."
        );
        return;
    }
    NOTIFICATIONS_AVAILABLE.store(true, Ordering::Release);

    let _ = app.run_on_main_thread(move || unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let center = UNUserNotificationCenter::currentNotificationCenter();

        register_notification_categories(&center);

        let delegate = VelquoNotificationDelegate::new(mtm);
        let proto: Retained<ProtocolObject<dyn UNUserNotificationCenterDelegate>> =
            ProtocolObject::from_retained(delegate);
        center.setDelegate(Some(&proto));
        // The notification center keeps a weak delegate reference.
        std::mem::forget(proto);

        let authorization = RcBlock::new(|granted: Bool, error: *mut NSError| {
            NOTIFICATIONS_AUTHORIZED.store(granted.as_bool(), Ordering::Release);
            if granted.as_bool() {
                log::info!("[macos-notify] Notification authorization granted");
            } else if !error.is_null() {
                let error = &*error;
                log::warn!(
                    "[macos-notify] Notification authorization failed: {}",
                    error.localizedDescription()
                );
            } else {
                log::warn!("[macos-notify] Notification authorization denied");
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &authorization,
        );
    });
}

/// Registers the notification category that carries the "Dismiss" action button.
/// Called once at startup and again whenever the UI language changes, so the
/// action title follows the app language rather than staying English.
unsafe fn register_notification_categories(center: &UNUserNotificationCenter) {
    let dismiss_action = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str(DISMISS_ACTION),
        &NSString::from_str(&crate::i18n::tr_current("native.notification.dismiss")),
        UNNotificationActionOptions::empty(),
    );
    let actions = NSArray::from_slice(&[&*dismiss_action]);
    let intent_identifiers = NSArray::<NSString>::from_slice(&[]);
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(NOTIFICATION_CATEGORY),
        &actions,
        &intent_identifiers,
        UNNotificationCategoryOptions::empty(),
    );
    let categories = NSSet::from_slice(&[&*category]);
    center.setNotificationCategories(&categories);
}

/// Re-registers the notification category after a language change. macOS applies
/// the new action titles to already-delivered notifications as well.
pub fn refresh_notification_action_titles(app: &AppHandle) {
    if !NOTIFICATIONS_AVAILABLE.load(Ordering::Acquire) {
        return;
    }
    let _ = app.run_on_main_thread(|| unsafe {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        register_notification_categories(&center);
    });
}

pub fn show_macos_notification(
    title: &str,
    subtitle: Option<&str>,
    body: &str,
    device_id: Option<&str>,
    notification_id: Option<&str>,
    dismissable: bool,
) {
    if !NOTIFICATIONS_AVAILABLE.load(Ordering::Acquire)
        || !NOTIFICATIONS_AUTHORIZED.load(Ordering::Acquire)
    {
        return;
    }

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let content = UNMutableNotificationContent::new();

    content.setTitle(&NSString::from_str(title));
    if let Some(subtitle) = subtitle.filter(|value| !value.is_empty()) {
        content.setSubtitle(&NSString::from_str(subtitle));
    }
    content.setBody(&NSString::from_str(body));

    let sound = UNNotificationSound::soundNamed(&NSString::from_str("Glass"));
    content.setSound(Some(&sound));

    let raw_id = notification_id.map(str::to_string);
    let normalized_id = notification_id.map(KdeConnectDaemon::normalize_notif_id);

    if device_id.is_some() || raw_id.is_some() {
        let device_key = NSString::from_str("deviceId");
        let notification_key = NSString::from_str("notificationId");
        let device_value = NSString::from_str(device_id.unwrap_or(""));
        let notification_value = NSString::from_str(raw_id.as_deref().unwrap_or(""));
        let device_object: &AnyObject = &device_value;
        let notification_object: &AnyObject = &notification_value;
        let user_info = NSDictionary::from_slices(
            &[&*device_key, &*notification_key],
            &[device_object, notification_object],
        );
        // `setUserInfo` takes the erased `NSDictionary` type, while `from_slices`
        // returns a concretely-typed dictionary. The runtime object is identical.
        let user_info_untyped = unsafe {
            &*(&*user_info as *const NSDictionary<NSString, AnyObject> as *const NSDictionary)
        };
        unsafe {
            content.setUserInfo(user_info_untyped);
        }
    }

    if dismissable && raw_id.is_some() {
        content.setCategoryIdentifier(&NSString::from_str(NOTIFICATION_CATEGORY));
    }

    let identifier = match (device_id, normalized_id.as_deref()) {
        (Some(device_id), Some(normalized_id)) if !normalized_id.is_empty() => {
            format!("{}:{}", device_id, normalized_id)
        }
        _ => format!("velquo-{}", Uuid::new_v4()),
    };
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&identifier),
        &content,
        None,
    );

    let completion = RcBlock::new(|error: *mut NSError| {
        if !error.is_null() {
            let error = unsafe { &*error };
            log::warn!(
                "[macos-notify] Failed to deliver notification: {}",
                error.localizedDescription()
            );
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&completion));
}

pub fn remove_macos_notifications(device_id: &str, notification_ids: &[String]) {
    if !NOTIFICATIONS_AVAILABLE.load(Ordering::Acquire) {
        return;
    }

    let targets: Vec<String> = notification_ids
        .iter()
        .map(|notification_id| KdeConnectDaemon::normalize_notif_id(notification_id))
        .filter(|notification_id| !notification_id.is_empty())
        .collect();
    if targets.is_empty() {
        return;
    }

    let identifiers: Vec<Retained<NSString>> = targets
        .iter()
        .map(|target| NSString::from_str(&format!("{}:{}", device_id, target)))
        .collect();
    let identifier_refs: Vec<&NSString> =
        identifiers.iter().map(|identifier| &**identifier).collect();
    let identifier_array = NSArray::from_slice(&identifier_refs);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.removeDeliveredNotificationsWithIdentifiers(&identifier_array);
    center.removePendingNotificationRequestsWithIdentifiers(&identifier_array);
}
