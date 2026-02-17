import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import eslintConfigPrettier from "eslint-config-prettier";

export default tseslint.config(
  // Only lint src/ directory
  { files: ["src/**/*.{ts,tsx}"] },

  // Recommended rules
  js.configs.recommended,
  ...tseslint.configs.recommended,

  // React Hooks rules
  {
    plugins: { "react-hooks": reactHooks },
    rules: reactHooks.configs.recommended.rules,
  },

  // React Refresh rules for Vite HMR
  {
    plugins: { "react-refresh": reactRefresh },
    rules: {
      "react-refresh/only-export-components": [
        "warn",
        { allowConstantExport: true },
      ],
    },
  },

  // Disable ESLint rules that conflict with Prettier
  eslintConfigPrettier
);
