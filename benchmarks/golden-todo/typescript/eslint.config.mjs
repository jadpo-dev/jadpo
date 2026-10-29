import tseslint from "typescript-eslint";
export default tseslint.config(
  { ignores: ["build/**", "node_modules/**", "drizzle/**", "*.mjs"] },
  ...tseslint.configs.recommendedTypeChecked,
  {
    files: ["**/*.ts"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_" },
      ],
      "no-restricted-syntax": [
        "error",
        {
          selector: 'CallExpression[callee.name="fetch"]',
          message: "Use the guarded Egress boundary.",
        },
      ],
    },
  },
  {
    files: ["src/**/*.ts"],
    ignores: [
      "src/db/store.ts",
      "src/db/schema.ts",
      "src/egress.ts",
      "src/server.ts",
      "src/host.ts",
      "src/limits.ts",
    ],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: [
                "postgres",
                "node:http",
                "node:https",
                "node:net",
                "node:tls",
                "node:dns*",
                "node:child_process",
                "bun",
              ],
              message: "Use the database/egress/server capability boundary.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["src/**/*.ts"],
    ignores: ["src/auth.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["jose", "node:crypto"],
              message:
                "Authentication and cryptography belong at the centralized auth boundary.",
            },
            {
              group: [
                "postgres",
                "node:http",
                "node:https",
                "node:net",
                "node:tls",
                "node:dns*",
                "node:child_process",
                "bun",
              ],
              message: "Use the database/egress/server capability boundary.",
            },
          ],
        },
      ],
    },
  },
  {
    files: [
      "src/server.ts",
      "src/host.ts",
      "src/limits.ts",
      "src/db/store.ts",
      "src/egress.ts",
    ],
    rules: { "no-restricted-imports": "off" },
  },
  {
    files: ["**/*.mjs"],
    extends: [],
    languageOptions: { parserOptions: { projectService: false } },
  },
);
