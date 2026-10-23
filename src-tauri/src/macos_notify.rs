#![allow(deprecated)]

use crate::kdeconnect::daemon::KdeConnectDaemon;
use crate::open_main_window;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, AllocAnyThread, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::{
    MainThreadMarker, NSData, NSDictionary, NSObject, NSString, NSUserNotification,
    NSUserNotificationActivationType, NSUserNotificationCenter, NSUserNotificationCenterDelegate,
};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter};

struct NotificationContext {
    app: AppHandle,
    daemon: Arc<KdeConnectDaemon>,
}

static NOTIFY_CTX: OnceLock<NotificationContext> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VelquoNotificationDelegate"]
    #[thread_kind = MainThreadOnly]
    struct VelquoNotificationDelegate;

    unsafe impl NSObjectProtocol for VelquoNotificationDelegate {}

    unsafe impl NSUserNotificationCenterDelegate for VelquoNotificationDelegate {
        #[unsafe(method(userNotificationCenter:shouldPresentNotification:))]
        fn should_present(
            &self,
            _center: &NSUserNotificationCenter,
            _notification: &NSUserNotification,
        ) -> bool {
            true
        }

        #[unsafe(method(userNotificationCenter:didActivateNotification:))]
        fn did_activate(
            &self,
            center: &NSUserNotificationCenter,
            notification: &NSUserNotification,
        ) {
            let activation_type = notification.activationType();
            let mut device_id_opt: Option<String> = None;
            let mut notif_id_opt: Option<String> = None;

            if let Some(user_info) = notification.userInfo() {
                let dev_key = NSString::from_str("deviceId");
                let notif_key = NSString::from_str("notificationId");

                if let Some(val) = user_info.objectForKey(&dev_key) {
                    if let Ok(ns_str) = val.downcast::<NSString>() {
                        let s = ns_str.to_string();
                        if !s.is_empty() {
                            device_id_opt = Some(s);
                        }
                    }
                }
                if let Some(val) = user_info.objectForKey(&notif_key) {
                    if let Ok(ns_str) = val.downcast::<NSString>() {
                        let s = ns_str.to_string();
                        if !s.is_empty() {
                            notif_id_opt = Some(s);
                        }
                    }
                }
            }

            // Remove the activated notification from macOS Notification Center
            center.removeDeliveredNotification(notification);

            let Some(ctx) = NOTIFY_CTX.get() else {
                return;
            };

            let additional_action_id = notification
                .additionalActivationAction()
                .and_then(|a| a.identifier())
                .map(|s| s.to_string());

            let is_dismiss_clicked = activation_type
                == NSUserNotificationActivationType::ActionButtonClicked
                || (activation_type
                    == NSUserNotificationActivationType::AdditionalActionClicked
                    && additional_action_id.as_deref() == Some("dismiss"));

            if is_dismiss_clicked {
                if let (Some(device_id), Some(notif_id)) = (device_id_opt, notif_id_opt) {
                    log::info!(
                        "[macos-notify] Dismiss action clicked in macOS notification for device='{}' id='{}'",
                        device_id,
                        notif_id
                    );
                    let _ = ctx
                        .daemon
                        .dismiss_device_notification(&device_id, &notif_id);
                }
            } else if activation_type == NSUserNotificationActivationType::ContentsClicked {
                log::info!(
                    "[macos-notify] Notification body clicked (device={:?}, notification={:?})",
                    device_id_opt,
                    notif_id_opt
                );
                open_main_window(&ctx.app);
                if let Some(device_id) = device_id_opt {
                    let _ = ctx.app.emit(
                        "velquo://focus-notification",
                        serde_json::json!({
                            "deviceId": device_id,
                            "notificationId": notif_id_opt,
                        }),
                    );
                }
            }
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
    let icon_bytes = include_bytes!("../icons/128x128@2x.png");
    let ns_data = NSData::with_bytes(icon_bytes);
    NSImage::initWithData(NSImage::alloc(), &ns_data)
}

pub fn init_macos_notifications(app: AppHandle, daemon: Arc<KdeConnectDaemon>) {
    let _ = NOTIFY_CTX.set(NotificationContext {
        app: app.clone(),
        daemon,
    });

    let _ = app.run_on_main_thread(move || unsafe {
        let mtm = MainThreadMarker::new_unchecked();

        // Ensure NSApplication has the Velquo icon set even when running as an unbundled binary in `tauri dev`
        if let Some(app_img) = load_velquo_app_nsimage() {
            let ns_app = NSApplication::sharedApplication(mtm);
            ns_app.setApplicationIconImage(Some(&app_img));
        }

        let delegate = VelquoNotificationDelegate::new(mtm);
        let center = NSUserNotificationCenter::defaultUserNotificationCenter();
        let proto = ProtocolObject::from_retained(delegate);
        center.setDelegate(Some(&proto));
        // Keep the delegate alive for the lifetime of the application
        std::mem::forget(proto);
        log::info!("[macos-notify] Initialized native NSUserNotificationCenter delegate");
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
    let Some(ctx) = NOTIFY_CTX.get() else {
        return;
    };

    let title_owned = title.to_string();
    let subtitle_owned = subtitle.map(|s| s.to_string());
    let body_owned = body.to_string();
    let device_id_owned = device_id.map(|s| s.to_string());
    let notif_id_owned = notification_id.map(|s| s.to_string());

    let _ = ctx.app.run_on_main_thread(move || unsafe {
        let notification = NSUserNotification::new();

        let ns_title = NSString::from_str(&title_owned);
        notification.setTitle(Some(&ns_title));

        if let Some(sub) = &subtitle_owned {
            if !sub.is_empty() {
                let ns_sub = NSString::from_str(sub);
                notification.setSubtitle(Some(&ns_sub));
            }
        }

        let ns_body = NSString::from_str(&body_owned);
        notification.setInformativeText(Some(&ns_body));

        let sound = NSString::from_str("Glass");
        notification.setSoundName(Some(&sound));

        if let (Some(dev_id), Some(n_id)) = (&device_id_owned, &notif_id_owned) {
            let ident = NSString::from_str(&format!("{}:{}", dev_id, n_id));
            notification.setIdentifier(Some(&ident));
        }

        // Attach deviceId and notificationId in userInfo so click & Dismiss handlers know which notification was activated
        if device_id_owned.is_some() || notif_id_owned.is_some() {
            let k1 = NSString::from_str("deviceId");
            let k2 = NSString::from_str("notificationId");
            let v1 = NSString::from_str(device_id_owned.as_deref().unwrap_or(""));
            let v2 = NSString::from_str(notif_id_owned.as_deref().unwrap_or(""));

            let v1_obj: &AnyObject = &*v1;
            let v2_obj: &AnyObject = &*v2;

            let dict = NSDictionary::from_slices(&[&*k1, &*k2], &[v1_obj, v2_obj]);
            notification.setUserInfo(Some(&dict));
        }

        if dismissable && device_id_owned.is_some() && notif_id_owned.is_some() {
            notification.setHasActionButton(true);
            let dismiss_label = NSString::from_str("Dismiss");
            notification.setActionButtonTitle(&dismiss_label);

            let sel_shows = sel!(_setShowsButtons:);
            let responds_shows: bool = msg_send![&*notification, respondsToSelector: sel_shows];
            if responds_shows {
                let _: () = msg_send![&*notification, _setShowsButtons: true];
            }
        } else {
            notification.setHasActionButton(false);
        }

        // Set the Velquo identity icon directly on the notification so it always renders with the Velquo icon
        if let Some(app_img) = load_velquo_app_nsimage() {
            let sel_identity = sel!(_setIdentityImage:);
            let responds_identity: bool =
                msg_send![&*notification, respondsToSelector: sel_identity];
            if responds_identity {
                let _: () = msg_send![&*notification, _setIdentityImage: &*app_img];
            }

            let sel_style = sel!(_setIdentityImageStyle:);
            let responds_style: bool = msg_send![&*notification, respondsToSelector: sel_style];
            if responds_style {
                let style: isize = 0;
                let _: () = msg_send![&*notification, _setIdentityImageStyle: style];
            }
        }

        let center = NSUserNotificationCenter::defaultUserNotificationCenter();
        center.deliverNotification(&notification);
    });
}
