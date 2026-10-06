import { afterAll, expect, test } from "bun:test";
import { SQL } from "bun";
import { Database } from "bun:sqlite";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { copyGoldenFixture } from "./golden-fixture";

// Actual emitted User.disable_user HTTP for selected signed API/JWT forms.
// Fresh imports exercise native conditional admission without rewriting output.
const root = mkdtempSync(join(tmpdir(), "jadpo-selected-lifecycle-"));
const project = join(root, "application"); copyGoldenFixture(project);
const built = Bun.spawnSync([Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo"), "build", project], { stdout: "pipe", stderr: "pipe" });
if (built.exitCode) throw Error(`Canonical fixture build failed: ${built.stderr}`);
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
symlinkSync(join(dependency, "node_modules"), join(project, "build/target/node_modules"), "dir");
if (Bun.env.JADPO_LIFECYCLE_CREDENTIAL_CAPTURE) {
  mkdirSync(Bun.env.JADPO_LIFECYCLE_CREDENTIAL_CAPTURE, { recursive: true });
  cpSync(join(project, "build/target"), join(Bun.env.JADPO_LIFECYCLE_CREDENTIAL_CAPTURE, "target"), { recursive: true, filter: path => !path.includes("/node_modules") });
}
const databaseUrl = Bun.env.JADPO_LIFECYCLE_CREDENTIAL_DATABASE_URL;
const oldDatabase = Bun.env.DATABASE_URL, oldSqlite = Bun.env.SQLITE_PATH;
if (databaseUrl) {
  const url = new URL(databaseUrl);
  if (url.hostname !== "127.0.0.1" || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Disposable local database required");
  Bun.env.DATABASE_URL = databaseUrl;
} else delete Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = join(root, "fixture.sqlite");
const environment = { DATABASE_URL: "https://db.test/golden", SESSION_SIGNING_KEY: Buffer.alloc(32, 91).toString("base64url"), BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: "https://issuer.test", OIDC_AUDIENCE: "todo", MAIL_API_KEY: Buffer.alloc(32, 92).toString("base64url"), MAIL_SENDER: "todo@example.test" };
const applicationPath = join(project, "build/target/app.ts");
const emittedSource = readFileSync(applicationPath, "utf8");
const permission = '__operation = allowOperationRetries(__operation, __authenticatedRetryCandidate && authenticated.kind === "user" && __operation.refreshAuthority !== null);';
expect(emittedSource.split(permission).length - 1).toBe(3);
const app = await import(pathToFileURL(applicationPath).href);
await app.initializeApplication(environment);
// The native HTTP application and selector share their actual closed issuer.
const nativeApplication = app;
const { persistence } = await import(pathToFileURL(join(project, "build/target/persistence.ts")).href);
const sql = databaseUrl ? new SQL({ url: databaseUrl, prepare: false, max: 4 }) : null;
const sqlite = databaseUrl ? null : new Database(Bun.env.SQLITE_PATH, { strict: true });
const rows = async (statement: string, values: any[] = []): Promise<any[]> => sql ? [...await sql.unsafe(statement, values)] : sqlite!.prepare(statement).all(...values);
const instant = (at: number) => sql ? new Date(at).toISOString() : at;
const binding = JSON.parse(readFileSync(join(project, "build/audit/jobs.json"), "utf8")).jobs[0].delivery.binding;
afterAll(async () => { expect(readFileSync(applicationPath, "utf8")).toBe(emittedSource); sqlite?.close(); await sql?.close(); if (oldDatabase === undefined) delete Bun.env.DATABASE_URL; else Bun.env.DATABASE_URL = oldDatabase; if (oldSqlite === undefined) delete Bun.env.SQLITE_PATH; else Bun.env.SQLITE_PATH = oldSqlite; rmSync(root, { recursive: true, force: true }); });

for (const [mode, disposition] of [["signed", "fresh"], ["signed", "revoked"], ["signed", "inactive"], ["jwt", "fresh"], ["jwt", "inactive"]] as const)
test(`selected ${mode} User.disable_user ${disposition} preserves current authority and retained intent`, async () => {
  const owner = crypto.randomUUID(), subject = `selected-${owner}`, at = Date.now();
  await rows('INSERT INTO "user" (id,authentication_subject,email,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [owner, subject, `${owner}@example.test`, "active", instant(at), null]);
  const oldFetch = globalThis.fetch, providerDocuments: string[] = [];
  try {
  let credential: string, sessionId: string | undefined;
  if (mode === "jwt") {
    const { SignJWT, generateKeyPair, exportJWK } = await import(pathToFileURL(join(dependency, "node_modules/jose/dist/webapi/index.js")).href);
    const keys = await generateKeyPair("RS256", { modulusLength: 2048, extractable: true });
    const jwk = { ...await exportJWK(keys.publicKey), kid: "selected-lifecycle", alg: "RS256", use: "sig" };
    const discovery = environment.OIDC_ISSUER + "/.well-known/openid-configuration", jwks = environment.OIDC_ISSUER + "/jwks";
    globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = input instanceof Request ? input.url : String(input);
      if (url !== discovery && url !== jwks) return oldFetch(input, init);
      providerDocuments.push(url);
      if (init?.redirect !== "error" || init?.credentials !== "omit") throw Error("JWT provider policy changed");
      return Response.json(url === discovery ? { issuer: environment.OIDC_ISSUER, jwks_uri: jwks } : { keys: [jwk] });
    }) as typeof fetch;
    const seconds = Math.floor(at / 1_000);
    credential = await new SignJWT({ iss: environment.OIDC_ISSUER, aud: environment.OIDC_AUDIENCE, sub: subject, iat: seconds, exp: seconds + 120 }).setProtectedHeader({ alg: "RS256", kid: "selected-lifecycle", typ: "JWT" }).sign(keys.privateKey);
  } else credential = "";
  await app.initializeApplication(environment); // JWT verifier captures actual fixture document boundary here.

  const host = app.authenticationHost();
  if (mode === "signed") { const issued = await host.issue("api_bearer", subject, at + 3_600_000, at); credential = issued.credential; sessionId = JSON.parse(Buffer.from(credential.split(".")[2], "base64url").toString())[3]; }
  const headers = { authorization: `Bearer ${credential!}`, "content-type": "application/json" };
  const principal = await host.authenticate(new Request("https://todo.test/users/" + owner, { method: "DELETE", headers }), true, Date.now(), true);
  expect(principal).toMatchObject({ kind: "user", subject, authenticationStrength: "primary", values: { user_id: owner } });
  const created = await app.handleRequest(new Request("https://todo.test/todos", { method: "POST", headers, body: JSON.stringify({ title: "retained selected lifecycle", due_at: new Date(at + 3_600_000).toISOString() }) }));
  expect(created.status).toBe(201); const todo = await created.json();
  const service = crypto.randomUUID(), name = `sender-${service}`;
  await rows('INSERT INTO service (id,owner_id,name,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [service, owner, name, "active", instant(at), null]);
  await rows('INSERT INTO reminder_service_membership (id,service_id,role) VALUES ($1,$2,$3)', [crypto.randomUUID(), service, "sender"]);
  const nativeHost = nativeApplication.authenticationHost();
  const key = await nativeHost.issueServiceCredential("api_bearer", name, at + 3_600_000, at), proof = await nativeHost.prepareDeliveryCredential("api_bearer", key.credential, Date.now());
  const page = await persistence.withOperationTime(new Date(at + 7_200_000).toISOString()).transaction((tx: any) => tx.select_delivery_intents(binding, proof));
  const enrolled = page.intents.find((value: any) => value.sourceEntityId === todo.id); expect(enrolled).toBeDefined();
  const retained = async () => ({ todo: await rows('SELECT * FROM todo WHERE id=$1', [todo.id]), schedule: await rows('SELECT * FROM "__jadpo_delivery_schedules_v1" WHERE source_entity_id=$1', [todo.id]), intent: await rows('SELECT * FROM "__jadpo_deliveries_v1" WHERE intent_id=$1', [enrolled.intentId]) });
  const baseline = await retained(), originalIntent = baseline.intent;
  expect(originalIntent).toHaveLength(1);
  expect(originalIntent[0]).toMatchObject({ intent_id: enrolled.intentId, source_entity: "Todo", source_entity_id: todo.id, source_revision: "1", state: "pending", ordering_key: todo.id, payload_version: "reminder.v1" });
  expect(enrolled.sourceRevision).toBe("1");
  expect(JSON.parse(originalIntent[0].payload)).toEqual(enrolled.payload);
  expect(enrolled.payload.idempotency_key).toBe(enrolled.intentId);
  const before = JSON.stringify(baseline), events: any[] = [], previousError = console.error;
  const suffix = crypto.randomUUID().replaceAll("-", ""), sequence = `selected_sequence_${suffix}`, fn = `selected_fn_${suffix}`;
  let locker: Database | null = null, held: any, heldWork: Promise<any> | undefined, changeWork: Promise<void> | undefined, release = () => {}, waiter = false;
  if (sql) {
    await sql.unsafe(`CREATE SEQUENCE "${sequence}"`);
    await sql.unsafe(`CREATE FUNCTION "${fn}"() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF nextval('"${sequence}"')=1 THEN RAISE EXCEPTION USING ERRCODE='40001', MESSAGE='selected lifecycle fixture abort'; END IF; RETURN NEW; END $$`);
    await sql.unsafe(`CREATE TRIGGER "${fn}" BEFORE UPDATE OF status,disabled_at ON "user" FOR EACH ROW EXECUTE FUNCTION "${fn}"()`);
    let ready!: () => void; const acquired = new Promise<void>(resolve => { ready = resolve; }), gate = new Promise<void>(resolve => { release = resolve; });
    heldWork = sql.begin(async (tx: any) => { held = tx; ready(); await gate; }); heldWork.catch(() => {}); await acquired;
  } else { locker = new Database(Bun.env.SQLITE_PATH); locker.exec("BEGIN IMMEDIATE"); }
  console.error = (...args: any[]) => {
    let event: any; try { event = JSON.parse(args[0]); } catch { return; }
    if (event.eventName !== "transaction.attempt" || event.operation !== "User.disable_user") return;
    events.push(event);
    if (event.attempt !== 1 || event.outcome !== "no_commit" || !event.retryScheduled) return;
    locker?.exec("ROLLBACK"); locker?.close(); locker = null;
    changeWork = (async () => {
      const change = (statement: string, values: any[] = []) => sql ? held.unsafe(statement, values) : sqlite!.prepare(statement).all(...values);
      if (sql) {
        // Initial auth:fresh has already admitted the first transaction. Only
        // its proven abort authorizes this external fixture lock/state change.
        await change(mode === "jwt" ? 'LOCK TABLE "user" IN ACCESS EXCLUSIVE MODE' : 'LOCK TABLE "__jadpo_auth_sessions" IN ACCESS EXCLUSIVE MODE');
        const deadline = performance.now() + 500;
        while (performance.now() < deadline) {
          await held.unsafe("SELECT pg_stat_clear_snapshot()");
          const waiting = await held.unsafe("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND state='active' AND wait_event_type='Lock' AND query LIKE $1", [mode === "jwt" ? '%FROM "user"%' : '%FROM%__jadpo_auth_sessions%']);
          if (waiting.length) { waiter = true; break; } await Bun.sleep(5);
        }
        expect(waiter).toBe(true);
      }
      if (disposition === "revoked") await change('DELETE FROM "__jadpo_auth_sessions" WHERE id=$1', [sessionId]);
      if (disposition === "inactive") await change('UPDATE "user" SET status=$1 WHERE id=$2', ["disabled", owner]);
    })().finally(() => release()); changeWork.catch(() => {});
  };
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: request => app.handleRequest(request) });
  try {
    const response = await fetch(`http://127.0.0.1:${server.port}/users/${owner}`, { method: "DELETE", headers });
    await changeWork; await heldWork;
    expect(events[0]).toMatchObject({ attempt: 1, outcome: "no_commit", retryScheduled: true, commitAcknowledged: false, retryableCause: sql ? "postgres_serialization" : "sqlite_busy" });
    if (sql) { expect(events[0].rollbackProven).toBe(true); expect(waiter).toBe(true); }
    expect(response.status).toBe(disposition === "fresh" ? 204 : disposition === "revoked" ? 401 : 403);
    const user = (await rows('SELECT status,disabled_at FROM "user" WHERE id=$1', [owner]))[0];
    expect(JSON.stringify(await retained())).toBe(before);
    if (disposition === "fresh") {
      expect(events).toHaveLength(2); expect(events[1]).toMatchObject({ operationId: events[0].operationId, attempt: 2, outcome: "committed", commitAcknowledged: true });
      expect(user.status).toBe("disabled"); expect(user.disabled_at).not.toBeNull();
      const denied = await fetch(`http://127.0.0.1:${server.port}/users/${owner}`, { method: "DELETE", headers }); expect(denied.status).toBe(403); expect(events).toHaveLength(2);
    } else { expect(events).toHaveLength(1); expect(user.status).toBe(disposition === "inactive" ? "disabled" : "active"); expect(user.disabled_at).toBeNull(); }
    if (mode === "jwt") expect(providerDocuments).toEqual([environment.OIDC_ISSUER + "/.well-known/openid-configuration", environment.OIDC_ISSUER + "/jwks"]);
    console.log(JSON.stringify({ control: `${mode}-${disposition}`, modifiedAdmissionBit: false, emittedConditionalAdmission: true, nativeRetryLookupWait: waiter, status: response.status, retainedIntent: enrolled.intentId, attempts: events, jwtDocumentRequests: providerDocuments.length }));
  } finally {
    console.error = previousError; globalThis.fetch = oldFetch; server.stop(true);
    if (locker) { locker.exec("ROLLBACK"); locker.close(); } release();
    const settled = await Promise.allSettled([changeWork, heldWork]);
    if (sql) { await sql.unsafe(`DROP TRIGGER IF EXISTS "${fn}" ON "user"`); await sql.unsafe(`DROP FUNCTION IF EXISTS "${fn}"()`); await sql.unsafe(`DROP SEQUENCE IF EXISTS "${sequence}"`); }
    const failure = settled.find(value => value.status === "rejected"); if (failure?.status === "rejected") throw failure.reason;
  }
  } finally { globalThis.fetch = oldFetch; }
}, 15_000);
