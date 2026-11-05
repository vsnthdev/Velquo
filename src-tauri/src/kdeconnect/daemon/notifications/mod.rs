mod cancel;
mod format;
mod otp;
use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::Result;
use std::sync::Arc;

impl KdeConnectDaemon {
    /// Dismiss a notification using the raw ID exposed to the UI. Internal list and macOS
    /// notification-center matching always compares normalized IDs.
    pub fn dismiss_device_notification(
        &self,
        device_id: &str,
        notification_id: &str,
    ) -> Result<()> {
        let _ = self.send_packet_to_device(
            device_id,
            &NetworkPacket::notification_cancel(notification_id),
        );
        let norm_target = Self::normalize_notif_id(notification_id);
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                entry
                    .view
                    .notifications
                    .retain(|n| Self::normalize_notif_id(&n.id) != norm_target);
            }
        }
        Self::remove_macos_notifications(device_id, &[norm_target]);
        self.notify_ui();
        Ok(())
    }

    pub(super) fn handle_notification_packet(
        self: &Arc<Self>,
        device_id: &str,
        device_name: &str,
        packet: &NetworkPacket,
    ) {
        let is_cancel = packet.get_bool("isCancel").unwrap_or(false);
        let raw_id_original = packet.get_str("id").unwrap_or("").to_string();
        let norm_id = Self::normalize_notif_id(&raw_id_original);

        if !self.is_device_notifications_allowed(device_id) {
            return;
        }

        if packet.get_bool("request").unwrap_or(false) {
            return;
        }

        if norm_id.is_empty() {
            return;
        }

        if is_cancel {
            cancel::handle_notification_cancel(self, device_id, &norm_id);
            return;
        }

        let app_name = packet.get_str("appName").unwrap_or("").trim().to_string();
        if app_name.eq_ignore_ascii_case("KDE Connect") {
            return;
        }

        let title = packet.get_str("title").unwrap_or("").trim().to_string();
        let text = packet.get_str("text").unwrap_or("").trim().to_string();
        let ticker = packet.get_str("ticker").unwrap_or("").trim().to_string();

        if title.is_empty() && text.is_empty() && ticker.is_empty() {
            return;
        }

        let is_clearable = packet.get_bool("isClearable").unwrap_or(true);
        let is_request_answer = packet.get_bool("requestAnswer").unwrap_or(false);
        let is_silent = packet.get_bool("silent").unwrap_or(false);
        let timestamp_ms = packet
            .get_i64("time")
            .filter(|&t| t > 0)
            .unwrap_or_else(current_millis);

        let combined = format!("{} {} {}", title, text, ticker);
        let otp_code = Self::extract_otp_code(&combined);

        let is_whatsapp = app_name.eq_ignore_ascii_case("whatsapp")
            || app_name.eq_ignore_ascii_case("whatsapp business")
            || norm_id.contains("|com.whatsapp|")
            || norm_id.contains("|com.whatsapp.w4b|");
        if is_whatsapp && otp_code.is_none() && self.is_ignore_whatsapp_conversations_enabled() {
            log::debug!(
                "[notifications] Ignoring non-OTP WhatsApp conversation notification from '{}'",
                device_name
            );
            return;
        }

        let notif_item = PhoneNotification {
            id: raw_id_original.clone(),
            app_name: if app_name.is_empty() {
                device_name.to_string()
            } else {
                app_name.clone()
            },
            title: title.clone(),
            text: text.clone(),
            ticker: ticker.clone(),
            is_clearable,
            timestamp_ms,
            otp_code: otp_code.clone(),
        };

        let incoming_pkg = Self::extract_notif_package(&norm_id).map(|s| s.to_string());
        let incoming_is_generic_summary = Self::is_generic_summary_placeholder(&notif_item);

        let mut is_new_or_changed = false;
        let mut is_new_otp = false;
        if let Ok(mut devices) = self.devices.lock() {
            if let Some(entry) = devices.get_mut(device_id) {
                // If this incoming notification is a generic group-summary placeholder (`|null|`)
                // and we already have a specific notification for the same package, ignore the redundant summary card!
                if incoming_is_generic_summary {
                    if let Some(ref pkg) = incoming_pkg {
                        let has_specific = entry.view.notifications.iter().any(|n| {
                            let n_norm = Self::normalize_notif_id(&n.id);
                            Self::extract_notif_package(&n_norm) == Some(pkg.as_str())
                                && !Self::is_generic_summary_placeholder(n)
                        });
                        if has_specific {
                            return;
                        }
                    }
                } else if let Some(ref pkg) = incoming_pkg {
                    // Conversely, when a specific notification arrives for a package, remove any generic summary placeholder for that package
                    entry.view.notifications.retain(|n| {
                        let n_norm = Self::normalize_notif_id(&n.id);
                        let same_pkg = Self::extract_notif_package(&n_norm) == Some(pkg.as_str());
                        !(same_pkg && Self::is_generic_summary_placeholder(n))
                    });
                }

                if let Some(existing) = entry
                    .view
                    .notifications
                    .iter_mut()
                    .find(|n| Self::normalize_notif_id(&n.id) == norm_id)
                {
                    if existing.title != notif_item.title
                        || existing.text != notif_item.text
                        || existing.ticker != notif_item.ticker
                    {
                        is_new_or_changed = true;
                    }
                    if notif_item.otp_code.is_some() && existing.otp_code != notif_item.otp_code {
                        is_new_otp = true;
                    }
                    *existing = notif_item;
                } else {
                    is_new_or_changed = true;
                    if notif_item.otp_code.is_some() {
                        is_new_otp = true;
                    }
                    entry.view.notifications.insert(0, notif_item);
                }

                entry
                    .view
                    .notifications
                    .sort_by_key(|b| std::cmp::Reverse(b.timestamp_ms));
                if entry.view.notifications.len() > 25 {
                    entry.view.notifications.truncate(25);
                }
            }
        }

        self.notify_ui();

        let notif_heading = if !app_name.is_empty() {
            format!("{} • {}", app_name, device_name)
        } else {
            device_name.to_string()
        };
        let notif_body = if !title.is_empty() && !text.is_empty() && title != app_name {
            format!("{}: {}", title, text)
        } else if !text.is_empty() {
            text.clone()
        } else if !ticker.is_empty() {
            ticker.clone()
        } else {
            title.clone()
        };

        // If an OTP is detected, auto-copy it when enabled and fresh.
        if otp::maybe_auto_copy_otp(
            self,
            device_id,
            device_name,
            &app_name,
            &raw_id_original,
            &notif_body,
            otp_code.as_deref(),
            is_new_otp,
            is_silent,
            timestamp_ms,
        ) {
            return;
        }

        // For non-OTP live, non-silent notifications, show the standard native macOS notification
        if is_new_or_changed && !is_request_answer && !is_silent {
            Self::show_macos_notification(
                &notif_heading,
                &notif_body,
                Some(device_id),
                Some(&raw_id_original),
                true,
            );
        }
    }
}
