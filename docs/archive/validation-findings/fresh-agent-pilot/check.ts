// Frozen blind grader for one internal pilot. Run only against a disposable DB.
import { Database } from "bun:sqlite";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
const [appRoot, resultPath, mode = "grade"] = process.argv.slice(2);
if (!appRoot || !resultPath || !Bun.env.SQLITE_PATH) throw new Error("app, results and disposable SQLITE_PATH required");
delete Bun.env.DATABASE_URL;
const app = await import(pathToFileURL(join(appRoot, "build/target/app.ts")).href);
const id = "10000000-0000-4000-8000-000000000001";
const missing = "10000000-0000-4000-8000-000000000099";
const original = { id, title: "First card", note: "keep this" };
const results: { obligation: string; passed: boolean; detail?: string }[] = [];
function equal(actual: any, expected: any) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(`Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}
function card(actual: any, expected: any) {
  equal(Object.keys(actual).sort(), ["id", "note", "title"]);
  for (const key of ["id", "title", "note"]) equal(actual[key], expected[key]);
}
async function post(path: string, value: any, status: number, code?: string) {
  const response = await app.handleRequest(new Request(`http://pilot.local${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(value) }));
  const body = await response.json();
  equal(response.status, status);
  if (code) equal(body.error.code, code);
  return body;
}
async function check(obligation: string, action: () => Promise<void>) {
  try { await action(); results.push({ obligation, passed: true }); }
  catch (error) { results.push({ obligation, passed: false, detail: String(error) }); }
}
if (mode === "read-fresh") {
  card(await post("/cards/find", { id }, 200), { id, title: "Revised", note: null });
  process.exit(0);
}
await check("valid creation round-trips exact fields", async () => {
  card(await post("/cards", original, 200), original);
  card(await post("/cards/find", { id }, 200), original);
});
await check("duplicate identity returns card_conflict and preserves original row", async () => {
  await post("/cards", { ...original, title: "Replacement" }, 409, "card_conflict");
  card(await post("/cards/find", { id }, 200), original);
});
await check("unknown identity returns card_missing", async () => { await post("/cards/find", { id: missing }, 404, "card_missing"); });
await check("short or overlong title rejects with 400 before insertion", async () => {
  for (const title of ["ab", "abcdefghijklm"]) await post("/cards", { ...original, id: missing, title }, 400, "invalid_request");
  await post("/cards/find", { id: missing }, 404, "card_missing");
});
await check("malformed UUID rejects with 400", async () => { await post("/cards", { ...original, id: "not-a-uuid" }, 400, "invalid_request"); });
await check("unknown top-level fields reject with 400", async () => { await post("/cards", { ...original, id: missing, surprise: true }, 400, "invalid_request"); });
await check("patching title preserves an omitted note", async () => { card(await post("/cards/patch", { id, changes: { title: "Revised" } }, 200), { ...original, title: "Revised" }); });
await check("explicit null patch clears note", async () => { card(await post("/cards/patch", { id, changes: { note: null } }, 200), { id, title: "Revised", note: null }); });
await check("empty patch returns empty_patch and preserves state", async () => {
  await post("/cards/patch", { id, changes: {} }, 422, "empty_patch");
  card(await post("/cards/find", { id }, 200), { id, title: "Revised", note: null });
});
await check("invalid patch returns 400 and preserves state", async () => {
  await post("/cards/patch", { id, changes: { title: "x", note: "must not persist" } }, 400, "invalid_request");
  card(await post("/cards/find", { id }, 200), { id, title: "Revised", note: null });
});
await check("patching a missing row returns card_missing", async () => { await post("/cards/patch", { id: missing, changes: { title: "Unknown" } }, 404, "card_missing"); });
await check("stored state survives a fresh process", async () => {
  const run = Bun.spawnSync([process.execPath, "--no-install", "--env-file=/dev/null", import.meta.path, appRoot, resultPath, "read-fresh"], { env: { ...process.env }, stdout: "pipe", stderr: "pipe" });
  if (run.exitCode !== 0) throw new Error(String(run.stderr));
});
const expected = JSON.parse(readFileSync(new URL("requirements.json", import.meta.url), "utf8")).obligations;
equal(results.map(result => result.obligation), expected);
writeFileSync(resultPath, JSON.stringify({ passed: results.every(result => result.passed), results }, null, 2) + "\n");
process.exit(results.every(result => result.passed) ? 0 : 1);
