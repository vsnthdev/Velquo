use crate::kdeconnect::daemon::*;

pub(super) fn handle_notification_cancel(
    daemon: &KdeConnectDaemon,
    device_id: &str,
    norm_id: &str,
) {
    let mut removed = false;
    let mut ids_to_remove = vec![norm_id.to_string()];
    let cancel_pkg = KdeConnectDaemon::extract_notif_package(norm_id).map(|s| s.to_string());
    let cancel_is_summary = KdeConnectDaemon::is_group_summary_notif_id(norm_id);

    if let Ok(mut devices) = daemon.devices.lock() {
        if let Some(entry) = devices.get_mut(device_id) {
            let before_ids: Vec<String> = entry
                .view
                .notifications
                .iter()
                .map(|n| KdeConnectDaemon::normalize_notif_id(&n.id))
                .collect();
            let before = before_ids.len();

            // 1. Remove exact normalized ID match
            entry.view.notifications.retain(|n| {
                let existing_norm = KdeConnectDaemon::normalize_notif_id(&n.id);
                existing_norm != norm_id
                    && !existing_norm.ends_with(&format!("::{}", norm_id))
                    && !norm_id.ends_with(&format!("::{}", existing_norm))
            });

            // 2. If Android cancelled the group summary (`|null|`), or if a child notification
            // was cancelled leaving only a generic summary placeholder for that package, clean it up
            if let Some(ref pkg) = cancel_pkg {
                if cancel_is_summary && entry.view.notifications.len() == before {
                    // User swiped away the grouped notification stack or Android cancelled a summary ID
                    // Remove any generic summary placeholder for this package
                    entry.view.notifications.retain(|n| {
                        let n_norm = KdeConnectDaemon::normalize_notif_id(&n.id);
                        let same_pkg =
                            KdeConnectDaemon::extract_notif_package(&n_norm) == Some(pkg.as_str());
                        !(same_pkg && KdeConnectDaemon::is_generic_summary_placeholder(n))
                    });
                    // If still nothing was removed (e.g. only 1 notification from this package existed with a different sub-id), remove it
                    if entry.view.notifications.len() == before {
                        let pkg_count = entry
                            .view
                            .notifications
                            .iter()
                            .filter(|n| {
                                KdeConnectDaemon::extract_notif_package(
                                    &KdeConnectDaemon::normalize_notif_id(&n.id),
                                ) == Some(pkg.as_str())
                            })
                            .count();
                        if pkg_count == 1 {
                            entry.view.notifications.retain(|n| {
                                KdeConnectDaemon::extract_notif_package(
                                    &KdeConnectDaemon::normalize_notif_id(&n.id),
                                ) != Some(pkg.as_str())
                            });
                        }
                    }
                } else {
                    // If a specific child notification was removed, check if only a generic summary placeholder remains for this package
                    let remaining_specific_for_pkg = entry
                        .view
                        .notifications
                        .iter()
                        .filter(|n| {
                            let n_norm = KdeConnectDaemon::normalize_notif_id(&n.id);
                            KdeConnectDaemon::extract_notif_package(&n_norm) == Some(pkg.as_str())
                                && !KdeConnectDaemon::is_generic_summary_placeholder(n)
                        })
                        .count();
                    if remaining_specific_for_pkg == 0 {
                        entry.view.notifications.retain(|n| {
                            let n_norm = KdeConnectDaemon::normalize_notif_id(&n.id);
                            let same_pkg = KdeConnectDaemon::extract_notif_package(&n_norm)
                                == Some(pkg.as_str());
                            !(same_pkg && KdeConnectDaemon::is_generic_summary_placeholder(n))
                        });
                    }
                }
            }

            let after_ids: Vec<String> = entry
                .view
                .notifications
                .iter()
                .map(|n| KdeConnectDaemon::normalize_notif_id(&n.id))
                .collect();
            for old_id in before_ids {
                if !after_ids.contains(&old_id) && !ids_to_remove.contains(&old_id) {
                    ids_to_remove.push(old_id);
                }
            }

            removed = entry.view.notifications.len() != before;
        }
    }
    KdeConnectDaemon::remove_macos_notifications(device_id, &ids_to_remove);
    if removed {
        daemon.notify_ui();
    }
}
