import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import stylistic from "@stylistic/eslint-plugin";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import perfectionist from "eslint-plugin-perfectionist";

const ALLOWED_PHOSPHOR_EXPORTS = new Set([
    "SSR",
    "Icon",
    "IconBase",
    "IconProps",
    "IconWeight",
    "IconContext",
]);

const phosphorPlugin = {
    rules: {
        "icon-suffix": {
            meta: {
                schema: [],
                fixable: "code",
                type: "suggestion",
                messages: {
                    missingSuffix:
                        "Import '{{expected}}' instead of '{{actual}}' from '@phosphor-icons/react'.",
                },
            },
            create(context) {
                return {
                    ImportDeclaration(node) {
                        if (node.source.value !== "@phosphor-icons/react") {
                            return;
                        }
                        for (const spec of node.specifiers) {
                            if (spec.type !== "ImportSpecifier") continue;
                            const importedName =
                                spec.imported.type === "Identifier"
                                    ? spec.imported.name
                                    : spec.imported.value;
                            if (
                                ALLOWED_PHOSPHOR_EXPORTS.has(importedName) ||
                                importedName.endsWith("Icon")
                            ) {
                                continue;
                            }
                            const expectedName = `${importedName}Icon`;
                            context.report({
                                node: spec,
                                messageId: "missingSuffix",
                                data: {
                                    actual: importedName,
                                    expected: expectedName,
                                },
                                fix(fixer) {
                                    const fixes = [];
                                    if (spec.local.name === importedName) {
                                        fixes.push(
                                            fixer.replaceText(
                                                spec,
                                                expectedName,
                                            ),
                                        );
                                        const vars =
                                            context.sourceCode.getDeclaredVariables(
                                                spec,
                                            );
                                        for (const v of vars) {
                                            for (const ref of v.references) {
                                                fixes.push(
                                                    fixer.replaceText(
                                                        ref.identifier,
                                                        expectedName,
                                                    ),
                                                );
                                            }
                                        }
                                    } else {
                                        fixes.push(
                                            fixer.replaceText(
                                                spec.imported,
                                                expectedName,
                                            ),
                                        );
                                    }
                                    return fixes;
                                },
                            });
                        }
                    },
                };
            },
        },
    },
};

export default tseslint.config(
    {
        ignores: ["dist/**", "src-tauri/**", "node_modules/**"],
    },
    {
        files: ["**/*.{js,ts,tsx}"],
        extends: [js.configs.recommended, ...tseslint.configs.recommended],
        languageOptions: {
            ecmaVersion: 2022,
            globals: {
                Bun: "readonly",
                ...globals.node,
                ...globals.browser,
            },
        },
        plugins: {
            perfectionist,
            "@stylistic": stylistic,
            phosphor: phosphorPlugin,
            "react-hooks": reactHooks,
            "react-refresh": reactRefresh,
        },
        rules: {
            ...reactHooks.configs.recommended.rules,
            "phosphor/icon-suffix": "error",
            "@stylistic/semi": ["error", "always"],
            "react-hooks/set-state-in-effect": "off",
            "@stylistic/jsx-indent-props": ["error", 4],
            "@stylistic/jsx-quotes": ["error", "prefer-double"],
            "@stylistic/indent": ["error", 4, { SwitchCase: 1 }],
            "@stylistic/object-curly-spacing": ["error", "always"],
            "@stylistic/quotes": ["error", "double", { avoidEscape: true }],
            "react-refresh/only-export-components": [
                "warn",
                { allowConstantExport: true },
            ],
            "perfectionist/sort-objects": [
                "error",
                {
                    order: "asc",
                    type: "line-length",
                },
            ],
            "perfectionist/sort-named-imports": [
                "error",
                {
                    order: "asc",
                    type: "line-length",
                },
            ],
            "perfectionist/sort-imports": [
                "error",
                {
                    groups: [],
                    order: "asc",
                    newlinesBetween: 0,
                    type: "line-length",
                },
            ],
        },
    },
);
