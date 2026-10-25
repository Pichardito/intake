// @ts-check
import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/**", "vendor/**", "node_modules/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.ts", "test/**/*.ts"],
    languageOptions: { globals: { ...globals.browser } },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
  {
    files: ["mock/**/*.mjs", "e2e/**/*.mjs"],
    // e2e files also contain page.evaluate callbacks that run in the browser.
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
  },
);
