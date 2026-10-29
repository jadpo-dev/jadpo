import { ESLint } from "eslint";
import { expect, it } from "vitest";
import { apiDocument, routeSchemas } from "../src/app.ts";
it("rejects egress bypasses and forgotten promises before execution", async () => {
  const eslint = new ESLint();
  const [result] = await eslint.lintText(
    "import { request } from 'node:https';\nPromise.resolve(request);\n",
    { filePath: "src/todos.ts" },
  );
  expect(result?.messages.map((message) => message.ruleId)).toContain(
    "no-restricted-imports",
  );
  expect(result?.messages.map((message) => message.ruleId)).toContain(
    "@typescript-eslint/no-floating-promises",
  );
}, 15000); // Cold type-aware ESLint startup; not an application runtime budget.
it("derives protected OpenAPI operations from actual installed route schemas", () => {
  const document = apiDocument();
  const paths = document.paths ?? {};
  expect(
    Object.values(paths).reduce(
      (n, path) => n + Object.keys(path ?? {}).length,
      0,
    ),
  ).toBe(routeSchemas.length);
  expect(JSON.stringify(paths)).not.toContain("/health/");
  for (const route of routeSchemas)
    expect(paths[route.path]?.[route.method]?.security).toEqual([
      { browser_session: [] },
      { bearer: [] },
    ]);
});
