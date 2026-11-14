#!/usr/bin/env bun
/**
 * Guards the three locale catalogs against drift.
 *
 * Checks, for every catalog against `en.json` (the source of truth):
 *   - missing keys (would silently fall back to English)
 *   - extra keys (dead translations, or a typo in the key name)
 *   - interpolation placeholders that do not match (`{{device}}` vs `{{devcie}}`)
 *   - untranslated values (same string as English, ignoring the locale labels)
 *
 * Usage: bun run i18n:check [--fix-missing]
 *   --fix-missing copies missing English entries into the target catalog so a
 *   translator can see exactly what is left to do. Existing values are kept.
 */
import { readFileSync, writeFileSync } from "node:fs";

const localesDir = new URL("../src/i18n/locales/", import.meta.url);
const SOURCE_LOCALE = "en";

type Catalog = Record<string, unknown>;
type FlatCatalog = Map<string, string>;

const LOCALES = ["en", "zh-Hans", "zh-Hant"] as const;

/** Values that are legitimately identical across locales. */
const INTENTIONALLY_IDENTICAL = new Set(["settings.version"]);

/** i18next plural suffixes (CLDR v4). A locale only needs the forms its own
 * plural rules define, so key parity is checked on the base key. */
const PLURAL_SUFFIXES = ["_zero", "_one", "_two", "_few", "_many", "_other"];

function pluralBase(key: string): string {
    const suffix = PLURAL_SUFFIXES.find((candidate) => key.endsWith(candidate));
    return suffix ? key.slice(0, -suffix.length) : key;
}

function loadCatalog(locale: string): Catalog {
    const raw = readFileSync(new URL(`${locale}.json`, localesDir), "utf8");
    return JSON.parse(raw) as Catalog;
}

function flatten(catalog: Catalog, prefix = ""): FlatCatalog {
    const flat: FlatCatalog = new Map();
    for (const [key, value] of Object.entries(catalog)) {
        const path = prefix ? `${prefix}.${key}` : key;
        if (value && typeof value == "object" && !Array.isArray(value)) {
            for (const [nestedKey, nestedValue] of flatten(value as Catalog, path)) {
                flat.set(nestedKey, nestedValue);
            }
        } else if (typeof value == "string") {
            flat.set(path, value);
        } else {
            throw new Error(`${path} is not a string: ${String(value)}`);
        }
    }
    return flat;
}

function localeKey(locale: string, key: string): string {
    return `${locale}.json → ${key}`;
}

function placeholders(value: string): string[] {
    return [...value.matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)]
        .map((match) => match[1])
        .sort();
}

function unflatten(flat: FlatCatalog): Catalog {
    const root: Catalog = {};
    for (const [path, value] of flat) {
        const segments = path.split(".");
        let cursor = root;
        for (const segment of segments.slice(0, -1)) {
            const next = cursor[segment];
            if (!next || typeof next != "object") {
                cursor[segment] = {};
            }
            cursor = cursor[segment] as Catalog;
        }
        cursor[segments[segments.length - 1]] = value;
    }
    return root;
}

const fixMissing = process.argv.includes("--fix-missing");

const sourceFlat = flatten(loadCatalog(SOURCE_LOCALE));
const problems: string[] = [];

// Collapse plural forms so `_one` / `_other` count as one logical message.
const groupByBase = (flat: FlatCatalog): Map<string, FlatCatalog> => {
    const grouped = new Map<string, FlatCatalog>();
    for (const [key, value] of flat) {
        const base = pluralBase(key);
        const forms = grouped.get(base) ?? new Map<string, string>();
        forms.set(key, value);
        grouped.set(base, forms);
    }
    return grouped;
};

const sourceGrouped = groupByBase(sourceFlat);

/** The representative string for a message, used for placeholder comparison. */
const representative = (forms: FlatCatalog): string => {
    const first = [...forms.keys()][0];
    return (first != undefined ? forms.get(first) : undefined) ?? "";
};

for (const locale of LOCALES) {
    if (locale == SOURCE_LOCALE) continue;

    const catalog = loadCatalog(locale);
    const flat = flatten(catalog);
    const grouped = groupByBase(flat);

    for (const [base, sourceForms] of sourceGrouped) {
        const forms = grouped.get(base);
        if (!forms || forms.size == 0) {
            problems.push(
                `✗ ${localeKey(locale, base)} missing (en: "${representative(sourceForms)}")`,
            );
            if (fixMissing) {
                for (const [key, value] of sourceForms) flat.set(key, value);
            }
            continue;
        }

        const sourceSlots = placeholders(representative(sourceForms));
        for (const [key, value] of forms) {
            const slots = placeholders(value);
            if (sourceSlots.join(",") != slots.join(",")) {
                problems.push(
                    `✗ ${localeKey(locale, key)} placeholder mismatch: ` +
                        `en has [${sourceSlots.join(", ")}], ${locale} has [${slots.join(", ")}]`,
                );
            }
        }

        const value = representative(forms);
        if (
            value.trim() == representative(sourceForms).trim() &&
            !INTENTIONALLY_IDENTICAL.has(base) &&
            !/\{\{/.test(value)
        ) {
            problems.push(`! ${localeKey(locale, base)} is identical to English: "${value}"`);
        }
    }

    for (const [base, forms] of grouped) {
        if (!sourceGrouped.has(base)) {
            for (const key of forms.keys()) {
                problems.push(`✗ ${localeKey(locale, key)} is not present in en.json`);
            }
        }
    }

    if (fixMissing) {
        writeFileSync(
            new URL(`${locale}.json`, localesDir),
            `${JSON.stringify(unflatten(flat), null, 4)}\n`,
        );
    }
}

const errors = problems.filter((line) => line.startsWith("✗"));
const warnings = problems.filter((line) => line.startsWith("!"));

if (problems.length == 0) {
    console.log(
        `==> i18n catalogs OK: ${sourceFlat.size} keys × ${LOCALES.length} locales.`,
    );
    process.exit(0);
}

for (const line of problems) {
    console.error(line);
}

console.error(
    `\n==> ${errors.length} error(s), ${warnings.length} warning(s) across ${LOCALES.length} catalogs.`,
);
if (fixMissing) {
    console.error("    Missing keys were copied from en.json — translate them, then re-run.");
}

process.exit(errors.length > 0 ? 1 : 0);
