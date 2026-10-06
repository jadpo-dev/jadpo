import { afterAll, expect, test } from "bun:test";
import { SQL } from "bun";
import { Database } from "bun:sqlite";
import { cpSync, mkdirSync, mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { copyGoldenFixture } from "./golden-fixture";
import { privateRetryApplication } from "./fixtures/protected-retry-application";

// RM402 prerequisite only. All HTTP workers preserve the emitted bodies, native
// crypto/store and current policy. Three inert admission bits are counterfactual;
// no product flag or static repeatability exclusion is changed by this suite.
const root = mkdtempSync(join(tmpdir(), "jadpo-protected-retry-"));
const project = join(root, "application");
copyGoldenFixture(project);
const build = Bun.spawnSync([Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo"), "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode) throw Error(`Checked canonical fixture build failed: ${build.stderr}`);
symlinkSync(resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies", "node_modules"), join(project, "build/target/node_modules"), "dir");
const path = join(project, "build/target/app.ts");
if (Bun.env.JADPO_PROTECTED_CAPTURE) {
  mkdirSync(Bun.env.JADPO_PROTECTED_CAPTURE, { recursive: true });
  cpSync(join(project, "build/target"), join(Bun.env.JADPO_PROTECTED_CAPTURE, "target"), { recursive: true, filter: value => !value.includes("/node_modules") });
  cpSync(join(project, "build/audit"), join(Bun.env.JADPO_PROTECTED_CAPTURE, "audit"), { recursive: true });
}
const databaseUrl = Bun.env.JADPO_PROTECTED_RETRY_DATABASE_URL;
const previousDatabase = Bun.env.DATABASE_URL, previousSqlite = Bun.env.SQLITE_PATH;
if (databaseUrl) {
  const url = new URL(databaseUrl);
  if (url.hostname !== "127.0.0.1" || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Disposable local database required");
  Bun.env.DATABASE_URL = databaseUrl;
} else delete Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = join(root, "fixture.sqlite");
const configuration = {
  DATABASE_URL: "https://db.test/golden", SESSION_SIGNING_KEY: Buffer.alloc(32, 81).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: "https://issuer.test", OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 82).toString("base64url"), MAIL_SENDER: "todo@example.test",
};
const app = await privateRetryApplication(path, true);
await app.initializeApplication(configuration);
const sql = databaseUrl ? new SQL({ url: databaseUrl, prepare: false, max: 8 }) : null;
const sqlite = databaseUrl ? null : new Database(Bun.env.SQLITE_PATH, { strict: true });
const rows = async (statement: string, values: any[] = []): Promise<any[]> => sql ? [...await sql.unsafe(statement, values)] : sqlite!.prepare(statement).all(...values);
const instant = (at: number) => sql ? new Date(at).toISOString() : at;
const captured: any[] = [];
const error = console.error;
console.error = (...args: any[]) => { try { const event = JSON.parse(args[0]); if (event.eventName === "transaction.attempt") captured.push(event); } catch {} error(...args); };
afterAll(async () => {
  console.error = error;
  sqlite?.close(); await sql?.close();
  if (previousDatabase === undefined) delete Bun.env.DATABASE_URL; else Bun.env.DATABASE_URL = previousDatabase;
  if (previousSqlite === undefined) delete Bun.env.SQLITE_PATH; else Bun.env.SQLITE_PATH = previousSqlite;
  rmSync(root, { recursive: true, force: true });
});
async function source(label: string) {
  const owner = crypto.randomUUID(), at = Date.now(), subject = `owner-${owner}`;
  await rows('INSERT INTO "user" (id,authentication_subject,email,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [owner, subject, `${owner}@example.test`, "active", instant(at), null]);
  const issued = await app.authenticationHost().issue("browser_session", subject, at + 3_600_000, at);
  const headers = { cookie: `todo_session=${issued.credential}`, origin: configuration.BROWSER_ORIGIN, "x-jadpo-csrf": issued.csrfToken, "content-type": "application/json" };
  const response = await app.handleRequest(new Request("https://todo.test/todos", { method: "POST", headers, body: JSON.stringify({ title: label }) }));
  expect(response.status).toBe(201);
  const todo = await response.json();
  return { owner, issued, headers, todo, sessionId: JSON.parse(Buffer.from(issued.credential.split(".")[2], "base64url").toString())[3] };
}
async function snapshot(ids: string[]) {
  return JSON.stringify({
    sources: await rows('SELECT * FROM todo WHERE id IN ($1,$2) ORDER BY id', ids),
    owners: await rows('SELECT * FROM "user" WHERE id IN (SELECT owner_id FROM todo WHERE id IN ($1,$2)) ORDER BY id', ids),
    schedules: await rows('SELECT * FROM "__jadpo_delivery_schedules_v1" WHERE source_entity_id IN ($1,$2) ORDER BY source_entity_id', ids),
  });
}
test("both service validator forms refuse all three protected user mutations before transaction admission", async () => {
  const f = await source("service denial source");
  const service = crypto.randomUUID(), name = `service-${service}`, at = Date.now();
  await rows('INSERT INTO service (id,owner_id,name,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [service, f.owner, name, "active", instant(at), null]);
  const key = await app.authenticationHost().issueServiceCredential("api_bearer", name, at + 3_600_000, at);
  const token = await app.authenticationHost().exchangeServiceCredential("api_bearer", key.credential, at);
  const before = await snapshot([f.todo.id, f.todo.id]);
  for (const credential of [key.credential, token.credential]) {
    const principal = await app.authenticationHost().authenticate(new Request("https://todo.test/todos", { headers: { authorization: `Bearer ${credential}` } }), true, Date.now());
    expect(principal).toMatchObject({ kind: "service", values: { service_id: service } });
    for (const [method, route] of [["PATCH", `/todos/${f.todo.id}`], ["DELETE", `/todos/${f.todo.id}`], ["DELETE", `/users/${f.owner}`]]) {
      captured.length = 0;
      const response = await app.handleRequest(new Request(`https://todo.test${route}`, { method, headers: { authorization: `Bearer ${credential}`, "content-type": "application/json" }, ...(method === "PATCH" ? { body: JSON.stringify({ title: "forbidden service edit" }) } : {}) }));
      expect(response.status).toBe(403);
      expect(captured).toHaveLength(0);
      expect(await snapshot([f.todo.id, f.todo.id])).toBe(before);
    }
  }
});

async function worker(onEvent: (event: any, worker: any) => void) {
  const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", resolve("tests/runtime/fixtures/protected-retry-worker.ts")], {
    env: { ...Bun.env, JADPO_PROTECTED_APP: path, JADPO_PROTECTED_CONFIGURATION: JSON.stringify(configuration) }, stdin: "pipe", stdout: "pipe", stderr: "pipe",
  });
  const result: any = { child, events: [], stderr: "" };
  result.errors = (async () => {
    let pending = "";
    for await (const chunk of child.stderr) {
      const text = new TextDecoder().decode(chunk); result.stderr += text; pending += text;
      let index: number;
      while ((index = pending.indexOf("\n")) >= 0) {
        const line = pending.slice(0, index); pending = pending.slice(index + 1);
        try { const event = JSON.parse(line); if (event.eventName === "transaction.attempt") { result.events.push(event); onEvent(event, result); } } catch {}
      }
    }
  })();
  const reader = child.stdout.getReader(); let output = "";
  const deadline = performance.now() + 5_000;
  try {
    while (!output.includes("\n")) {
      let timeout: ReturnType<typeof setTimeout>;
      const chunk = await Promise.race([reader.read(), new Promise<never>((_, reject) => { timeout = setTimeout(() => reject(Error("Fixture worker initialization watchdog")), Math.max(1, deadline - performance.now())); })]).finally(() => clearTimeout(timeout!));
      if (chunk.done) throw Error(`Worker initialization failed: ${result.stderr}`);
      output += new TextDecoder().decode(chunk.value);
    }
  } catch (failure) {
    child.kill(); await child.exited; reader.releaseLock(); await result.errors; throw failure;
  }
  const ready = JSON.parse(output.trim()); expect(ready).toMatchObject({ ready: true, modifiedAdmissionBit: true, releasedPublicRetry: false });
  result.port = ready.port;
  result.output = (async () => { while (!(await reader.read()).done) {} reader.releaseLock(); })();
  return result;
}
for (const revoke of [false, true]) test.skipIf(!sql)(`actual guarded HTTP PostgreSQL deadlock ${revoke ? "revoked victim refuses retry" : "victim reauthenticates and commits once"}`, async () => {
  const f = await source("deadlock initial a"), g = await source("deadlock initial b");
  const suffix = crypto.randomUUID().replaceAll("-", ""), fn = `protected_deadlock_${suffix}`;
  const workers: any[] = []; let release = () => {}, held: any, heldWork: Promise<any> | undefined, mutation: Promise<void> | undefined, victim: any, waiter = false;
  const before = await snapshot([f.todo.id, g.todo.id]);
  const deadlock = (event: any, candidate: any) => {
    if (event.retryableCause !== "postgres_deadlock" || !event.retryScheduled || victim) return;
    victim = candidate;
    if (!revoke) return;
    mutation = (async () => {
      const deadline = performance.now() + 500;
      while (performance.now() < deadline) {
        await held.unsafe("SELECT pg_stat_clear_snapshot()");
        const waiting = await held.unsafe("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND state='active' AND query LIKE '%FROM%__jadpo_auth_sessions%'");
        if (waiting.length) { waiter = true; break; } await Bun.sleep(5);
      }
      expect(waiter).toBe(true);
      await held.unsafe('DELETE FROM "__jadpo_auth_sessions" WHERE id=$1', [candidate.fixture.sessionId]);
    })().finally(() => release());
    mutation.catch(() => {});
  };
  try {
    workers.push(await worker(deadlock), await worker(deadlock)); workers[0].fixture = f; workers[1].fixture = g;
    // Arm only after both real app imports/initializations have acknowledged.
    await sql!.unsafe(`CREATE FUNCTION "${fn}"() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
      IF NEW.title IN ('deadlock-a','deadlock-b') THEN
        PERFORM set_config('deadlock_timeout','50ms',true);
        IF NEW.title='deadlock-a' THEN PERFORM pg_advisory_xact_lock(982341); PERFORM pg_sleep(0.2); PERFORM pg_advisory_xact_lock(982342);
        ELSE PERFORM pg_advisory_xact_lock(982342); PERFORM pg_sleep(0.2); PERFORM pg_advisory_xact_lock(982341); END IF;
      END IF; RETURN NEW; END $$`);
    await sql!.unsafe(`CREATE TRIGGER "${fn}" BEFORE UPDATE OF title ON todo FOR EACH ROW EXECUTE FUNCTION "${fn}"()`);
    if (revoke) {
      let ready!: () => void; const acquired = new Promise<void>(resolve => { ready = resolve; }), gate = new Promise<void>(resolve => { release = resolve; });
      heldWork = sql!.begin(async (tx: any) => { await tx.unsafe('LOCK TABLE "__jadpo_auth_sessions" IN ACCESS EXCLUSIVE MODE'); held = tx; ready(); await gate; });
      heldWork.catch(() => {}); await acquired;
    }
    const due = new Date(Date.now() + 3_600_000).toISOString();
    const responses = await Promise.all(workers.map((w, index) => fetch(`http://127.0.0.1:${w.port}/todos/${w.fixture.todo.id}`, { method: "PATCH", headers: w.fixture.headers, body: JSON.stringify({ title: index === 0 ? "deadlock-a" : "deadlock-b", due_at: due }) })));
    await mutation; await heldWork;
    expect(victim).toBeDefined();
    const first = victim.events.find((event: any) => event.retryableCause === "postgres_deadlock");
    expect(first).toMatchObject({ attempt: 1, outcome: "no_commit", rollbackProven: true, commitAcknowledged: false, retryScheduled: true });
    const index = workers.indexOf(victim);
    expect(responses[index].status).toBe(revoke ? 401 : 200); expect(responses[1-index].status).toBe(200);
    expect(workers[1-index].events).toHaveLength(1);
    expect(workers[1-index].events[0]).toMatchObject({ attempt: 1, outcome: "committed", commitAcknowledged: true });
    if (revoke) { expect(waiter).toBe(true); expect(victim.events).toHaveLength(1); }
    else { expect(victim.events).toHaveLength(2); expect(victim.events[1]).toMatchObject({ operationId: first.operationId, attempt: 2, outcome: "committed", commitAcknowledged: true }); }
    for (let i = 0; i < 2; i++) {
      const row = (await rows('SELECT title,owner_id,deleted_at FROM todo WHERE id=$1', [workers[i].fixture.todo.id]))[0];
      expect(row.owner_id).toBe(workers[i].fixture.owner); expect(row.deleted_at).toBeNull();
      expect(row.title).toBe(revoke && i === index ? workers[i].fixture.todo.title : i === 0 ? "deadlock-a" : "deadlock-b");
      const revision = (await rows('SELECT schedule_revision FROM "__jadpo_delivery_schedules_v1" WHERE source_entity_id=$1', [workers[i].fixture.todo.id]))[0];
      expect(Number(revision.schedule_revision)).toBe(revoke && i === index ? 1 : 2);
    }
    expect(await snapshot([f.todo.id, g.todo.id])).not.toBe(before);
    console.log(JSON.stringify({ control: revoke ? "deadlock-revocation" : "deadlock-fresh", modifiedAdmissionBit: true, releasedPublicRetry: false, nativeFreshCredentialLockWait: waiter, statuses: responses.map(value => value.status), attempts: workers.map(value => value.events) }));
  } finally {
    release(); await heldWork?.catch(() => {}); await mutation?.catch(() => {});
    for (const w of workers) { w.child.stdin.end(); const exited = await Promise.race([w.child.exited, Bun.sleep(1_000).then(() => null)]); if (exited === null) { w.child.kill(); await w.child.exited; } await w.errors; await w.output; }
    await sql!.unsafe(`DROP TRIGGER IF EXISTS "${fn}" ON todo`); await sql!.unsafe(`DROP FUNCTION IF EXISTS "${fn}"()`);
  }
}, 15_000);
