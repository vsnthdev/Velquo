import i18n from "i18next";
import en from "./locales/en.json";
import zhHans from "./locales/zh-Hans.json";
import zhHant from "./locales/zh-Hant.json";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initReactI18next } from "react-i18next";

/**
 * Supported UI locales, expressed as BCP-47 tags. We use script subtags
 * (`zh-Hans` / `zh-Hant`) instead of region subtags (`zh-CN` / `zh-TW`)
 * because the difference that matters here is the script, not the country.
 */
export const SUPPORTED_LOCALES = ["en", "zh-Hans", "zh-Hant"] as const;

export type Locale = (typeof SUPPORTED_LOCALES)[number];

/** What the user picked in Settings. `system` follows the macOS locale list. */
export type LanguagePreference = Locale | "system";

export const DEFAULT_LOCALE: Locale = "en";

/** Mirrors the preference stored in Rust's `AppSettings` (app_settings.json).
 * localStorage is only a cache so the first paint needs no IPC round-trip; Rust
 * remains the source of truth because it builds the native menus. */
const LANGUAGE_PREFERENCE_STORAGE_KEY = "velquo.language";

/** Language names are intentionally NOT translated: each one is shown in its
 * own script so that a user who cannot read the current UI language can still
 * find their language. */
export const LOCALE_LABELS: Record<Locale, string> = {
    en: "English",
    "zh-Hans": "简体中文",
    "zh-Hant": "繁體中文",
};

const resources = {
    en: { translation: en },
    "zh-Hans": { translation: zhHans },
    "zh-Hant": { translation: zhHant },
} as const;

export function isLocale(value: unknown): value is Locale {
    return SUPPORTED_LOCALES.includes(value as Locale);
}

export function isLanguagePreference(value: unknown): value is LanguagePreference {
    return value == "system" || isLocale(value);
}

/**
 * Maps a system locale tag (as reported by macOS, e.g. `zh-Hans-CN`,
 * `zh-Hant-TW`, `en-US`, `zh_HK`) onto one of our supported locales.
 * An explicit script subtag always wins over the region subtag.
 */
export function matchLocale(rawTag: string | null | undefined): Locale | null {
    if (!rawTag) return null;
    const subtags = rawTag.trim().replace(/_/g, "-").toLowerCase().split("-");
    const base = subtags[0];
    if (!base) return null;

    if (base == "en") return "en";
    if (base != "zh") return null;

    const rest = subtags.slice(1);
    const script = rest.find((tag) => tag == "hans" || tag == "hant");
    if (script) return script == "hant" ? "zh-Hant" : "zh-Hans";

    const region = rest.find((tag) => tag.length == 2 || tag.length == 3);
    return region == "tw" || region == "hk" || region == "mo" ? "zh-Hant" : "zh-Hans";
}

export function getSystemLocaleCandidates(): readonly string[] {
    if (typeof navigator == "undefined") return [];
    if (navigator.languages && navigator.languages.length > 0) {
        return navigator.languages;
    }
    return navigator.language ? [navigator.language] : [];
}

/** Resolves a stored preference into the concrete locale i18next should use. */
export function resolveLocale(
    preference: LanguagePreference,
    candidates: readonly string[] = getSystemLocaleCandidates(),
): Locale {
    if (preference != "system") return preference;
    for (const candidate of candidates) {
        const matched = matchLocale(candidate);
        if (matched) return matched;
    }
    return DEFAULT_LOCALE;
}

export function readLanguagePreference(): LanguagePreference {
    try {
        const stored = localStorage.getItem(LANGUAGE_PREFERENCE_STORAGE_KEY);
        if (isLanguagePreference(stored)) return stored;
    } catch {
        // localStorage can be unavailable (private mode, disabled storage).
    }
    return "system";
}

function writeLanguagePreference(preference: LanguagePreference): void {
    try {
        if (preference == "system") {
            localStorage.removeItem(LANGUAGE_PREFERENCE_STORAGE_KEY);
        } else {
            localStorage.setItem(LANGUAGE_PREFERENCE_STORAGE_KEY, preference);
        }
    } catch {
        // Non-fatal: the UI still switches for this session.
    }
}

/** Keeps `<html lang>` in sync so WebKit picks the right CJK font (PingFang
 * SC vs TC) and so `:lang()` based CSS rules apply. */
export function applyDocumentLang(locale: Locale): void {
    if (typeof document == "undefined") return;
    document.documentElement.lang = locale;
    document.documentElement.dir = "ltr";
}

/** Must run before the first render, otherwise the first paint falls back to
 * `fallbackLng` and the UI visibly flips language. */
export function initI18n(): Locale {
    const locale = resolveLocale(readLanguagePreference());

    i18n.use(initReactI18next).init({
        resources,
        lng: locale,
        // Bundled resources resolve synchronously, so `t()` is usable on the
        // very first render.
        initAsync: false,
        returnNull: false,
        // Never fall back from `zh-Hant` to `zh-Hans`: they are different
        // catalogs, not dialects of the same one.
        load: "currentOnly",
        fallbackLng: DEFAULT_LOCALE,
        supportedLngs: [...SUPPORTED_LOCALES],
        interpolation: { escapeValue: false },
    });

    applyDocumentLang(locale);
    return locale;
}

/** True when the webview runs inside the Tauri shell rather than a plain
 * browser (`bun run dev` without Tauri). */
function isRunningInTauri(): boolean {
    return typeof window != "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Switches the UI language and mirrors the choice to Rust so the menu bar,
 * tray menu, window titles and notification actions follow along. */
export async function setLanguagePreference(
    preference: LanguagePreference,
): Promise<Locale> {
    const previousPreference = readLanguagePreference();
    const previousLocale = i18n.language as Locale;

    writeLanguagePreference(preference);

    // Apply optimistically so the UI never waits on IPC; the only case where the
    // resolved locale differs is "system", where Rust's NSLocale is authoritative.
    await applyLocale(resolveLocale(preference));

    if (!isRunningInTauri()) {
        // Outside Tauri there is no native side to persist to; the switch still
        // applies for this session.
        return i18n.language as Locale;
    }

    try {
        const state = await invoke<RustLanguageState>("set_language", { preference });
        if (isLanguagePreference(state.preference)) {
            writeLanguagePreference(state.preference);
        }
        if (isLocale(state.locale)) {
            await applyLocale(state.locale);
        }
    } catch (err) {
        // Rust rejected the change, so the native menu bar and tray still show the
        // old language. Roll the webview back instead of leaving the two silently
        // out of sync, then let the caller react.
        writeLanguagePreference(previousPreference);
        await applyLocale(previousLocale);
        throw err;
    }

    return i18n.language as Locale;
}

interface RustLanguageState {
    preference: string;
    locale: string;
}

async function applyLocale(locale: Locale): Promise<void> {
    if (i18n.language != locale) {
        await i18n.changeLanguage(locale);
    }
    applyDocumentLang(locale);
}

/**
 * Reconciles the cached preference with Rust's copy, which wins: it may hold a
 * value written before the webview storage was cleared.
 */
export async function syncLanguageWithRust(): Promise<void> {
    let state: RustLanguageState;
    try {
        state = await invoke<RustLanguageState>("get_language");
    } catch {
        return; // Not running inside Tauri.
    }
    if (!isLanguagePreference(state.preference)) return;

    const cached = readLanguagePreference();
    if (cached != state.preference) {
        writeLanguagePreference(state.preference);
    }
    if (isLocale(state.locale) && i18n.language != state.locale) {
        await applyLocale(state.locale);
    }
}

/** Keeps secondary windows (file browser, updater) in step with a language change
 * made in the main window. */
export async function listenForLanguageChanges(): Promise<() => void> {
    try {
        const unlisten = await listen<string>("velquo://language-changed", () => {
            void syncLanguageWithRust();
        });
        return unlisten;
    } catch {
        return () => {};
    }
}

export { i18n };
