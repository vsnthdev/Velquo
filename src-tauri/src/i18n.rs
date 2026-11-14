//! Native (Rust-side) internationalization.
//!
//! The menu bar, tray menu, window titles and notification action titles are
//! built by Rust, so they cannot read the webview's i18next instance. To keep a
//! single source of truth, this module reads the *same* JSON catalogs the
//! frontend bundles (`src/i18n/locales/*.json`) with `include_str!`.
//!
//! The active language is process-wide (`current()`), set once at startup from
//! the persisted `AppSettings.language` and updated by the `set_language`
//! command.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// Locale tags use script subtags (`zh-Hans` / `zh-Hant`) rather than region
/// subtags, matching the frontend's `Locale` type exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhHans,
    ZhHant,
}

pub const DEFAULT_LOCALE: Locale = Locale::En;

impl Locale {
    pub fn tag(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::ZhHans => "zh-Hans",
            Locale::ZhHant => "zh-Hant",
        }
    }

    fn index(self) -> u8 {
        match self {
            Locale::En => 0,
            Locale::ZhHans => 1,
            Locale::ZhHant => 2,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => Locale::ZhHans,
            2 => Locale::ZhHant,
            _ => Locale::En,
        }
    }

    /// Maps a BCP-47 tag onto a supported locale. Mirrors `matchLocale()` in
    /// `src/i18n/index.ts`: an explicit script subtag always wins over the
    /// region subtag, and `zh-TW` / `zh-HK` / `zh-MO` imply Traditional.
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().replace('_', "-").to_ascii_lowercase();
        let mut segments = normalized.split('-');
        let base = segments.next()?;

        match base {
            "en" => Some(Locale::En),
            "zh" => {
                let rest: Vec<&str> = segments.filter(|segment| !segment.is_empty()).collect();
                if let Some(script) = rest.iter().find(|s| **s == "hans" || **s == "hant") {
                    return Some(if *script == "hant" {
                        Locale::ZhHant
                    } else {
                        Locale::ZhHans
                    });
                }
                let region = rest.iter().find(|s| s.len() == 2 || s.len() == 3);
                Some(match region {
                    Some(region) if ["tw", "hk", "mo"].contains(region) => Locale::ZhHant,
                    _ => Locale::ZhHans,
                })
            }
            _ => None,
        }
    }

    /// First supported locale among the user's preferred languages, falling back
    /// to English.
    fn from_system() -> Self {
        for tag in system_locale_tags() {
            if let Some(locale) = Locale::parse(&tag) {
                return locale;
            }
        }
        DEFAULT_LOCALE
    }
}

/// The user's preferred languages, most preferred first, as reported by macOS
/// (for example `["zh-Hant-TW", "en-US"]`).
#[cfg(target_os = "macos")]
fn system_locale_tags() -> Vec<String> {
    use objc2_foundation::NSLocale;
    NSLocale::preferredLanguages()
        .iter()
        .map(|tag| tag.to_string())
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn system_locale_tags() -> Vec<String> {
    Vec::new()
}

static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(0);

pub fn set_current(locale: Locale) {
    CURRENT_LOCALE.store(locale.index(), Ordering::Relaxed);
}

pub fn current() -> Locale {
    Locale::from_index(CURRENT_LOCALE.load(Ordering::Relaxed))
}

/// Resolves the stored preference (`system` or a concrete tag) into a locale.
pub fn resolve(preference: &str) -> Locale {
    let trimmed = preference.trim();
    if trimmed.eq_ignore_ascii_case("system") {
        return Locale::from_system();
    }
    Locale::parse(trimmed).unwrap_or_else(Locale::from_system)
}

/// Normalizes a preference coming from the frontend. Returns `None` when the
/// value is not one we accept, so a bad payload cannot corrupt the settings file.
pub fn normalize_preference(preference: &str) -> Option<String> {
    let trimmed = preference.trim();
    if trimmed.eq_ignore_ascii_case("system") {
        return Some("system".to_string());
    }
    Locale::parse(trimmed).map(|locale| locale.tag().to_string())
}

fn catalogs() -> &'static HashMap<&'static str, Value> {
    static CATALOGS: OnceLock<HashMap<&'static str, Value>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        let mut map = HashMap::new();
        for (tag, raw) in [
            ("en", include_str!("../../src/i18n/locales/en.json")),
            (
                "zh-Hans",
                include_str!("../../src/i18n/locales/zh-Hans.json"),
            ),
            (
                "zh-Hant",
                include_str!("../../src/i18n/locales/zh-Hant.json"),
            ),
        ] {
            match serde_json::from_str::<Value>(raw) {
                Ok(value) => {
                    map.insert(tag, value);
                }
                Err(error) => log::error!("[i18n] Failed to parse the {} catalog: {}", tag, error),
            }
        }
        map
    })
}

fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
    let mut node = catalogs().get(locale.tag())?;
    for segment in key.split('.') {
        node = node.get(segment)?;
    }
    node.as_str()
}

/// Looks up `key` (dot-separated) for `locale`, falling back to English and then
/// to the key itself. Native strings are few, so a linear walk is plenty.
pub fn tr(locale: Locale, key: &str) -> String {
    if let Some(value) = lookup(locale, key) {
        return value.to_string();
    }
    if locale != Locale::En {
        if let Some(value) = lookup(Locale::En, key) {
            log::warn!(
                "[i18n] Missing '{}' in {} — falling back to English",
                key,
                locale.tag()
            );
            return value.to_string();
        }
    }
    log::warn!("[i18n] Missing key '{}'", key);
    key.to_string()
}

/// Same as [`tr`], but replaces `{{name}}` placeholders.
pub fn tr_with(locale: Locale, key: &str, vars: &[(&str, &str)]) -> String {
    let mut out = tr(locale, key);
    for (name, value) in vars {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
}

pub fn tr_current(key: &str) -> String {
    tr(current(), key)
}

pub fn tr_current_with(key: &str, vars: &[(&str, &str)]) -> String {
    tr_with(current(), key, vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_script_and_region_tags() {
        assert_eq!(Locale::parse("en-US"), Some(Locale::En));
        assert_eq!(Locale::parse("zh-Hans-CN"), Some(Locale::ZhHans));
        assert_eq!(Locale::parse("zh-Hant-TW"), Some(Locale::ZhHant));
        assert_eq!(Locale::parse("zh_TW"), Some(Locale::ZhHant));
        assert_eq!(Locale::parse("zh-HK"), Some(Locale::ZhHant));
        assert_eq!(Locale::parse("zh-SG"), Some(Locale::ZhHans));
        assert_eq!(Locale::parse("zh"), Some(Locale::ZhHans));
        // An explicit script wins over the region.
        assert_eq!(Locale::parse("zh-Hans-HK"), Some(Locale::ZhHans));
        assert_eq!(Locale::parse("fr-FR"), None);
    }

    #[test]
    fn resolves_preferences() {
        assert_eq!(normalize_preference("system"), Some("system".to_string()));
        assert_eq!(normalize_preference("zh-Hant"), Some("zh-Hant".to_string()));
        assert_eq!(normalize_preference("zh_TW"), Some("zh-Hant".to_string()));
        assert_eq!(normalize_preference("klingon"), None);
    }

    #[test]
    fn reads_shared_catalogs() {
        assert_eq!(
            tr(Locale::En, "native.tray.pairNewDevice"),
            "Pair new device"
        );
        assert_eq!(
            tr(Locale::ZhHans, "native.tray.pairNewDevice"),
            "配对新设备"
        );
        assert_eq!(
            tr(Locale::ZhHant, "native.tray.pairNewDevice"),
            "配對新裝置"
        );
        assert_eq!(
            tr_with(
                Locale::ZhHant,
                "native.windowTitle.files",
                &[("device", "Pixel 8")]
            ),
            "Pixel 8 — 檔案"
        );
        // Unknown keys degrade to the key instead of panicking.
        assert_eq!(tr(Locale::En, "native.nope"), "native.nope");
    }
}
