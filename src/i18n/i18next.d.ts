import type en from "./locales/en.json";

/**
 * Makes `t()` keys type-checked and autocompleted. `en.json` is the source of
 * truth for the key space; every other catalog must mirror it (enforced by
 * `bun run i18n:check`).
 */
declare module "i18next" {
    interface CustomTypeOptions {
        defaultNS: "translation";
        resources: {
            translation: typeof en;
        };
    }
}
