import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

// Counterfactual conformance fixture, never an emitted/public admission path.
// Keep every imported native implementation and every emitted body unchanged.
export async function privateRetryApplication(path: string, enabled: boolean) {
  let source = readFileSync(path, "utf8");
  const closed = "const __authenticatedRetryCandidate = true;\n      __operation = allowOperationRetries(__operation, false);";
  if (source.split(closed).length - 1 !== 3) throw Error("Expected exactly three checked protected candidates");
  if (enabled) source = source.replaceAll(closed, "const __authenticatedRetryCandidate = true;\n      __operation = allowOperationRetries(__operation, __authenticatedRetryCandidate);");
  const names: string[] = [], values: unknown[] = [];
  for (const match of source.matchAll(/^import \{([^}]+)\} from "([^"]+)";\n/gm)) {
    const imported = await import(pathToFileURL(join(dirname(path), match[2])).href);
    for (const binding of match[1].split(",")) {
      const [original, alias] = binding.trim().split(/\s+as\s+/u);
      names.push(alias ?? original); values.push(imported[original]);
    }
  }
  source = source.replace(/^import \{[^}]+\} from "[^"]+";\n/gm, "")
    .replace(/\bexport /gu, "").replaceAll("import.meta.url", JSON.stringify(pathToFileURL(path).href)).replaceAll("import.meta.main", "false");
  const javascript = new Bun.Transpiler({ loader: "ts" }).transformSync(source)
    + "\nreturn {initializeApplication,authenticationHost,handleRequest};";
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  return new AsyncFunction(...names, javascript)(...values);
}
