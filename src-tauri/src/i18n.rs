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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    Ja,
    Ko,
    Ru,
    ZhHans,
    ZhHant,
}

pub const DEFAULT_LOCALE: Locale = Locale::En;

impl Locale {
    pub fn tag(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Ja => "ja",
            Locale::Ko => "ko",
            Locale::Ru => "ru",
            Locale::ZhHans => "zh-Hans",
            Locale::ZhHant => "zh-Hant",
        }
    }

    fn index(self) -> u8 {
        match self {
            Locale::En => 0,
            Locale::Ja => 1,
            Locale::Ko => 2,
            Locale::Ru => 3,
            Locale::ZhHans => 4,
            Locale::ZhHant => 5,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => Locale::Ja,
            2 => Locale::Ko,
            3 => Locale::Ru,
            4 => Locale::ZhHans,
            5 => Locale::ZhHant,
            _ => Locale::En,
        }
    }

    /// Maps a BCP-47 tag onto a supported locale. Mirrors `matchLocale()` in
    /// `src/i18n/index.ts`: for Chinese an explicit script subtag always wins
    /// over the region subtag, and `zh-TW` / `zh-HK` / `zh-MO` imply
    /// Traditional.
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().replace('_', "-").to_ascii_lowercase();
        let mut segments = normalized.split('-');
        let base = segments.next()?;

        match base {
            "en" => Some(Locale::En),
            "ja" => Some(Locale::Ja),
            "ko" => Some(Locale::Ko),
            "ru" => Some(Locale::Ru),
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

    /// CLDR cardinal plural suffix (`one`, `few`, `many`, `other`) matching
    /// i18next v4 key conventions.
    fn plural_suffix(self, count: usize) -> &'static str {
        match self {
            Locale::En => {
                if count == 1 {
                    "one"
                } else {
                    "other"
                }
            }
            Locale::Ru => {
                let mod10 = count % 10;
                let mod100 = count % 100;
                if mod10 == 1 && mod100 != 11 {
                    "one"
                } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                    "few"
                } else {
                    "many"
                }
            }
            Locale::Ja | Locale::Ko | Locale::ZhHans | Locale::ZhHant => "other",
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
            ("ja", include_str!("../../src/i18n/locales/ja.json")),
            ("ko", include_str!("../../src/i18n/locales/ko.json")),
            ("ru", include_str!("../../src/i18n/locales/ru.json")),
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

fn lookup_plural(locale: Locale, key: &str, count: usize) -> Option<&'static str> {
    let suffixed = format!("{key}_{}", locale.plural_suffix(count));
    lookup(locale, &suffixed).or_else(|| lookup(locale, key))
}

fn replace_vars(mut out: String, vars: &[(&str, &str)]) -> String {
    for (name, value) in vars {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
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
    replace_vars(tr(locale, key), vars)
}

/// Looks up a pluralized message (`<key>_<suffix>`, falling back to `<key>` and
/// then English) and replaces `{{name}}` placeholders.
pub fn tr_plural_with(locale: Locale, key: &str, count: usize, vars: &[(&str, &str)]) -> String {
    let template = if let Some(value) = lookup_plural(locale, key, count) {
        value.to_string()
    } else if locale != Locale::En {
        if let Some(value) = lookup_plural(Locale::En, key, count) {
            log::warn!(
                "[i18n] Missing '{}' in {} — falling back to English",
                key,
                locale.tag()
            );
            value.to_string()
        } else {
            log::warn!("[i18n] Missing key '{}'", key);
            key.to_string()
        }
    } else {
        log::warn!("[i18n] Missing key '{}'", key);
        key.to_string()
    };
    replace_vars(template, vars)
}

pub fn tr_current(key: &str) -> String {
    tr(current(), key)
}

pub fn tr_current_with(key: &str, vars: &[(&str, &str)]) -> String {
    tr_with(current(), key, vars)
}

pub fn tr_current_plural_with(key: &str, count: usize, vars: &[(&str, &str)]) -> String {
    tr_plural_with(current(), key, count, vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_script_and_region_tags() {
        assert_eq!(Locale::parse("en-US"), Some(Locale::En));
        assert_eq!(Locale::parse("ja-JP"), Some(Locale::Ja));
        assert_eq!(Locale::parse("ko-KR"), Some(Locale::Ko));
        assert_eq!(Locale::parse("ko_KR"), Some(Locale::Ko));
        assert_eq!(Locale::parse("ru"), Some(Locale::Ru));
        assert_eq!(Locale::parse("ru-RU"), Some(Locale::Ru));
        assert_eq!(Locale::parse("ru_RU"), Some(Locale::Ru));
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
        assert_eq!(normalize_preference("ja"), Some("ja".to_string()));
        assert_eq!(normalize_preference("ko-KR"), Some("ko".to_string()));
        assert_eq!(normalize_preference("ru"), Some("ru".to_string()));
        assert_eq!(normalize_preference("ru-RU"), Some("ru".to_string()));
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
            tr(Locale::Ja, "native.tray.pairNewDevice"),
            "新しいデバイスをペアリング"
        );
        assert_eq!(
            tr(Locale::Ko, "native.tray.pairNewDevice"),
            "새 기기 페어링"
        );
        assert_eq!(
            tr(Locale::Ru, "native.tray.pairNewDevice"),
            "Связать устройство"
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
                Locale::Ru,
                "native.windowTitle.files",
                &[("device", "Pixel 8")]
            ),
            "Pixel 8 — Файлы"
        );
        assert_eq!(
            tr_with(
                Locale::ZhHant,
                "native.windowTitle.files",
                &[("device", "Pixel 8")]
            ),
            "Pixel 8 — 檔案"
        );
        // The tray's quit action is native-only, so guard that the key exists.
        assert_eq!(tr(Locale::En, "native.tray.quit"), "Quit Velquo");
        assert_eq!(tr(Locale::Ru, "native.tray.quit"), "Завершить Velquo");
        // Unknown keys degrade to the key instead of panicking.
        assert_eq!(tr(Locale::En, "native.nope"), "native.nope");
    }

    #[test]
    fn resolves_plural_forms() {
        let key = "native.notification.filesReceived.title";
        assert_eq!(
            tr_plural_with(Locale::En, key, 3, &[("device", "Pixel 8"), ("count", "3")]),
            "Received 3 files from Pixel 8"
        );
        assert_eq!(
            tr_plural_with(Locale::Ru, key, 2, &[("device", "Pixel 8"), ("count", "2")]),
            "Получено 2 файла от Pixel 8"
        );
        assert_eq!(
            tr_plural_with(Locale::Ru, key, 5, &[("device", "Pixel 8"), ("count", "5")]),
            "Получено 5 файлов от Pixel 8"
        );
        assert_eq!(
            tr_plural_with(
                Locale::Ru,
                key,
                11,
                &[("device", "Pixel 8"), ("count", "11")]
            ),
            "Получено 11 файлов от Pixel 8"
        );
        assert_eq!(
            tr_plural_with(
                Locale::Ru,
                key,
                21,
                &[("device", "Pixel 8"), ("count", "21")]
            ),
            "Получен 21 файл от Pixel 8"
        );
        assert_eq!(
            tr_plural_with(
                Locale::Ru,
                key,
                22,
                &[("device", "Pixel 8"), ("count", "22")]
            ),
            "Получено 22 файла от Pixel 8"
        );
    }
}
