import js from "@eslint/js";
import globals from "globals";

export default [
  { ignores: ["node_modules/", "*.vsix"] },
  js.configs.recommended,
  {
    files: ["**/*.js"],
    languageOptions: { sourceType: "commonjs", globals: globals.node },
  },
];
