use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::current_millis;
use std::sync::Arc;

impl KdeConnectDaemon {
    pub(super) fn extract_otp_code(combined_text: &str) -> Option<String> {
        let lower = combined_text.to_ascii_lowercase();
        let has_explicit_otp_word = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .any(|w| matches!(w, "otp" | "2fa" | "passcode" | "one-time" | "onetime"));

        const OTP_PHRASES: [&str; 17] = [
            "verification code",
            "security code",
            "login code",
            "auth code",
            "authentication code",
            "confirmation code",
            "access code",
            "secret code",
            "temporary code",
            "verify your",
            "one time password",
            "one-time password",
            "one time code",
            "verification pin",
            "temporary pin",
            "your pin is",
            "your code is",
        ];

        if !has_explicit_otp_word && !OTP_PHRASES.iter().any(|p| lower.contains(p)) {
            return None;
        }

        for raw_token in combined_text.split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | '.'
                        | '!'
                        | '?'
                        | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '"'
                        | '\''
                        | '<'
                        | '>'
                )
        }) {
            let trimmed = raw_token.trim_matches('-');
            if trimmed.is_empty() {
                continue;
            }

            // Support either pure digits (4..=8 chars) or hyphenated 6-digit codes like `123-456`
            let digits_only: String = if trimmed.chars().all(|c| c.is_ascii_digit()) {
                trimmed.to_string()
            } else if trimmed.len() == 7
                && trimmed.as_bytes()[3] == b'-'
                && trimmed[..3].chars().all(|c| c.is_ascii_digit())
                && trimmed[4..].chars().all(|c| c.is_ascii_digit())
            {
                format!("{}{}", &trimmed[..3], &trimmed[4..])
            } else {
                continue;
            };

            if (4..=8).contains(&digits_only.len()) {
                // Avoid false-matching 4-digit calendar years (e.g. `2026`) unless the word `otp` is explicitly present
                if digits_only.len() == 4
                    && !has_explicit_otp_word
                    && (2024..=2035).contains(&digits_only.parse::<u16>().unwrap_or(0))
                {
                    continue;
                }
                return Some(digits_only);
            }
        }

        None
    }
}

pub(super) fn maybe_auto_copy_otp(
    daemon: &Arc<KdeConnectDaemon>,
    device_id: &str,
    device_name: &str,
    app_name: &str,
    raw_id_original: &str,
    notif_body: &str,
    otp_code: Option<&str>,
    is_new_otp: bool,
    is_silent: bool,
    timestamp_ms: i64,
) -> bool {
    let Some(code) = otp_code else {
        return false;
    };

    const FIVE_MINUTES_MS: i64 = 5 * 60 * 1000;
    const TEN_MINUTES_MS: i64 = 10 * 60 * 1000;
    let now_ms = current_millis();
    let age_ms = now_ms.saturating_sub(timestamp_ms);
    let is_within_last_5_mins = (-60_000..=FIVE_MINUTES_MS).contains(&age_ms);
    if !is_new_otp || !is_within_last_5_mins || !daemon.is_device_auto_copy_otp_allowed(device_id) {
        return false;
    }

    let mut already_copied_recently = false;
    if let Ok(mut copied_map) = daemon.recently_copied_otps.lock() {
        copied_map.retain(|_, &mut ts| now_ms.saturating_sub(ts) <= TEN_MINUTES_MS);
        let key = (device_id.to_string(), code.to_string());
        if let std::collections::hash_map::Entry::Vacant(e) = copied_map.entry(key) {
            e.insert(now_ms);
        } else {
            already_copied_recently = true;
        }
    }

    if already_copied_recently {
        return false;
    }

    log::info!(
        "[otp] Extracted OTP code '{}' from '{}' notification on '{}' (silent={}, age_ms={}), auto-copying to Mac clipboard",
        code,
        app_name,
        device_name,
        is_silent,
        age_ms
    );
    let _ = daemon.copy_text_to_clipboard(code);

    let otp_notif_title = format!("Copied OTP: {}", code);
    KdeConnectDaemon::show_macos_notification(
        &otp_notif_title,
        notif_body,
        Some(device_id),
        Some(raw_id_original),
        true,
    );
    true
}
