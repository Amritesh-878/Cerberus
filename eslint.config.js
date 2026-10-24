import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import prettier from "eslint-config-prettier";
import solid from "eslint-plugin-solid/configs/typescript";
import globals from "globals";
import tseslint from "typescript-eslint";

export default defineConfig(
  { ignores: ["dist/", "src-tauri/", "node_modules/", "coverage/"] },
  js.configs.recommended,
  tseslint.configs.strictTypeChecked,
  {
    languageOptions: {
      globals: { ...globals.browser },
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  { files: ["**/*.js"], extends: [tseslint.configs.disableTypeChecked] },
  {
    files: ["**/*.tsx"],
    ...solid,
    rules: {
      ...solid.rules,
      // React habits that break Solid's reactivity or skip its keyed rendering.
      "solid/no-destructure": "error",
      "solid/reactivity": "error",
      "solid/prefer-for": "error",
      "solid/prefer-show": "error",
    },
  },
  prettier,
);
