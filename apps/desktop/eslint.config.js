import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri", "node_modules"] },
  {
    files: ["**/*.{ts,tsx,js}"],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
    ],
    languageOptions: {
      globals: globals.browser,
    },
    plugins: {
      "react-refresh": reactRefresh,
    },
    rules: {
      "react-refresh/only-export-components": "warn",
    },
  },
  {
    // shadcn üretimi ve sözleşme dosyaları: bileşen dışı dışa aktarım serbest.
    files: ["src/components/ui/**", "src/theme/index.tsx", "src/i18n/index.tsx"],
    rules: {
      "react-refresh/only-export-components": "off",
    },
  },
  {
    // Yapılandırma dosyaları Node ortamında çalışır.
    files: ["*.config.{js,ts}"],
    languageOptions: {
      globals: globals.node,
    },
  },
);
