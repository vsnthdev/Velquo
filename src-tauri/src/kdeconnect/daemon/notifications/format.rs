use crate::kdeconnect::daemon::*;

impl KdeConnectDaemon {
    pub(crate) fn normalize_notif_id(raw: &str) -> String {
        let mut s = raw.replace(['\r', '\n'], "").trim().to_string();
        if let Some(idx) = s.find("org.kde.kdeconnect_tp::") {
            s = s[idx + "org.kde.kdeconnect_tp::".len()..].to_string();
        }
        s
    }

    pub(super) fn extract_notif_package(norm_id: &str) -> Option<&str> {
        // Android StatusBarNotification key format: `userId|packageName|id|tag|uid`
        let mut parts = norm_id.split('|');
        let _user = parts.next()?;
        let pkg = parts.next()?;
        if pkg.contains('.') {
            Some(pkg)
        } else {
            None
        }
    }

    pub(super) fn is_group_summary_notif_id(norm_id: &str) -> bool {
        // Group summary notifications typically have `null` as the 4th pipe-separated segment (`tag`)
        let parts: Vec<&str> = norm_id.split('|').collect();
        parts.len() >= 4 && parts[3] == "null"
    }

    pub(super) fn is_generic_summary_placeholder(n: &PhoneNotification) -> bool {
        let norm = Self::normalize_notif_id(&n.id);
        let title_generic = n.title.is_empty() || n.title.eq_ignore_ascii_case(&n.app_name);
        let body_redacted = n
            .text
            .to_ascii_lowercase()
            .contains("sensitive notification content hidden");
        Self::is_group_summary_notif_id(&norm) && (title_generic || body_redacted)
    }
}
