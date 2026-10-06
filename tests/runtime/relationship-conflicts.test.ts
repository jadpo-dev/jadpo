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
const inventory = JSON.parse(readFileSync(join(project, "build/inventory/routes.json"), "utf8"));
const failureAudit = JSON.parse(readFileSync(join(project, "build/audit/failures.json"), "utf8"));
const openapi = JSON.parse(readFileSync(join(project, "build/openapi/openapi.json"), "utf8"));
afterAll(async () => { if (sql) await sql.close(); sqlite?.close(); delete Bun.env.DATABASE_URL; delete Bun.env.SQLITE_PATH; delete Bun.env.JADPO_LIFECYCLE_RETENTION; rmSync(root, { recursive: true, force: true }); });
async function rejected(work: () => Promise<unknown> | unknown): Promise<any> { try { await work(); } catch (error) { return error; } throw Error("Expected actual adapter rejection"); }
const nativeTodo = (id: string, userId: string) => ({ id, user_id: userId, title: "retained", status: "open", deleted_at: null, updated_at: instant });

test("REL-002: actual generated orphan INSERT and HTTP generic conflict produce native safe relationship_conflict without a row", async () => {
  const metadata = inventory.routes.find((route: any) => route.route === "POST /lifecycle/todos");
  expect(metadata.relationship_failure_analysis).toMatchObject({ qualified: true, may_fail: true, status: "checked_static_may_failure" });
  const descriptor = metadata.operational_failures.find((failure: any) => failure.code === expected.code);
  expect(descriptor).toMatchObject({ code: expected.code, http_status: expected.status, message: expected.message, automatic_retry: false });
  expect(failureAudit.operational_failures.find((failure: any) => failure.code === expected.code)).toEqual(descriptor);
  const id = crypto.randomUUID(), missing = crypto.randomUUID();
  const error = await rejected(() => client.create_Todo(nativeTodo(id, missing)));
  expect(error).toBeInstanceOf(PersistenceFault); expect(error.kind).toBe("constraint"); expect(normalizePersistenceFailure(error)).toEqual(expected);
  expect(await rows('SELECT id FROM todo WHERE id=$1', [id])).toEqual([]);
  const response = await app.handleRequest(new Request("https://fixture.test/lifecycle/todos", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ user_id: missing, title: "missing parent", status: "open" }) }));
  expect(response.status).toBe(409); const body = await response.json();
  expect(body.error.code).toBe("relationship_conflict"); expect(body.error.message).toBe(expected.message);
  expect(response.status).toBe(descriptor.http_status); expect(body.error.code).toBe(descriptor.code); expect(body.error.message).toBe(descriptor.message);
  const alternatives = openapi.paths["/lifecycle/todos"].post.responses["409"].content["application/json"].schema.oneOf;
  const operationalSchema = alternatives.find((schema: any) => schema.properties?.error?.properties?.code?.enum?.includes(body.error.code));
  expect(operationalSchema.additionalProperties).toBe(false); expect(operationalSchema.properties.error.additionalProperties).toBe(false);
  expect(operationalSchema.properties.error.required.sort()).toEqual(Object.keys(body.error).sort());
  expect(alternatives.some((schema: any) => schema.$ref === "#/components/schemas/TodoConflict")).toBe(true);
  expect(Object.keys(body.error).sort()).toEqual(["code", "message", "request_id"]);
  for (const secret of [missing, "FOREIGN KEY", "23503", "SQLITE", "todo_user_id_fk", "user_id", "constraint failed"]) expect(JSON.stringify(body)).not.toContain(secret);
  expect(await rows('SELECT id FROM todo')).toEqual([]);
});

test("actual lifecycle soft-delete UPDATE, unrelated rename and query metadata do not invent relationship failures", () => {
  for (const path of ["/lifecycle/users", "/lifecycle/reminders", "/lifecycle/todos/{todo_id}/delete", "/lifecycle/todos/{todo_id}/rename"]) {
    const route = inventory.routes.find((route: any) => route.route === `POST ${path}`);
    expect(route.relationship_failure_analysis).toMatchObject({ qualified: true, may_fail: false, status: "checked_not_reachable" });
    expect(route.operational_failures.some((failure: any) => failure.code === expected.code)).toBe(false);
    const schema = openapi.paths[path].post.responses["409"]?.content?.["application/json"]?.schema;
    expect(JSON.stringify(schema ?? null)).not.toContain(expected.code);
  }
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


// A private checked fixture qualifies the two SQL UPDATE endpoints, including
// the implicit NO ACTION rule on the referenced unique key. No golden or host
// authority is added by these unauthenticated test-only declarations.
test("actual outgoing and incoming key UPDATE failures match route metadata and retain both rows", async () => {
  const directory = join(root, "update-endpoints"); mkdirSync(directory);
  writeFileSync(join(directory, "app.jadpo"), `
entity MetadataParent { id: Uuid identity key: Text unique note: Text }
entity MetadataChild { id: Uuid identity parent_key: MetadataParent.key references MetadataParent.key on_delete restrict }
input ParentChange { id: MetadataParent.id key: MetadataParent.key note: MetadataParent.note }
input ChildChange { id: MetadataChild.id key: MetadataParent.key }
failure Missing {kind:NotFound code:"missing"}
failure MutationRejected {kind:Conflict code:"conflict"}
action change_parent(input:ParentChange) ->MetadataParent fails Missing,MutationRejected {
 return attempt update required MetadataParent {where:id==input.id set:{key:input.key} missing:Missing conflict:MutationRejected}
}
action change_child(input:ChildChange) ->MetadataChild fails Missing,MutationRejected {
 return attempt update required MetadataChild {where:id==input.id set:{parent_key:MetadataChild.parent_key(input.key)} missing:Missing conflict:MutationRejected}
}
action rename_parent(input:ParentChange) ->MetadataParent fails Missing,MutationRejected {
 return attempt update required MetadataParent {where:id==input.id set:{note:input.note} missing:Missing conflict:MutationRejected}
}
route POST /parent-key {auth:none input:ParentChange output:MetadataParent run:change_parent(input)}
route POST /child-key {auth:none input:ChildChange output:MetadataChild run:change_child(input)}
route POST /parent-note {auth:none input:ParentChange output:MetadataParent run:rename_parent(input)}
`);
  const built = Bun.spawnSync([compiler,"build",directory],{stdout:"pipe",stderr:"pipe"});
  expect(built.exitCode,`${built.stdout}\n${built.stderr}`).toBe(0);
  const target = await import(pathToFileURL(join(directory,"build/target/app.ts")).href);
  const persistent = await import(pathToFileURL(join(directory,"build/target/persistence.ts")).href);
  const routes = JSON.parse(readFileSync(join(directory,"build/inventory/routes.json"),"utf8")).routes;
  const owner = crypto.randomUUID(), child = crypto.randomUUID(), key = crypto.randomUUID();
  await persistent.persistence.create_MetadataParent({id:owner,key,note:"original"});
  await persistent.persistence.create_MetadataChild({id:child,parent_key:key});
  const before = {parent:await rows('SELECT * FROM metadata_parent WHERE id=$1',[owner]),child:await rows('SELECT * FROM metadata_child WHERE id=$1',[child])};
  for (const [path,input] of [["/parent-key",{id:owner,key:crypto.randomUUID(),note:"original"}],["/child-key",{id:child,key:crypto.randomUUID()}]] as const) {
    const metadata = routes.find((route:any)=>route.route===`POST ${path}`);
    expect(metadata.relationship_failure_analysis).toMatchObject({qualified:true,may_fail:true});
    const response = await target.handleRequest(new Request(`https://fixture.test${path}`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(input)}));
    const body = await response.json(); expect(response.status).toBe(409);expect(body.error.code).toBe(expected.code);expect(body.error.message).toBe(expected.message);
    expect(JSON.stringify(body)).not.toContain(key);expect(JSON.stringify(body)).not.toContain("FOREIGN KEY");
    expect(await rows('SELECT * FROM metadata_parent WHERE id=$1',[owner])).toEqual(before.parent);
    expect(await rows('SELECT * FROM metadata_child WHERE id=$1',[child])).toEqual(before.child);
  }
  const metadata = routes.find((route:any)=>route.route==="POST /parent-note");
  expect(metadata.relationship_failure_analysis).toMatchObject({qualified:true,may_fail:false});
  const response = await target.handleRequest(new Request("https://fixture.test/parent-note",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({id:owner,key,note:"changed"})}));
  expect(response.status).toBe(200);expect((await response.json()).note).toBe("changed");
  expect(await rows('SELECT * FROM metadata_child WHERE id=$1',[child])).toEqual(before.child);
});
