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
    },
  },
  {
    // Business code reaches the database through the Store and signs nothing
    // itself: postgres.js lives in src/db, cryptography in src/auth.ts.
    files: ["src/**/*.ts"],
    ignores: ["src/db/store.ts", "src/auth.ts", "src/server.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["postgres", "node:crypto", "node:http", "node:net"],
              message:
                "Use the Store (src/db/store.ts) or the auth boundary (src/auth.ts).",
            },
          ],
        },
      ],
    },
  },
);
