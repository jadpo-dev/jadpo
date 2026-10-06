import { afterAll, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const postgresUrl = Bun.env.JADPO_LIFECYCLE_DATABASE_URL;
if (postgresUrl) {
  const url = new URL(postgresUrl);
  if (!["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Requires disposable local PostgreSQL");
}
delete Bun.env.DATABASE_URL; delete Bun.env.SQLITE_PATH;
const root = mkdtempSync(join(tmpdir(), "jadpo-relationship-conflicts-")), project = join(root, "application");
mkdirSync(project);
// The existing accepted lifecycle fixture supplies real retained child/FK SQL.
// A generic declared conflict pressures the public normalization precedence.
const source = readFileSync(resolve("tests/compile/pass/lifecycle/170_lifecycle_initial_values.jadpo"), "utf8")
  .replace("action create_todo(user_id: User.id, title: Todo.title, status: Todo.status) -> Todo {", "action create_todo(user_id: User.id, title: Todo.title, status: Todo.status) -> Todo fails TodoConflict {")
  .replace("status: status\n        }\n    }", "status: status\n        } conflict: TodoConflict\n    }");
writeFileSync(join(project, "app.jadpo"), source);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode) throw Error(`Relationship fixture failed: ${build.stdout}\n${build.stderr}`);
const path = join(root, "application.sqlite");
if (postgresUrl) Bun.env.DATABASE_URL = postgresUrl; else Bun.env.SQLITE_PATH = path;
Bun.env.JADPO_LIFECYCLE_RETENTION = "30d";
const module = await import(pathToFileURL(join(project, "build/target/persistence.ts")).href);
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const { persistence, PersistenceFault, OutcomeUnknownFault, normalizePersistenceFailure } = module;
const sql = postgresUrl ? new SQL({ url: postgresUrl, prepare: false }) : null, sqlite = sql ? null : new Database(path);
sqlite?.exec("PRAGMA foreign_keys=ON");
const rows = (text: string, values: unknown[] = []): Promise<any[]> | any[] => sql ? sql.unsafe(text, values) : sqlite!.prepare(text).all(...values);
const instant = "2026-10-06T01:00:00.000Z";
const client = persistence.withOperationTime(instant);
const expected = { status: 409, code: "relationship_conflict", message: "The operation conflicts with a relationship." };
afterAll(async () => { if (sql) await sql.close(); sqlite?.close(); delete Bun.env.DATABASE_URL; delete Bun.env.SQLITE_PATH; delete Bun.env.JADPO_LIFECYCLE_RETENTION; rmSync(root, { recursive: true, force: true }); });
async function rejected(work: () => Promise<unknown> | unknown): Promise<any> { try { await work(); } catch (error) { return error; } throw Error("Expected actual adapter rejection"); }
const nativeTodo = (id: string, userId: string) => ({ id, user_id: userId, title: "retained", status: "open", deleted_at: null, updated_at: instant });

test("REL-002: actual generated orphan INSERT and HTTP generic conflict produce native safe relationship_conflict without a row", async () => {
  const id = crypto.randomUUID(), missing = crypto.randomUUID();
  const error = await rejected(() => client.create_Todo(nativeTodo(id, missing)));
  expect(error).toBeInstanceOf(PersistenceFault); expect(error.kind).toBe("constraint"); expect(normalizePersistenceFailure(error)).toEqual(expected);
  expect(await rows('SELECT id FROM todo WHERE id=$1', [id])).toEqual([]);
  const response = await app.handleRequest(new Request("https://fixture.test/lifecycle/todos", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ user_id: missing, title: "missing parent", status: "open" }) }));
  expect(response.status).toBe(409); const body = await response.json();
  expect(body.error.code).toBe("relationship_conflict"); expect(body.error.message).toBe(expected.message);
  expect(Object.keys(body.error).sort()).toEqual(["code", "message", "request_id"]);
  for (const secret of [missing, "FOREIGN KEY", "23503", "SQLITE", "todo_user_id_fk", "user_id", "constraint failed"]) expect(JSON.stringify(body)).not.toContain(secret);
  expect(await rows('SELECT id FROM todo')).toEqual([]);
});

test("REL-004: actual restricted owner DELETE normalizes at the native host entry, retaining disabled owner and child without an authored delete route", async () => {
  const owner = crypto.randomUUID(), child = crypto.randomUUID();
  await client.create_User({ id: owner, name: "retained owner", batch_group: "default", status: "active", disabled_at: null, updated_at: instant });
  await client.create_Todo(nativeTodo(child, owner)); await client.transition_required_User_by_id_disable(owner);
  const before = { user: await rows('SELECT * FROM "user" WHERE id=$1', [owner]), child: await rows('SELECT * FROM todo WHERE id=$1', [child]) };
  const error = await rejected(() => rows('DELETE FROM "user" WHERE id=$1', [owner]));
  expect(normalizePersistenceFailure(error)).toEqual(expected);
  // SQLite RESTRICT is native trigger classification1811, not generic787.
  if (!sql) expect(error.errno).toBe(1811);
  expect(await rows('SELECT * FROM "user" WHERE id=$1', [owner])).toEqual(before.user); expect(await rows('SELECT * FROM todo WHERE id=$1', [child])).toEqual(before.child);
  expect(JSON.stringify(normalizePersistenceFailure(error))).toBe(JSON.stringify(expected));
});

test("real unique/check/custom-trigger failures keep generic declared conflict, and unknown COMMIT dominates an underlying FK classification", async () => {
  const owner = crypto.randomUUID(); await client.create_User({ id: owner, name: "control", batch_group: "default", status: "active", disabled_at: null, updated_at: instant });
  const value = nativeTodo(crypto.randomUUID(), owner); await client.create_Todo(value);
  expect(normalizePersistenceFailure(await rejected(() => client.create_Todo(value)))).toBeNull();
  await rows('CREATE TABLE relationship_check_probe (value INTEGER CHECK (value>0))');
  expect(normalizePersistenceFailure(await rejected(() => rows('INSERT INTO relationship_check_probe VALUES (-1)')))).toBeNull();
  if (sql) {
    await rows("CREATE FUNCTION relationship_custom_trigger() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'unrelated trigger rejection' USING ERRCODE='23514'; END $$");
    await rows('CREATE TRIGGER relationship_custom_trigger BEFORE INSERT ON todo FOR EACH ROW EXECUTE FUNCTION relationship_custom_trigger()');
  } else await rows("CREATE TRIGGER relationship_custom_trigger BEFORE INSERT ON todo BEGIN SELECT RAISE(ABORT,'unrelated trigger rejection'); END");
  try {
    const raw = await rejected(() => client.create_Todo(nativeTodo(crypto.randomUUID(), owner))); expect(normalizePersistenceFailure(raw)).toBeNull(); if (!sql) { let native = raw; while (native.cause) native = native.cause; expect(native.errno).toBe(1811); }
    const response = await app.handleRequest(new Request("https://fixture.test/lifecycle/todos", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ user_id: owner, title: "custom trigger", status: "open" }) }));
    expect(response.status).toBe(409); expect((await response.json()).error.code).toBe("todo_conflict");
  } finally { if (sql) { await rows('DROP TRIGGER relationship_custom_trigger ON todo'); await rows('DROP FUNCTION relationship_custom_trigger()'); } else await rows('DROP TRIGGER relationship_custom_trigger'); }
  const actualForeignKey = await rejected(() => client.create_Todo(nativeTodo(crypto.randomUUID(), crypto.randomUUID())));
  const unknown = new PersistenceFault("transaction.commit", "unknown", actualForeignKey, null, { phase: "commit", database: sql ? "postgres" : "sqlite", outcome: "unknown", retryableCause: null, rollbackProven: false, commitAcknowledged: false });
  expect(normalizePersistenceFailure(unknown)).toBeNull(); expect(normalizePersistenceFailure(new OutcomeUnknownFault("transaction.commit", actualForeignKey))).toBeNull();
});

// Exact immutable independent review counterexamples: no cause inspection may
// replace a safe public failure with a second exception.
test("the native public normalizer returns null for all eight unreadable error shapes", () => {
  const revoked = Proxy.revocable({}, {}); revoked.revoke();
  const values: Array<[string, unknown]> = [
    ["code accessor", { get code() { throw Error("unreadable code"); } }],
    ["cause accessor", { code: "23503", get cause() { throw Error("unreadable cause"); } }],
    ["message accessor", { code: "SQLITE_CONSTRAINT_TRIGGER", errno: 1811, get message() { throw Error("unreadable message"); } }],
    ["wrapped cause accessor", new PersistenceFault("test.constraint", "constraint", { code: "23503", get cause() { throw Error("unreadable nested cause"); } })],
    ["get proxy", new Proxy({}, { get() { throw Error("unreadable proxy"); } })],
    ["prototype proxy", new Proxy({}, { getPrototypeOf() { throw Error("unreadable prototype"); } })],
    ["revoked proxy", revoked.proxy],
    ["code conversion", { code: { toString() { throw Error("unreadable conversion"); } } }],
  ];
  for (const [name, value] of values) {
    let result: unknown = "not evaluated";
    expect(() => { result = normalizePersistenceFailure(value); }, name).not.toThrow();
    expect(result, name).toBeNull();
  }
});
