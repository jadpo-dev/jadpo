import { afterAll, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

// Component evidence only: the production renderer consumes the complete
// checked fixture and opaque binding, while public full-target lowering still
// refuses its job. No generated code is rewritten; invocation components use a
// disposable loopback reference provider, never a real mail account or scheduler.
const postgresUrl = Bun.env.JADPO_DELIVERY_HOOK_DATABASE_URL;
if (postgresUrl) {
  const url = new URL(postgresUrl);
  if (!["localhost", "127.0.0.1"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw new Error("Requires a disposable local delivery component database");
}
delete Bun.env.DATABASE_URL;
delete Bun.env.SQLITE_PATH;
const root = mkdtempSync(join(tmpdir(), "jadpo-delivery-hook-components-"));
const repository = new URL("../../", import.meta.url).pathname;
const compiler = Bun.spawn(["cargo", "test", "--manifest-path", join(repository, "jadpo/Cargo.toml"), "-p", "jadpo-core", "--lib", "target::tests::delivery_schedule_hook_components_keep_full_execution_disabled", "--", "--exact"], {
  cwd: repository, env: { ...Bun.env, CARGO_INCREMENTAL: "0", JADPO_DELIVERY_HOOK_COMPONENT_OUTPUT: root }, stdout: "pipe", stderr: "pipe",
});
const [compileOut, compileErr, compileExit] = await Promise.all([new Response(compiler.stdout).text(), new Response(compiler.stderr).text(), compiler.exited]);
if (compileExit !== 0 || !compileOut.includes("1 passed")) throw new Error(`component generation failed: ${compileOut}\n${compileErr}`);
const sqlitePath = join(root, "application.sqlite");
if (postgresUrl) Bun.env.DATABASE_URL = postgresUrl;
else Bun.env.SQLITE_PATH = sqlitePath;
const persistencePath = join(root, "persistence.ts");
const dependency = join(repository, "build/validation/jwt-dependencies/node_modules");
if (!existsSync(join(dependency, "jose/package.json"))) throw new Error("Install the pinned JWT dependency before running native authentication components");
symlinkSync(dependency, join(root, "node_modules"), "dir");
const { persistence } = await import(pathToFileURL(persistencePath).href) as { persistence: any };
const { persistence: ordinary } = await import(pathToFileURL(join(root, "ordinary-persistence.ts")).href) as { persistence: any };
const app = await import(pathToFileURL(join(root, "app.ts")).href);
const { authenticationStorage, resolveAuthenticationAuthority } = await import(pathToFileURL(persistencePath).href);
const { createFirstPartyAuthentication } = await import(pathToFileURL(join(root, "first-party-authentication.ts")).href);
const { setReferenceMailEndpointForTesting } = await import(pathToFileURL(join(root, "service-adapter.ts")).href);
const sql = postgresUrl ? new SQL(postgresUrl, { max: 3, prepare: false }) : null;
const sqlite = sql === null ? new Database(sqlitePath) : null;
afterAll(async () => { if (sql) await sql.close(); if (sqlite) sqlite.close(); rmSync(root, { recursive: true, force: true }); });
const instant = "2026-10-05T08:00:00.000Z";
const representationVariant = Bun.env.JADPO_DELIVERY_HOOK_COMPONENT_VARIANT === "completion-representation";
const changeField = representationVariant ? "modified_at" : "updated_at";
const dueA = "2026-10-06T08:00:00.000Z", dueB = "2026-10-07T08:00:00.000Z";
const owner = crypto.randomUUID();
// A test-owned user principal exercises unchanged row policy; it is not worker
// credential/authentication evidence or a generated reminder authority grant.
const client = persistence.withOperationTime(instant).withPolicy({ kind: "user", subject: owner, values: { user_id: owner } }, "Todo.patch_todo");
const method = "update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at";
const rows = async (statement: string, values: any[] = []) => sql ? await sql.unsafe(statement, values) : sqlite!.prepare(statement).all(...values);
const revision = (id: string) => client.transaction((tx: any) => tx.read_delivery_schedule_revision("Todo", id));
const patch = (id: string, value: Record<string, unknown>) => client[method](id, value, null);
const stored = (id: string) => rows('SELECT title, due_at, reminder_sent_at FROM "todo" WHERE id = $1', [id]);
const setSent = (id: string) => rows('UPDATE "todo" SET reminder_sent_at = $1 WHERE id = $2', [sql ? instant : Date.parse(instant), id]);
const todo = (id = crypto.randomUUID()) => ({ id, owner_id: owner, title: "component reminder", status: "open", due_at: dueA, reminder_sent_at: null, created_at: instant, [changeField]: instant, deleted_at: null });
const stamps = (id: string) => rows(`SELECT created_at,"${changeField}" AS updated_at FROM todo WHERE id=$1`, [id]);
// Raw fixture seed: authored User creation is deliberately not a granted API.
await rows('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [owner, owner, `${owner}@example.invalid`, "active", sql ? instant : Date.parse(instant), null]);

const bindingIdentity = JSON.parse(readFileSync(join(root, "binding.json"), "utf8")).binding;
const authEnvironment = {
  DATABASE_URL: "https://db.example.invalid/component",
  SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"), BROWSER_ORIGIN: "https://todo.example.invalid",
  OIDC_ISSUER: "https://issuer.example.invalid", OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"), MAIL_SENDER: "todo@example.invalid",
};
const dbInstant = (milliseconds: number) => sql ? new Date(milliseconds).toISOString() : milliseconds;
async function authorityFixture() {
  await app.initializeApplication(authEnvironment);
  const service = crypto.randomUUID(), membership = crypto.randomUUID(), subject = `component-${service}`;
  const now = Date.now();
  await rows('INSERT INTO "service" (id, owner_id, name, status, created_at, disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [service, owner, subject, "active", dbInstant(now), null]);
  await rows('INSERT INTO "reminder_service_membership" (id, service_id, role) VALUES ($1,$2,$3)', [membership, service, "sender"]);
  const host = app.authenticationHost();
  const issued = await host.issueServiceCredential("api_bearer", subject, now + 3_600_000, now);
  const proof = await host.prepareDeliveryCredential("api_bearer", issued.credential, now);
  return { host, issued, proof, service, membership, subject, now };
}
const authority = (proof: unknown, binding = bindingIdentity) => persistence.transaction((tx: any) => tx.check_delivery_authority(binding, proof));
const scanTime = "2026-10-08T08:00:00.000Z";
const scan = (proof: unknown, after: unknown = null, at = scanTime) => persistence.withOperationTime(at).transaction((tx: any) => tx.select_delivery_intents(bindingIdentity, proof, after));
const clearScanTodos = () => rows('DELETE FROM "todo"'); // This process's disposable component fixtures only.
const intentRows = (id: string) => rows('SELECT intent_id, payload FROM "__jadpo_deliveries_v1" WHERE source_entity_id=$1', [id]);
async function unpublishedHost() {
  // Real configured crypto, exact shared declared storage and actual generated
  // authority queries. Its only missing fact is publication by the app.
  return createFirstPartyAuthentication([{ name: "api_bearer", principal: "service", mode: "api_key", cookie: null,
    audience: "todo-service-key", origin: null, secret: authEnvironment.SESSION_SIGNING_KEY, maximumDelayMs: 300_000 }],
    authenticationStorage, async (strategy: string, subject: string) => {
      const actual = await resolveAuthenticationAuthority(strategy, subject, "service");
      if (actual.length !== 1 || actual[0].status !== "active") return { kind: "missing" };
      return { kind: "active", principal: { kind: "service", subject: actual[0].name, authenticationStrength: "primary", values: { service_id: actual[0].id } } };
    }, undefined, () => {});
}

describe(`checked worker admission authority component (${sql ? "postgres" : "sqlite"})`, () => {
  test("actual generated authentication crypto and declared storage mint an empty opaque proof, not an ordinary policy grant", async () => {
    const f = await authorityFixture();
    expect(Object.isFrozen(f.proof)).toBe(true);
    expect(Reflect.ownKeys(f.proof)).toEqual([]);
    expect(JSON.stringify(f.proof)).toBe("{}");
    expect(await authority(f.proof)).toBe(true);
    expect(await authority(f.proof, "unselected-binding")).toBe(false);
    await expect(persistence.check_delivery_authority(bindingIdentity, f.proof)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    let expired: any;
    await persistence.transaction(async (tx: any) => { expired = tx; expect(await tx.check_delivery_authority(bindingIdentity, f.proof)).toBe(true); });
    await expect(expired.check_delivery_authority(bindingIdentity, f.proof)).rejects.toMatchObject({ operation: "transaction.inactive" });
    const ordinaryService = persistence.withOperationTime(instant).withPolicy({ kind: "service", subject: f.subject, values: { service_id: f.service } }, "Todo.patch_todo");
    expect(await ordinaryService[method](crypto.randomUUID(), { due_at: dueB }, null)).toBeNull();
  });
  test("service-shaped, copied, serialized and foreign-storage native proofs cannot establish authority", async () => {
    const f = await authorityFixture();
    const principal = { kind: "service", subject: f.subject, authenticationStrength: "primary", values: { service_id: f.service } };
    for (const forged of [principal, {}, { ...f.proof }, JSON.parse(JSON.stringify(f.proof)), Object.create(f.proof), null]) {
      await expect(authority(forged)).rejects.toMatchObject({ code: "invalid_credentials" });
    }
    // The same actual key/records through a different host storage object still
    // cannot become this generated module's worker provenance.
    const foreign = await createFirstPartyAuthentication([{ name: "api_bearer", principal: "service", mode: "api_key", cookie: null,
      audience: "todo-service-key", origin: null, secret: authEnvironment.SESSION_SIGNING_KEY, maximumDelayMs: 300_000 }],
      { ...authenticationStorage }, async () => ({ kind: "active", principal }), undefined, () => {});
    const foreignProof = await foreign.prepareDeliveryCredential("api_bearer", f.issued.credential, f.now);
    await expect(authority(foreignProof)).rejects.toMatchObject({ code: "invalid_credentials" });
  });
  test("invalid key and signed service exchange cannot substitute for the selected direct api-key validator", async () => {
    const f = await authorityFixture();
    const changed = f.issued.credential.slice(0, -1) + (f.issued.credential.endsWith("A") ? "B" : "A");
    await expect(f.host.prepareDeliveryCredential("api_bearer", changed, f.now)).rejects.toMatchObject({ code: "invalid_credentials" });
    const signed = await f.host.exchangeServiceCredential("api_bearer", f.issued.credential, f.now);
    await expect(f.host.prepareDeliveryCredential("api_bearer", signed.credential, f.now)).rejects.toMatchObject({ code: "invalid_credentials" });
  });
  test("live revoked/status/expired credential states invalidate previously verified proof using database time", async () => {
    for (const [column, value] of [["revoked_at", dbInstant(Date.now())], ["status", "revoked"], ["expires_at", dbInstant(Date.now() - 10_000)]] as const) {
      const f = await authorityFixture();
      await rows(`UPDATE "service_credential" SET "${column}"=$1 WHERE id=$2`, [value, f.issued.credentialId]);
      expect(await authority(f.proof)).toBe(false);
    }
    const f = await authorityFixture();
    await rows('UPDATE "service_credential" SET expires_at=$1 WHERE id=$2', [dbInstant(f.now - 1_000), f.issued.credentialId]);
    // Even a caller-supplied old verification time cannot make admission use it.
    const oldTimeProof = await f.host.prepareDeliveryCredential("api_bearer", f.issued.credential, f.now - 2_000);
    expect(await authority(oldTimeProof)).toBe(false);
  });
  test("live service disable, rename, credential reassignment, membership deletion and duplicate membership refuse admission", async () => {
    for (const mutation of ["disabled", "renamed", "credential-principal", "membership-deleted", "membership-other-service", "membership-duplicate", "membership-wrong-role"] as const) {
      const f = await authorityFixture();
      if (mutation === "disabled") await rows('UPDATE service SET status=$1 WHERE id=$2', ["disabled", f.service]);
      if (mutation === "renamed") await rows('UPDATE service SET name=$1 WHERE id=$2', ["renamed-" + f.service, f.service]);
      if (mutation === "membership-deleted") await rows('DELETE FROM reminder_service_membership WHERE id=$1', [f.membership]);
      if (mutation === "membership-duplicate") await rows('INSERT INTO reminder_service_membership (id,service_id,role) VALUES ($1,$2,$3)', [crypto.randomUUID(), f.service, "sender"]);
      if (mutation === "membership-wrong-role") await rows('UPDATE reminder_service_membership SET role=$1 WHERE id=$2', ["observer", f.membership]);
      if (mutation === "credential-principal" || mutation === "membership-other-service") {
        const other = crypto.randomUUID();
        await rows('INSERT INTO service (id,owner_id,name,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)', [other, owner, "other-" + other, "active", dbInstant(f.now), null]);
        if (mutation === "credential-principal") await rows('UPDATE service_credential SET service_id=$1 WHERE id=$2', [other, f.issued.credentialId]);
        else await rows('UPDATE reminder_service_membership SET service_id=$1 WHERE id=$2', [other, f.membership]);
      }
      expect(await authority(f.proof)).toBe(false);
    }
  });
  test("caught native authority SQL failure rolls back its savepoint, not surrounding successful source work", async () => {
    const f = await authorityFixture(), value = todo();
    // Test-owned disposable schema fault, not a synthetic JavaScript throw.
    // Restore the same table/records after the owning transaction ends.
    await rows('ALTER TABLE "__jadpo_auth_service_credentials" RENAME COLUMN data TO data_fault_probe');
    try {
      await client.transaction(async (tx: any) => {
        await tx.create_Todo(value);
        await expect(tx.check_delivery_authority(bindingIdentity, f.proof)).rejects.toMatchObject({ operation: "delivery.authority" });
        await tx.update_required_Todo_by_id_set_title(value.id, "source retained after native authority failure");
      });
    } finally { await rows('ALTER TABLE "__jadpo_auth_service_credentials" RENAME COLUMN data_fault_probe TO data'); }
    expect((await stored(value.id))[0].title).toBe("source retained after native authority failure");
    expect(await revision(value.id)).toBe("1");
    expect(await authority(f.proof)).toBe(true);
  });
  test("live verifier replacement and private metadata tamper cannot reuse old crypto provenance", async () => {
    const f = await authorityFixture();
    await rows('UPDATE service_credential SET verifier=$1 WHERE id=$2', [Buffer.alloc(32, 91).toString("base64url"), f.issued.credentialId]);
    expect(await authority(f.proof)).toBe(false);
    for (const field of ["keyId", "subject", "serviceId", "strategy", "binding", "verifierDigest", "extra"]) {
      const g = await authorityFixture();
      const record = JSON.parse((await rows('SELECT data FROM "__jadpo_auth_service_credentials" WHERE id=$1', [g.issued.credentialId]))[0].data);
      record[field] = "tampered";
      await rows('UPDATE "__jadpo_auth_service_credentials" SET data=$1 WHERE id=$2', [JSON.stringify(record), g.issued.credentialId]);
      expect(await authority(g.proof)).toBe(false);
    }
  });
  test("configuration reinitialization invalidates pending old proofs and a fresh current host still verifies", async () => {
    const f = await authorityFixture();
    await app.initializeApplication(authEnvironment);
    await expect(authority(f.proof)).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(f.host.prepareDeliveryCredential("api_bearer", f.issued.credential, f.now)).rejects.toMatchObject({ code: "invalid_credentials" });
    const current = await app.authenticationHost().prepareDeliveryCredential("api_bearer", f.issued.credential, f.now);
    expect(await authority(current)).toBe(true);
    await expect(app.initializeApplication({ ...authEnvironment, SESSION_SIGNING_KEY: "bad" })).rejects.toBeDefined();
    await expect(authority(current)).rejects.toMatchObject({ code: "invalid_credentials" });
  });
  test("an unpublished native issuer on the exact real storage cannot authorize before or after failure and key rotation", async () => {
    const f = await authorityFixture(), other = await unpublishedHost();
    const otherProof = await other.prepareDeliveryCredential("api_bearer", f.issued.credential, f.now);
    expect(await authority(f.proof)).toBe(true);
    await expect(authority(otherProof)).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(app.initializeApplication({ ...authEnvironment, SESSION_SIGNING_KEY: "bad" })).rejects.toBeDefined();
    expect(() => app.authenticationHost()).toThrow();
    await expect(authority(f.proof)).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(authority(otherProof)).rejects.toMatchObject({ code: "invalid_credentials" });
    const freshOther = await other.prepareDeliveryCredential("api_bearer", f.issued.credential, Date.now());
    await expect(authority(freshOther)).rejects.toMatchObject({ code: "invalid_credentials" });
    await app.initializeApplication({ ...authEnvironment, SESSION_SIGNING_KEY: Buffer.alloc(32, 81).toString("base64url") });
    await expect(app.authenticationHost().prepareDeliveryCredential("api_bearer", f.issued.credential, Date.now())).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(authority(otherProof)).rejects.toMatchObject({ code: "invalid_credentials" });
    const issued = await app.authenticationHost().issueServiceCredential("api_bearer", f.subject, Date.now() + 3_600_000, Date.now());
    const current = await app.authenticationHost().prepareDeliveryCredential("api_bearer", issued.credential, Date.now());
    expect(await authority(current)).toBe(true);
  });
  test("reconfiguration during an awaited native metadata digest withdraws the selected issuer", async () => {
    const f = await authorityFixture();
    const original = crypto.subtle.digest;
    let entered!: () => void, release!: () => void;
    const waiting = new Promise<void>(resolve => { entered = resolve; });
    const resume = new Promise<void>(resolve => { release = resolve; });
    // Match the actual stored verifier, not credential HMAC input. Only the
    // admission's genuine metadata digest is held; native results stay intact.
    const storedVerifier = (await rows('SELECT verifier FROM service_credential WHERE id=$1', [f.issued.credentialId]))[0].verifier;
    let held = false;
    crypto.subtle.digest = async function (algorithm: any, input: any) {
      if (!held && new TextDecoder().decode(input) === storedVerifier) { held = true; entered(); await resume; }
      return Reflect.apply(original, this, [algorithm, input]);
    } as any;
    const reading = authority(f.proof);
    try {
      await waiting;
      await app.initializeApplication(authEnvironment);
      release();
      await expect(reading).rejects.toMatchObject({ code: "invalid_credentials" });
    } finally { release(); crypto.subtle.digest = original; await reading.catch(() => {}); }
    expect(held).toBe(true);
    expect(await authority(await app.authenticationHost().prepareDeliveryCredential("api_bearer", f.issued.credential, Date.now()))).toBe(true);
  });
  test("a delayed stale authentication initialization cannot publish over a newer successful or failed generation", async () => {
    for (const success of [true, false]) {
      const f = await authorityFixture(), original = crypto.subtle.importKey;
      let entered!: () => void, release!: () => void, held = false;
      const waiting = new Promise<void>(resolve => { entered = resolve; });
      const resume = new Promise<void>(resolve => { release = resolve; });
      crypto.subtle.importKey = async function (...arguments_: any[]) {
        const data = arguments_[1];
        if (!held && arguments_[0] === "raw" && data instanceof Uint8Array && data.length === 32 && data.every(value => value === 71)) {
          held = true; entered(); await resume;
        }
        return Reflect.apply(original, this, arguments_);
      } as any;
      const stale = app.initializeApplication(authEnvironment);
      try {
        await waiting;
        await expect(authority(f.proof)).rejects.toMatchObject({ code: "invalid_credentials" });
        if (success) {
          await app.initializeApplication(authEnvironment);
          const selected = app.authenticationHost();
          release();
          await expect(stale).rejects.toMatchObject({ code: "authentication_misconfigured" });
          expect(app.authenticationHost()).toBe(selected);
          expect(await authority(await selected.prepareDeliveryCredential("api_bearer", f.issued.credential, Date.now()))).toBe(true);
        } else {
          await expect(app.initializeApplication({ ...authEnvironment, SESSION_SIGNING_KEY: "bad" })).rejects.toBeDefined();
          release();
          await expect(stale).rejects.toMatchObject({ code: "authentication_misconfigured" });
          expect(() => app.authenticationHost()).toThrow();
          await expect(authority(f.proof)).rejects.toMatchObject({ code: "invalid_credentials" });
        }
      } finally { release(); crypto.subtle.importKey = original; await stale.catch(() => {}); }
      expect(held).toBe(true);
    }
  });
  test(sql ? "credential expiry during a PostgreSQL row-lock wait is checked after admission locks" : "credential expiry during a SQLite query delay is checked after the authority read", async () => {
    const f = await authorityFixture();
    await rows('UPDATE service_credential SET expires_at=$1 WHERE id=$2', [dbInstant(Date.now() + (sql ? 800 : 150)), f.issued.credentialId]);
    if (sql) {
      let reading: Promise<boolean> | undefined;
      try {
        await sql.begin(async (holder: any) => {
          await holder.unsafe('SELECT id FROM service WHERE id=$1 FOR UPDATE', [f.service]);
          reading = authority(f.proof);
          // Own test database wait; no execution-profile default or provider I/O.
          await holder.unsafe("SELECT pg_sleep(1.0)");
        });
        expect(await reading).toBe(false);
      } finally { if (reading) await reading; }
    } else {
      const original = Database.prototype.prepare;
      Database.prototype.prepare = function (statement: any, ...arguments_: any[]) {
        const prepared = Reflect.apply(original, this, [statement, ...arguments_]);
        if (String(statement).startsWith('SELECT k."data" AS metadata')) {
          const all = prepared.all;
          prepared.all = function (...values: any[]) {
            const result = Reflect.apply(all, this, values);
            Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
            return result;
          };
        }
        return prepared;
      } as any;
      try { expect(await authority(f.proof)).toBe(false); }
      finally { Database.prototype.prepare = original; }
    }
  });
  test(sql ? "PostgreSQL live authority row locks survive savepoint release until the owning transaction commits" : "SQLite test-owned mutation in the owning transaction is observed and rollback restores authority", async () => {
    const f = await authorityFixture();
    if (sql) {
      const marker = `authority-${crypto.randomUUID()}`;
      let mutation: Promise<unknown> | undefined, finished = false;
      try {
        await persistence.transaction(async (tx: any) => {
          expect(await tx.check_delivery_authority(bindingIdentity, f.proof)).toBe(true);
          mutation = Promise.resolve(sql.unsafe(`UPDATE service SET status=$1 WHERE id=$2 /* ${marker} */`, ["disabled", f.service])).then(value => { finished = true; return value; });
          let waiting = false;
          for (let attempt = 0; attempt < 30 && !waiting; attempt++) {
            const activity = await sql.unsafe("SELECT wait_event_type FROM pg_stat_activity WHERE query LIKE $1 AND pid <> pg_backend_pid()", [`%${marker}%`]);
            waiting = activity.some((row: any) => row.wait_event_type === "Lock");
            if (!waiting) await sql.unsafe("SELECT pg_sleep(0.01)");
          }
          expect(waiting).toBe(true);
          expect(finished).toBe(false);
        });
      } finally { if (mutation) await mutation; }
      expect(finished).toBe(true);
      expect(await authority(f.proof)).toBe(false);
      return;
    }
    // Capture the real owning adapter on its actual authority SELECT. This is
    // test setup, NOT a newly granted Service action or generated worker proof.
    const originalPrepare = Database.prototype.prepare;
    let owningAdapter: any;
    Database.prototype.prepare = function (statement: any, ...arguments_: any[]) {
      if (String(statement).startsWith('SELECT k."data" AS metadata')) owningAdapter = this;
      return Reflect.apply(originalPrepare, this, [statement, ...arguments_]);
    } as any;
    try {
      await expect(persistence.transaction(async (tx: any) => {
        expect(await tx.check_delivery_authority(bindingIdentity, f.proof)).toBe(true);
        expect(owningAdapter).toBeDefined();
        const statement = owningAdapter.prepare('UPDATE service SET status=$1 WHERE id=$2'); try { statement.run("disabled", f.service); } finally { statement.finalize(); }
        expect(await tx.check_delivery_authority(bindingIdentity, f.proof)).toBe(false);
        throw new Error("rollback authority mutation");
      })).rejects.toThrow("rollback authority mutation");
    } finally { Database.prototype.prepare = originalPrepare; }
    expect(await authority(f.proof)).toBe(true);
  });
});

describe(`checked selection and immutable enrollment component (${sql ? "postgres" : "sqlite"})`, () => {
  test("strict captured-time eligibility and active-owner visibility precede enrollment, without an ordinary email grant", async () => {
    await clearScanTodos(); const f = await authorityFixture();
    const eligible = todo(), equal = todo(), future = todo(), missingDue = todo(), done = todo(), deleted = todo(), sent = todo(), inactive = todo();
    equal.due_at = scanTime; future.due_at = "2026-10-09T08:00:00.000Z"; missingDue.due_at = null as any; done.status = "done";
    await client.transaction(async (tx: any) => { for (const value of [eligible, equal, future, missingDue, done, deleted, sent, inactive]) await tx.create_Todo(value); });
    await rows('UPDATE todo SET deleted_at=$1 WHERE id=$2', [sql ? instant : Date.parse(instant), deleted.id]);
    await setSent(sent.id);
    const disabled = crypto.randomUUID();
    await rows('INSERT INTO "user" (id,authentication_subject,email,status,created_at,disabled_at) VALUES ($1,$1,$2,$3,$4,$4)', [disabled, `${disabled}@example.invalid`, "disabled", sql ? instant : Date.parse(instant)]);
    await rows('UPDATE todo SET owner_id=$1 WHERE id=$2', [disabled, inactive.id]);
    const page = await scan(f.proof);
    expect(page.intents.map((i: any) => i.sourceEntityId)).toEqual([eligible.id]);
    expect(page.after).toBeNull();
    expect(page.intents[0].payload).toEqual({ idempotency_key: page.intents[0].intentId, from: authEnvironment.MAIL_SENDER, to: `${owner}@example.invalid`, todo_title: eligible.title, due_at: dueA });
    expect(page.intents[0].sourceRevision).toBe("1");
    expect(await persistence.withPolicy({ kind: "service", subject: f.subject, values: { service_id: f.service } }).allowsPolicy("User", "read.email")).toBe(false);
    await expect(persistence.select_delivery_intents(bindingIdentity, f.proof)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    await expect(persistence.transaction((tx: any) => tx.select_delivery_intents(bindingIdentity, f.proof))).rejects.toMatchObject({ operation: "delivery.operation_time" });
  });
  test("more than500 earlier unauthorized rows cannot hide501 eligible rows; continuation is typed due/id at one snapshot", async () => {
    await clearScanTodos(); const f = await authorityFixture(), disabled = crypto.randomUUID();
    await rows('INSERT INTO "user" (id,authentication_subject,email,status,created_at,disabled_at) VALUES ($1,$1,$2,$3,$4,$4)', [disabled, `${disabled}@example.invalid`, "disabled", sql ? instant : Date.parse(instant)]);
    const valid: string[] = [];
    await client.transaction(async (tx: any) => {
      for (let index = 0; index < 1002; index++) {
        const id = `018f57d0-bf42-4f25-9417-${String(index + 1).padStart(12, "0")}`, value = todo(id);
        if (index < 501) { value.owner_id = disabled; value.due_at = "2026-10-06T07:00:00.000Z"; }
        else valid.push(id);
        // Raw fixture owner seeding is not an ordinary role grant. Generated
        // create/hooks run under the actual owning user's unchanged policy.
        await tx.withPolicy({ kind: "user", subject: value.owner_id, values: { user_id: value.owner_id } }).create_Todo(value);
      }
    });
    const first = await scan(f.proof);
    expect(first.intents.map((i: any) => i.sourceEntityId)).toEqual(valid.slice(0, 500));
    expect(first.after).toEqual({ binding: bindingIdentity, operationTime: scanTime, dueAt: dueA, id: valid[499] });
    const second = await scan(f.proof, first.after);
    expect(second.intents.map((i: any) => i.sourceEntityId)).toEqual(valid.slice(500));
    expect(second.after).toBeNull();
    await expect(scan(f.proof, { ...first.after, operationTime: "2026-10-08T08:00:00.001Z" })).rejects.toMatchObject({ operation: "delivery.cursor_snapshot" });
    for (const cursor of [{ ...first.after, id: "not-a-uuid" }, { ...first.after, dueAt: null }, { ...first.after, binding: "another-binding" }, { ...first.after, offset: 500 }]) await expect(scan(f.proof, cursor)).rejects.toBeDefined();
  }, 30_000);
  test("repeat scan retains original identity and payload despite current title, recipient and configured sender changes", async () => {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    const first = (await scan(f.proof)).intents[0];
    await rows('UPDATE todo SET title=$1 WHERE id=$2', ["changed after enrollment", value.id]);
    await rows('UPDATE "user" SET email=$1 WHERE id=$2', [`changed-${owner}@example.invalid`, owner]);
    try {
      await app.initializeApplication({ ...authEnvironment, MAIL_SENDER: "changed-sender@example.invalid" });
      const current = await app.authenticationHost().prepareDeliveryCredential("api_bearer", f.issued.credential, Date.now());
      const second = (await scan(current)).intents[0];
      expect(second.intentId).toBe(first.intentId); expect(second.payload).toEqual(first.payload);
      expect(second.keySequence).toBe("1"); expect(await intentRows(value.id)).toHaveLength(1);
      // This retained recipient is NOT dispatch permission. Admission still
      // needs to compare it to current authorized recipient before any I/O.
      expect(second.payload.to).not.toBe(`changed-${owner}@example.invalid`);
    } finally { await rows('UPDATE "user" SET email=$1 WHERE id=$2', [`${owner}@example.invalid`, owner]); }
  });
  test(sql ? "observed PostgreSQL boundary lock-wait changes refuse a stale page and fresh bounded continuation covers every eligible row" : "stable SQLite pages retain bounded capture and current eligibility", async () => {
    if (!sql) {
      await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
      expect((await scan(f.proof)).intents[0].sourceEntityId).toBe(value.id); return;
    }
    for (const [iteration, change] of [{ due_at: dueB }, { due_at: scanTime }, { status: "done" }].entries()) {
      await clearScanTodos(); const f = await authorityFixture();
      const ids = Array.from({ length: 501 }, (_, i) => `018f57d0-bf42-4f25-9417-${String(30001 + iteration * 1000 + i).padStart(12, "0")}`);
      await client.transaction(async (tx: any) => { for (const id of ids) await tx.create_Todo(todo(id)); });
      let ready!: () => void, release!: () => void;
      const entered = new Promise<void>(resolve => { ready = resolve; }), held = new Promise<void>(resolve => { release = resolve; });
      const writer = client.transaction(async (tx: any) => { await tx[method](ids[499], change, null); ready(); await held; });
      await entered;
      // Bun's rejects matcher may synchronously wait before the release loop;
      // capture rejection as data so the native held writer is always released.
      const reading = scan(f.proof).then(page => ({ page, error: null }), error => ({ page: null, error }));
      try {
        let waiting = false;
        for (let attempt = 0; attempt < 400 && !waiting; attempt++) {
          const activity = await sql.unsafe("SELECT COUNT(*)::int AS n FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT%AS source_id%'");
          waiting = activity[0].n > 0; if (!waiting) await Bun.sleep(10);
        }
        expect(waiting).toBe(true);
      } finally { release(); await writer; }
      expect((await reading).error).toMatchObject({ operation: "delivery.selection_changed" });
      expect(Number((await rows('SELECT COUNT(*) AS n FROM "__jadpo_deliveries_v1" WHERE source_entity_id=$1', [ids[0]]))[0].n)).toBe(0);
      const first = await scan(f.proof), second = first.after === null ? { intents: [] } : await scan(f.proof, first.after);
      const intents = [...first.intents, ...second.intents];
      const expected = change.due_at === dueB ? ids : ids.filter(id => id !== ids[499]);
      expect(new Set(intents.map((i: any) => i.sourceEntityId))).toEqual(new Set(expected));
      if (change.due_at === dueB) {
        const moved = intents.find((i: any) => i.sourceEntityId === ids[499]); expect(moved.sourceRevision).toBe("2"); expect(moved.payload.due_at).toBe(dueB);
      }
      // Only this iteration's private fixture IDs/revisions are removed, in
      // the same disposable process; immutable production retention unchanged.
      await rows('DELETE FROM "todo"');
      await rows('DELETE FROM "__jadpo_deliveries_v1" WHERE source_entity_id IN (' + ids.map((_, i) => `$${i+1}`).join(',') + ')', ids);
    }
  }, 30_000);
  test("supplied equal and A-to-B-to-A schedules enroll distinct revision identities while duplicates retain each one", async () => {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    const intents = [(await scan(f.proof)).intents[0]];
    for (const due of [dueA, dueB, dueA]) {
      await patch(value.id, { due_at: due });
      const next = (await scan(f.proof)).intents[0]; intents.push(next);
      expect((await scan(f.proof)).intents[0].intentId).toBe(next.intentId);
    }
    expect(intents.map((i: any) => i.sourceRevision)).toEqual(["1", "2", "3", "4"]);
    expect(intents.map((i: any) => i.keySequence)).toEqual(["1", "2", "3", "4"]);
    expect(new Set(intents.map((i: any) => i.intentId)).size).toBe(4);
    expect(intents.map((i: any) => i.payload.due_at)).toEqual([dueA, dueA, dueB, dueA]);
    expect(intents.every((i: any) => i.payload.idempotency_key === i.intentId)).toBe(true);
  });
  test("whole-source rollback and missing revision cannot publish partial page enrollment", async () => {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    await expect(persistence.withOperationTime(scanTime).transaction(async (tx: any) => { await tx.select_delivery_intents(bindingIdentity, f.proof); throw new Error("test-owned rollback"); })).rejects.toBeDefined();
    expect(await intentRows(value.id)).toEqual([]); expect(await revision(value.id)).toBe("1");
    const lost = todo(); await client.create_Todo(lost);
    await rows('DELETE FROM "__jadpo_delivery_schedules_v1" WHERE source_entity=$1 AND source_entity_id=$2', ["Todo", lost.id]);
    await expect(scan(f.proof)).rejects.toMatchObject({ operation: "delivery.schedule_missing" });
    expect(await intentRows(value.id)).toEqual([]); expect(await intentRows(lost.id)).toEqual([]);
  });
  test("concurrent native scans resolve one immutable identity per business revision", async () => {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    const pages = await Promise.all([scan(f.proof), scan(f.proof)]);
    expect(pages[0].intents[0].intentId).toBe(pages[1].intents[0].intentId);
    expect(pages[0].intents[0].payload).toEqual(pages[1].intents[0].payload);
    expect(await intentRows(value.id)).toHaveLength(1);
  });
  test("caught native enrollment SQL failure rolls back the entire page and leaves surrounding source work committable", async () => {
    await clearScanTodos(); const f = await authorityFixture();
    const first = todo("018f57d0-bf42-4f25-9417-000000002001"), second = todo("018f57d0-bf42-4f25-9417-000000002002"), outer = todo();
    await client.transaction(async (tx: any) => { await tx.create_Todo(first); await tx.create_Todo(second); });
    // Isolated native foundation setup makes this test runnable on its own;
    // the sentinel is not a compiled reminder or authority assertion.
    await client.transaction((tx: any) => tx.enqueue_delivery_intent({ job: "fixture_schema", sourceOperation: "fixture", sourceEntity: "fixture", sourceEntityId: crypto.randomUUID(), sourceRevision: "1", orderingKey: "fixture", payloadVersion: "fixture.v1", payload: {} }));
    if (sql) {
      await rows(`CREATE FUNCTION rm306_enrollment_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_entity_id = '${second.id}' THEN RAISE EXCEPTION 'test-owned enrollment fault'; END IF; RETURN NEW; END $$`);
      await rows('CREATE TRIGGER rm306_enrollment_fault BEFORE INSERT ON "__jadpo_deliveries_v1" FOR EACH ROW EXECUTE FUNCTION rm306_enrollment_fault()');
    } else await rows(`CREATE TRIGGER rm306_enrollment_fault BEFORE INSERT ON "__jadpo_deliveries_v1" WHEN NEW.source_entity_id = '${second.id}' BEGIN SELECT RAISE(ABORT, 'test-owned enrollment fault'); END`);
    try {
      await persistence.withOperationTime(scanTime).withPolicy({ kind: "user", subject: owner, values: { user_id: owner } }, "Todo.patch_todo").transaction(async (tx: any) => {
        await expect(tx.select_delivery_intents(bindingIdentity, f.proof)).rejects.toMatchObject({ operation: "delivery.enqueue" });
        await tx.create_Todo(outer);
      });
      expect(await intentRows(first.id)).toEqual([]); expect(await intentRows(second.id)).toEqual([]);
      expect(await rows('SELECT ordering_key FROM "__jadpo_delivery_keys_v1" WHERE ordering_key=$1 OR ordering_key=$2', [first.id, second.id])).toEqual([]);
      expect(await stored(outer.id)).toHaveLength(1); expect(await revision(outer.id)).toBe("1");
    } finally {
      if (sql) { await rows('DROP TRIGGER rm306_enrollment_fault ON "__jadpo_deliveries_v1"'); await rows('DROP FUNCTION rm306_enrollment_fault()'); }
      else await rows('DROP TRIGGER rm306_enrollment_fault');
    }
  });
  test("a fresh process uses the actual generated host and preserves enrolled identity/payload after restart", async () => {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    const first = (await scan(f.proof)).intents[0];
    const code = `const app=await import(${JSON.stringify(pathToFileURL(join(root, "app.ts")).href)});const {persistence}=await import(${JSON.stringify(pathToFileURL(persistencePath).href)});await app.initializeApplication(${JSON.stringify(authEnvironment)});const proof=await app.authenticationHost().prepareDeliveryCredential("api_bearer",process.env.COMPONENT_WORKER_KEY,Date.now());const page=await persistence.withOperationTime(${JSON.stringify(scanTime)}).transaction(tx=>tx.select_delivery_intents(${JSON.stringify(bindingIdentity)},proof));console.log(JSON.stringify(page));process.exit(0);`;
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", code], { env: { ...Bun.env, DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath, COMPONENT_WORKER_KEY: f.issued.credential }, stdout: "pipe", stderr: "pipe" });
    const [out, err, exit] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
    expect(exit, err).toBe(0);
    const second = JSON.parse(out).intents[0]; expect(second.intentId).toBe(first.intentId); expect(second.payload).toEqual(first.payload);
    expect(await intentRows(value.id)).toHaveLength(1);
  });
  test("invalid source or retained payload cannot be admitted as a new immutable request", async () => {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    await rows('UPDATE todo SET title=$1 WHERE id=$2', ["", value.id]);
    await expect(scan(f.proof)).rejects.toBeDefined();
    await rows('UPDATE todo SET title=$1 WHERE id=$2', [value.title, value.id]);
    const original = (await scan(f.proof)).intents[0];
    for (const payload of [{ ...original.payload, idempotency_key: crypto.randomUUID() }, { ...original.payload, due_at: null }, { ...original.payload, extra: true }]) {
      await rows('UPDATE "__jadpo_deliveries_v1" SET payload=$1 WHERE intent_id=$2', [JSON.stringify(payload), original.intentId]);
      await expect(scan(f.proof)).rejects.toBeDefined();
      expect(await intentRows(value.id)).toHaveLength(1);
    }
  });
});

describe(`closed scheduler assembly components (${sql ? "postgres" : "sqlite"})`, () => {
  // Explicit native activation test profile; per-delivery golden approval does
  // not choose a 500-call activation profile.
  const profile = Object.freeze({ executionMs: 120_000, leaseMs: 90_000 });
  const activate = (proof: unknown, at = scanTime) => app.runDeliveryScheduleActivation(bindingIdentity, proof, profile, at);
  const activationRows = () => rows('SELECT * FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [bindingIdentity]);
  async function fixture() {
    await clearScanTodos();
    // Initialize the actual native schema, then isolate this binding's ticks.
    await persistence.transaction((tx: any) => tx.tick_delivery_schedule(bindingIdentity, 900_000, scanTime));
    await rows('DELETE FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [bindingIdentity]);
    const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    return { ...f, value };
  }
  async function provider(work: (calls: any[], endpoint: string) => Promise<void>, onCall?: () => Promise<void>) {
    const calls: any[] = [], previous = Bun.env.NODE_ENV; Bun.env.NODE_ENV = "test";
    const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
      const body = await request.json();
      // A separate connection observes both durable commits before HTTP.
      const activation = (await activationRows())[0];
      const claim = (await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1', [body.idempotency_key]))[0];
      calls.push({ body, page: JSON.parse(activation.page), possibleDispatch: Number(claim.possible_dispatch) });
      if (onCall) await onCall();
      return Response.json({ accepted_at: instant }, { status: 202, headers: { "Idempotency-Key": body.idempotency_key } });
    } });
    const endpoint = `http://127.0.0.1:${server.port}`;
    setReferenceMailEndpointForTesting(endpoint);
    try { await work(calls, endpoint); }
    finally { setReferenceMailEndpointForTesting(null); server.stop(true); if (previous === undefined) delete Bun.env.NODE_ENV; else Bun.env.NODE_ENV = previous; }
  }
  test("generated scheduler commits selection/page and admission before provider dispatch, then finishes one singleton tick", async () => {
    const f = await fixture();
    await provider(async calls => {
      expect(await activate(f.proof)).toEqual({ status: "finished", selected: 1, attempted: 1, continued: false });
      expect(calls).toHaveLength(1); expect(calls[0].possibleDispatch).toBe(1);
      expect(calls[0].page.intentIds).toEqual([calls[0].body.idempotency_key]);
      expect(calls[0].body).toEqual((await persistence.read_delivery_intent(calls[0].body.idempotency_key)).payload);
      expect(await activate(f.proof)).toEqual({ status: "idle", selected: 0, attempted: 0, continued: false });
      expect(calls).toHaveLength(1);
    });
    const saved = (await activationRows())[0]; expect(saved.activation_id).toBeNull(); expect(saved.page).toBeNull();
    expect((await stored(f.value.id))[0].reminder_sent_at).not.toBeNull();
  });
  test("forged credential, unknown binding and missing/invalid profiles refuse before durable enrollment or HTTP", async () => {
    const f = await fixture();
    await provider(async calls => {
      await expect(activate({})).rejects.toMatchObject({ code: "invalid_credentials" });
      await expect(app.runDeliveryScheduleActivation("foreign-binding", f.proof, profile, scanTime)).rejects.toMatchObject({ code: "authority_invariant" });
      for (const invalid of [undefined, null, {}, { executionMs: 0, leaseMs: 1 }, { executionMs: 10, leaseMs: 20 }, { ...profile, extra: true }]) {
        await expect(app.runDeliveryScheduleActivation(bindingIdentity, f.proof, invalid, scanTime)).rejects.toBeDefined();
      }
      expect(calls).toHaveLength(0); expect(await activationRows()).toEqual([]); expect(await intentRows(f.value.id)).toEqual([]);
    });
    await expect(persistence.delivery_activation_clock()).rejects.toMatchObject({ operation: "delivery.transaction_required" });
  });
  test("owning page COMMIT failure leaves no intent/activation and cannot dispatch", async () => {
    const f = await fixture();
    await rows('CREATE TABLE rm306_activation_parent (id TEXT PRIMARY KEY)');
    await rows('CREATE TABLE rm306_activation_child (id TEXT REFERENCES rm306_activation_parent(id) DEFERRABLE INITIALLY DEFERRED)');
    if (sql) {
      await rows("CREATE FUNCTION rm306_activation_commit_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.page IS NOT NULL THEN INSERT INTO rm306_activation_child VALUES ('missing-parent'); END IF; RETURN NEW; END $$");
      await rows('CREATE TRIGGER rm306_activation_commit_fault AFTER UPDATE ON "__jadpo_delivery_activations_v1" FOR EACH ROW EXECUTE FUNCTION rm306_activation_commit_fault()');
    } else await rows("CREATE TRIGGER rm306_activation_commit_fault AFTER UPDATE OF page ON __jadpo_delivery_activations_v1 WHEN NEW.page IS NOT NULL BEGIN INSERT INTO rm306_activation_child VALUES ('missing-parent'); END");
    try {
      await provider(async calls => {
        await expect(activate(f.proof)).rejects.toBeDefined();
        expect(calls).toHaveLength(0); expect(await activationRows()).toEqual([]); expect(await intentRows(f.value.id)).toEqual([]);
      });
    } finally {
      await rows(sql ? 'DROP TRIGGER rm306_activation_commit_fault ON "__jadpo_delivery_activations_v1"' : 'DROP TRIGGER rm306_activation_commit_fault');
      if (sql) await rows('DROP FUNCTION rm306_activation_commit_fault()');
      await rows('DROP TABLE rm306_activation_child'); await rows('DROP TABLE rm306_activation_parent');
    }
    await provider(async calls => { expect((await activate(f.proof)).status).toBe("finished"); expect(calls).toHaveLength(1); });
  });
  test("overlapping activation stays idle while the committed singleton page owns the provider call", async () => {
    const f = await fixture();
    let release!: () => void, entered!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    const observed = new Promise<void>(resolve => { entered = resolve; });
    await provider(async calls => {
      const first = activate(f.proof);
      try {
        await observed;
        expect((await activate(f.proof)).status).toBe("idle"); expect(calls).toHaveLength(1);
      } finally { release(); }
      expect((await first).status).toBe("finished"); expect(calls).toHaveLength(1);
    }, async () => { entered(); await held; });
  });
  test("generated between-intent renewal outlives the initial lease without changing the immutable activation page or execution cap", async () => {
    const f = await fixture();
    for (let i = 0; i < 3; i++) await client.create_Todo(todo());
    const short = { executionMs: 5_000, leaseMs: 1_500 };
    let initial: any;
    await provider(async calls => {
      const result = await app.runDeliveryScheduleActivation(bindingIdentity, f.proof, short, scanTime);
      expect(result).toEqual({ status: "finished", selected: 4, attempted: 4, continued: false });
      expect(calls).toHaveLength(4);
      expect(new Set(calls.map(call => call.body.idempotency_key)).size).toBe(4);
      expect((await activationRows())[0].activation_id).toBeNull();
    }, async () => {
      const row = (await activationRows())[0];
      if (initial === undefined) initial = row;
      else {
        expect(row.activation_id).toBe(initial.activation_id); expect(row.generation).toBe(initial.generation);
        expect(row.execution_deadline).toBe(initial.execution_deadline); expect(row.page).toBe(initial.page);
        expect(row.lease_until >= initial.lease_until).toBe(true);
      }
      await Bun.sleep(500);
    });
    expect(Date.now() >= Date.parse(initial.lease_until)).toBe(true);
  });
  test("generated renewals cannot reset the activation execution deadline or discard its unfinished work", async () => {
    const f = await fixture(); for (let i = 0; i < 5; i++) await client.create_Todo(todo());
    let initial: any;
    await provider(async calls => {
      const result = await app.runDeliveryScheduleActivation(bindingIdentity, f.proof, { executionMs: 1_500, leaseMs: 1_000 }, scanTime);
      expect(result.status).toBe("partial"); expect(result.selected).toBe(6); expect(result.attempted < 6).toBe(true);
      expect(calls.length < 6).toBe(true);
      const row = (await activationRows())[0];
      expect(row.activation_id).toBe(initial.activation_id); expect(row.execution_deadline).toBe(initial.execution_deadline);
      expect(row.page).toBe(initial.page); expect(row.cursor).toBeNull();
      expect(row.lease_until <= row.execution_deadline).toBe(true);
    }, async () => {
      const row = (await activationRows())[0];
      if (initial === undefined) initial = row;
      else expect(row.execution_deadline).toBe(initial.execution_deadline);
      await Bun.sleep(400);
    });
  });
  test("expired activation refuses renewal before another admission and restart retains the original page", async () => {
    const f = await fixture(), other = todo(); await client.create_Todo(other);
    let originalPage: string;
    await provider(async calls => {
      expect(await activate(f.proof)).toEqual({ status: "partial", selected: 2, attempted: 1, continued: true });
      expect(calls).toHaveLength(1);
      expect((await activationRows())[0].page).toBe(originalPage!);
      expect(Number((await activationRows())[0].generation)).toBe(1);
    }, async () => {
      originalPage = (await activationRows())[0].page;
      await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", bindingIdentity]);
    });
    await provider(async calls => {
      expect(await activate(f.proof)).toEqual({ status: "finished", selected: 2, attempted: 2, continued: false });
      expect(calls).toHaveLength(1); expect(JSON.stringify(calls[0].page)).toBe(originalPage!);
      expect(Number((await activationRows())[0].generation)).toBe(2);
    });
  });
  async function staged(f: Awaited<ReturnType<typeof fixture>>) {
    return persistence.withOperationTime(scanTime).transaction(async (tx: any) => {
      await tx.tick_delivery_schedule(bindingIdentity, 900_000, scanTime);
      const handle = await tx.claim_delivery_activation(bindingIdentity, scanTime, profile);
      const page = await tx.select_delivery_intents(bindingIdentity, f.proof, null);
      expect(await tx.stage_delivery_activation_page(handle, page.intents.map((intent: any) => intent.intentId), page.after)).toBe(true);
      return page.intents[0];
    });
  }
  const expire = () => rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1,execution_deadline=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", bindingIdentity]);
  test("malformed/cross-binding retained page and cursor refuse before commit or provider effect, preserving every durable byte", async () => {
    const valid = { binding: bindingIdentity, operationTime: scanTime, dueAt: dueA, id: "018f57d0-bf42-4f25-9417-000000007001" };
    for (const after of [{ ...valid, binding: "foreign-binding" }, { ...valid, operationTime: "bad" }, { ...valid, id: "bad" }, { ...valid, dueAt: scanTime }, { ...valid, extra: true }]) {
      const f = await fixture(), intent = await staged(f); await expire();
      await rows('UPDATE "__jadpo_delivery_activations_v1" SET page=$1 WHERE binding=$2', [JSON.stringify({ intentIds: [intent.intentId], after }), bindingIdentity]);
      const before = await activationRows();
      await provider(async calls => { await expect(activate(f.proof)).rejects.toBeDefined(); expect(calls).toHaveLength(0); });
      expect(await activationRows()).toEqual(before); expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("pending");
    }
    const f = await fixture(), intent = await staged(f); await expire();
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET cursor=$1,continuation_for=$2 WHERE binding=$3', [JSON.stringify({ ...valid, binding: "foreign-binding" }), scanTime, bindingIdentity]);
    const before = await activationRows();
    await provider(async calls => { await expect(activate(f.proof)).rejects.toBeDefined(); expect(calls).toHaveLength(0); });
    expect(await activationRows()).toEqual(before); expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("pending");
  });
  test("fresh generated process resumes the immutable staged page with a new operation snapshot and original intent identity", async () => {
    const f = await fixture(), intent = await staged(f); await expire();
    // A later source is not allowed to replace or enlarge retained work.
    const later = todo(); await client.create_Todo(later);
    const resumedAt = "2026-10-08T08:01:00.000Z";
    await provider(async (calls, endpoint) => {
      const code = `const app=await import(${JSON.stringify(pathToFileURL(join(root, "app.ts")).href)});const adapter=await import(${JSON.stringify(pathToFileURL(join(root, "service-adapter.ts")).href)});await app.initializeApplication(${JSON.stringify(authEnvironment)});adapter.setReferenceMailEndpointForTesting(${JSON.stringify(endpoint)});const proof=await app.authenticationHost().prepareDeliveryCredential("api_bearer",process.env.COMPONENT_WORKER_KEY,Date.now());const result=await app.runDeliveryScheduleActivation(${JSON.stringify(bindingIdentity)},proof,${JSON.stringify(profile)},${JSON.stringify(resumedAt)});console.log(JSON.stringify(result));process.exit(0);`;
      const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", code], { env: { ...Bun.env, NODE_ENV: "test", DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath, COMPONENT_WORKER_KEY: f.issued.credential }, stdout: "pipe", stderr: "pipe" });
      const [out, err, exit] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
      expect(exit, err).toBe(0); expect(JSON.parse(out.trim().split("\n").at(-1)!)).toEqual({ status: "finished", selected: 1, attempted: 1, continued: false });
      expect(calls).toHaveLength(1); expect(calls[0].body.idempotency_key).toBe(intent.intentId); expect(calls[0].page.intentIds).toEqual([intent.intentId]);
      expect((await stored(later.id))[0].reminder_sent_at).toBeNull(); expect(await intentRows(later.id)).toEqual([]);
    });
  }, 30_000);
});

describe(`committed admission and adapter-origin outcome components (${sql ? "postgres" : "sqlite"})`, () => {
  // Explicit test-owned finite profile, not the unanswered production proposal.
  const profile = Object.freeze({ executionMs: 120_000, leaseMs: 90_000 });
  const prepare = (proof: unknown, id: string) => app.prepareDeliveryInvocation(bindingIdentity, proof, id, scanTime, profile);
  async function enrolled() {
    await clearScanTodos(); const f = await authorityFixture(), value = todo(); await client.create_Todo(value);
    const intent = (await scan(f.proof)).intents[0]; return { ...f, value, intent };
  }
  async function provider(kind: "accepted" | "lost" | "rejected" | "rate_limited", work: (calls: any[]) => Promise<void>) {
    const calls: any[] = [], previous = Bun.env.NODE_ENV; Bun.env.NODE_ENV = "test";
    const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
      const body = await request.json(); calls.push({ body, key: request.headers.get("Idempotency-Key"), credential: request.headers.get("Authorization") });
      if (kind === "lost") return new Response("invalid acknowledgement", { status: 202, headers: { "Idempotency-Key": body.idempotency_key } });
      if (kind === "rejected") return Response.json({ code: "invalid_recipient" }, { status: 400 });
      if (kind === "rate_limited") return Response.json({ code: "rate_limited" }, { status: 429 });
      return Response.json({ accepted_at: "2099-01-01T00:00:00.000Z" }, { status: 202, headers: { "Idempotency-Key": body.idempotency_key } });
    } });
    setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);
    try { await work(calls); }
    finally { setReferenceMailEndpointForTesting(null); server.stop(true); if (previous === undefined) delete Bun.env.NODE_ENV; else Bun.env.NODE_ENV = previous; }
  }
  test("owning commit publishes one empty, noncopyable one-use grant and actual adapter receipt alone can complete", async () => {
    const f = await enrolled();
    const before = (await stamps(f.value.id))[0];
    await expect(prepare({}, f.intent.intentId)).rejects.toMatchObject({ code: "invalid_credentials" });
    const grant = await prepare(f.proof, f.intent.intentId); expect(grant).not.toBeNull(); expect(Reflect.ownKeys(grant)).toEqual([]); expect(Object.isFrozen(grant)).toBe(true);
    const ledger = (await rows('SELECT possible_dispatch,invocations FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1', [f.intent.intentId]))[0];
    expect(Number(ledger.possible_dispatch)).toBe(1); expect(Number(ledger.invocations)).toBe(1);
    expect(await prepare(f.proof, f.intent.intentId)).toBeNull();
    for (const forged of [{}, { ...grant }, JSON.parse(JSON.stringify(grant)), Object.create(grant), f.intent]) await expect(app.dispatchDeliveryInvocation(forged)).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(app.completeDeliveryInvocation({ accepted_at: instant })).rejects.toMatchObject({ code: "invalid_credentials" });
    await provider("accepted", async calls => {
      const receipt = await app.dispatchDeliveryInvocation(grant); expect(Reflect.ownKeys(receipt)).toEqual([]);
      expect(calls).toHaveLength(1); expect(calls[0].body).toEqual(f.intent.payload); expect(calls[0].key).toBe(f.intent.intentId);
      await expect(app.dispatchDeliveryInvocation(grant)).rejects.toMatchObject({ code: "invalid_credentials" });
      await expect(app.completeDeliveryInvocation({ ...receipt })).rejects.toMatchObject({ code: "invalid_credentials" });
      expect(await app.completeDeliveryInvocation(receipt)).toBe(true);
      await expect(app.completeDeliveryInvocation(receipt)).rejects.toMatchObject({ code: "invalid_credentials" });
    });
    const receipt = (await rows('SELECT accepted_at,observed_at FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1', [f.intent.intentId]))[0];
    expect(receipt.accepted_at).toBe("2099-01-01T00:00:00.000Z");
    expect(new Date((await stored(f.value.id))[0].reminder_sent_at).toISOString()).toBe(receipt.observed_at);
    expect(receipt.observed_at).not.toBe(receipt.accepted_at);
    const changed = (await stamps(f.value.id))[0];
    expect(changed.created_at).toEqual(before.created_at);
    expect(new Date(changed.updated_at).toISOString()).not.toBe(new Date(before.updated_at).toISOString());
    expect(new Date(changed.updated_at).toISOString()).not.toBe(receipt.accepted_at);
    expect(await revision(f.value.id)).toBe("1");
    expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("succeeded");
  });
  test("current source revision, recipient, owner eligibility and role refuse pre-admission and cannot change immutable payload", async () => {
    for (const mutation of ["revision", "recipient", "done", "deleted", "sent", "owner", "role"] as const) {
      const f = await enrolled();
      if (mutation === "revision") await patch(f.value.id, { due_at: dueB });
      if (mutation === "recipient") await rows('UPDATE "user" SET email=$1 WHERE id=$2', [`changed-${owner}@example.invalid`, owner]);
      if (mutation === "done") await patch(f.value.id, { status: "done" });
      if (mutation === "deleted") await rows('UPDATE todo SET deleted_at=$1 WHERE id=$2', [dbInstant(Date.now()), f.value.id]);
      if (mutation === "sent") await setSent(f.value.id);
      if (mutation === "owner") await rows('UPDATE "user" SET status=$1 WHERE id=$2', ["disabled", owner]);
      if (mutation === "role") await rows('UPDATE reminder_service_membership SET role=$1 WHERE id=$2', ["observer", f.membership]);
      try { expect(await prepare(f.proof, f.intent.intentId)).toBeNull(); expect((await persistence.read_delivery_intent(f.intent.intentId)).payload).toEqual(f.intent.payload); }
      finally { await rows('UPDATE "user" SET email=$1,status=$2 WHERE id=$3', [`${owner}@example.invalid`, "active", owner]); }
    }
  });
  test("post-admission role/configuration withdrawal is no extra cut; receipt cannot mark a newer or ABA schedule", async () => {
    const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId);
    await patch(f.value.id, { due_at: dueB }); await patch(f.value.id, { due_at: dueA });
    const before = (await stamps(f.value.id))[0].updated_at;
    await rows('UPDATE reminder_service_membership SET role=$1 WHERE id=$2', ["observer", f.membership]);
    await expect(app.initializeApplication({ ...authEnvironment, SESSION_SIGNING_KEY: "bad" })).rejects.toBeDefined();
    await provider("accepted", async calls => { const receipt = await app.dispatchDeliveryInvocation(grant); expect(calls[0].body).toEqual(f.intent.payload); expect(await app.completeDeliveryInvocation(receipt)).toBe(true); });
    expect((await stored(f.value.id))[0].reminder_sent_at).toBeNull(); expect(await revision(f.value.id)).toBe("3");
    expect((await stamps(f.value.id))[0].updated_at).toEqual(before);
    expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("succeeded");
  });
  test("possible acceptance without valid acknowledgement remains unknown and blocks newer same-key schedules", async () => {
    const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId);
    await provider("lost", async calls => { const receipt = await app.dispatchDeliveryInvocation(grant); expect(await app.completeDeliveryInvocation(receipt)).toBe(true); expect(calls).toHaveLength(1); });
    expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("outcome_unknown");
    await patch(f.value.id, { due_at: dueB }); const next = (await scan(f.proof)).intents[0];
    expect(next.intentId).not.toBe(f.intent.intentId); expect(await prepare(f.proof, next.intentId)).toBeNull(); expect((await stored(f.value.id))[0].reminder_sent_at).toBeNull();
  });
  test("typed adapter no-effect rejection fails permanently; transient exhaustion preserves same-ID retry budget", async () => {
    for (const kind of ["rejected", "rate_limited"] as const) {
      const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId);
      await provider(kind, async calls => {
        const receipt = await app.dispatchDeliveryInvocation(grant);
        expect(await app.completeDeliveryInvocation(receipt)).toBe(kind === "rejected" ? "failed" : "retry_wait");
        expect(calls).toHaveLength(kind === "rejected" ? 1 : 3);
        for (const call of calls) { expect(call.key).toBe(f.intent.intentId); expect(call.body).toEqual(f.intent.payload); }
      });
      expect((await stored(f.value.id))[0].reminder_sent_at).toBeNull();
    }
  });
  test("rollback after nested admission releases no dispatcher token; deferred native COMMIT failure leaves pending intent", async () => {
    const f = await enrolled();
    let facts: any;
    await expect(persistence.withOperationTime(scanTime).transaction(async (tx: any) => { facts = await tx.prepare_delivery_invocation(bindingIdentity, f.proof, f.intent.intentId, { ...profile, maxInvocations: 3, lifetimeMs: 3_600_000 }); throw new Error("owning rollback"); })).rejects.toThrow("owning rollback");
    await expect(app.dispatchDeliveryInvocation(facts)).rejects.toMatchObject({ code: "invalid_credentials" });
    expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("pending");
    await rows('CREATE TABLE rm306_commit_parent (id TEXT PRIMARY KEY)');
    await rows('CREATE TABLE rm306_commit_child (id TEXT REFERENCES rm306_commit_parent(id) DEFERRABLE INITIALLY DEFERRED)');
    if (sql) {
      await rows("CREATE FUNCTION rm306_commit_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.possible_dispatch = 1 THEN INSERT INTO rm306_commit_child VALUES ('missing-parent'); END IF; RETURN NEW; END $$");
      await rows('CREATE TRIGGER rm306_commit_fault AFTER UPDATE ON "__jadpo_delivery_claims_v1" FOR EACH ROW EXECUTE FUNCTION rm306_commit_fault()');
    } else await rows("CREATE TRIGGER rm306_commit_fault AFTER UPDATE OF possible_dispatch ON __jadpo_delivery_claims_v1 WHEN NEW.possible_dispatch=1 BEGIN INSERT INTO rm306_commit_child VALUES ('missing-parent'); END");
    try {
      await expect(prepare(f.proof, f.intent.intentId)).rejects.toBeDefined();
      expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("pending");
      expect(await rows('SELECT intent_id FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1', [f.intent.intentId])).toHaveLength(0);
    } finally {
      await rows(sql ? 'DROP TRIGGER rm306_commit_fault ON "__jadpo_delivery_claims_v1"' : 'DROP TRIGGER rm306_commit_fault');
      if (sql) await rows('DROP FUNCTION rm306_commit_fault()');
      await rows('DROP TABLE rm306_commit_child'); await rows('DROP TABLE rm306_commit_parent');
    }
    expect(await prepare(f.proof, f.intent.intentId)).not.toBeNull();
  });
  test("a caught native sent-write fault rolls receipt and state back; the same adapter outcome completes without another mail", async () => {
    const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId);
    const before = (await stamps(f.value.id))[0];
    await provider("accepted", async calls => {
      const receipt = await app.dispatchDeliveryInvocation(grant), receivedBy = Date.now();
      if (sql) {
        await rows("CREATE FUNCTION rm306_sent_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test-owned sent fault'; END $$");
        await rows('CREATE TRIGGER rm306_sent_fault BEFORE UPDATE OF reminder_sent_at ON todo FOR EACH ROW EXECUTE FUNCTION rm306_sent_fault()');
      } else await rows("CREATE TRIGGER rm306_sent_fault BEFORE UPDATE OF reminder_sent_at ON todo BEGIN SELECT RAISE(ABORT,'test-owned sent fault'); END");
      try {
        await expect(app.completeDeliveryInvocation(receipt)).rejects.toMatchObject({ operation: "delivery.completion.sent" });
        expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("running");
        expect(await rows('SELECT intent_id FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1', [f.intent.intentId])).toHaveLength(0);
        expect((await stamps(f.value.id))[0]).toEqual(before);
      } finally {
        await rows(sql ? 'DROP TRIGGER rm306_sent_fault ON todo' : 'DROP TRIGGER rm306_sent_fault'); if (sql) await rows('DROP FUNCTION rm306_sent_fault()');
      }
      await Bun.sleep(20);
      expect(await app.completeDeliveryInvocation(receipt)).toBe(true); expect(calls).toHaveLength(1);
      const observation = (await rows('SELECT observed_at FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1', [f.intent.intentId]))[0].observed_at;
      expect(Date.parse(observation)).toBeLessThanOrEqual(receivedBy);
      expect(new Date((await stored(f.value.id))[0].reminder_sent_at).toISOString()).toBe(observation);
      const changed = (await stamps(f.value.id))[0];
      expect(changed.created_at).toEqual(before.created_at);
      expect(Date.parse(new Date(changed.updated_at).toISOString())).toBeGreaterThan(receivedBy);
      expect(await revision(f.value.id)).toBe("1");
    });
  });
  test("already sent, missing source and expired fence do not touch generated timestamps or change the business revision", async () => {
    for (const kind of ["sent", "missing", "fence"] as const) {
      const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId);
      const before = (await stamps(f.value.id))[0];
      await provider("accepted", async calls => {
        const receipt = await app.dispatchDeliveryInvocation(grant);
        if (kind === "sent") await setSent(f.value.id);
        if (kind === "missing") await rows('DELETE FROM todo WHERE id=$1', [f.value.id]);
        if (kind === "fence") await rows('UPDATE "__jadpo_delivery_claims_v1" SET lease_until=$1 WHERE intent_id=$2', ["2000-01-01T00:00:00.000Z", f.intent.intentId]);
        expect(await app.completeDeliveryInvocation(receipt)).toBe(kind !== "fence");
        expect(calls).toHaveLength(1);
      });
      expect(await revision(f.value.id)).toBe("1");
      const after = await stamps(f.value.id);
      if (kind === "missing") expect(after).toHaveLength(0); else expect(after[0]).toEqual(before);
    }
  });
  test("owner disable and source deletion after committed admission do not create an extra completion authority cut", async () => {
    const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId);
    await rows('UPDATE "user" SET status=$1 WHERE id=$2', ["disabled", owner]); await rows('UPDATE todo SET deleted_at=$1 WHERE id=$2', [dbInstant(Date.now()), f.value.id]);
    try { await provider("accepted", async calls => { const receipt = await app.dispatchDeliveryInvocation(grant); expect(await app.completeDeliveryInvocation(receipt)).toBe(true); expect(calls).toHaveLength(1); }); }
    finally { await rows('UPDATE "user" SET status=$1 WHERE id=$2', ["active", owner]); }
    expect((await stored(f.value.id))[0].reminder_sent_at).not.toBeNull();
    expect(new Date((await stamps(f.value.id))[0].updated_at).toISOString()).not.toBe(instant);
  });
  if (representationVariant) test("renamed generated field and actual derived change record complete atomically, including a caught change-log failure", async () => {
    const f = await enrolled(), grant = await prepare(f.proof, f.intent.intentId), before = await stamps(f.value.id);
    const changes = () => rows('SELECT operation,payload,created_at FROM "__jadpo_changes" WHERE entity=$1 AND entity_id=$2 ORDER BY revision', ["Todo", f.value.id]);
    const original = await changes(); expect(original).toHaveLength(1);
    await provider("accepted", async calls => {
      const receipt = await app.dispatchDeliveryInvocation(grant);
      if (sql) {
        await rows("CREATE FUNCTION rm306_change_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test-owned derived change fault'; END $$");
        await rows('CREATE TRIGGER rm306_change_fault BEFORE INSERT ON "__jadpo_changes" FOR EACH ROW EXECUTE FUNCTION rm306_change_fault()');
      } else await rows("CREATE TRIGGER rm306_change_fault BEFORE INSERT ON __jadpo_changes BEGIN SELECT RAISE(ABORT,'test-owned derived change fault'); END");
      try {
        await expect(app.completeDeliveryInvocation(receipt)).rejects.toMatchObject({ operation: "change.Todo.update" });
        expect(await stamps(f.value.id)).toEqual(before); expect(await changes()).toEqual(original);
        expect((await stored(f.value.id))[0].reminder_sent_at).toBeNull();
        expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("running");
        expect(await rows('SELECT intent_id FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1', [f.intent.intentId])).toHaveLength(0);
      } finally {
        await rows(sql ? 'DROP TRIGGER rm306_change_fault ON "__jadpo_changes"' : 'DROP TRIGGER rm306_change_fault'); if (sql) await rows('DROP FUNCTION rm306_change_fault()');
      }
      expect(await app.completeDeliveryInvocation(receipt)).toBe(true); expect(calls).toHaveLength(1);
      const records = await changes(); expect(records).toHaveLength(2); expect(records[1].operation).toBe("update");
      const record = JSON.parse(records[1].payload), after = (await stamps(f.value.id))[0];
      expect(new Date(record.modified_at).toISOString()).toBe(new Date(after.updated_at).toISOString());
      expect(new Date(record.reminder_sent_at).toISOString()).toBe(new Date((await stored(f.value.id))[0].reminder_sent_at).toISOString());
      expect(new Date(records[1].created_at).toISOString()).toBe(new Date(after.updated_at).toISOString());
      expect(after.created_at).toEqual(before[0].created_at); expect(await revision(f.value.id)).toBe("1");
    });
  });
  if (!sql) test("native SQLite commit applied but acknowledgement lost publishes no token and recovery preserves uncertainty", async () => {
    const f = await enrolled(), original = Database.prototype.exec;
    let applied = false, grant: unknown;
    Database.prototype.exec = function(statement: any) {
      const result = Reflect.apply(original, this, [statement]);
      if (statement === "COMMIT" && !applied) { applied = true; throw new Error("test-owned acknowledgement loss after actual COMMIT"); }
      return result;
    } as any;
    try { await expect(prepare(f.proof, f.intent.intentId).then(value => { grant = value; })).rejects.toMatchObject({ operation: "transaction.commit", kind: "unknown", transaction: { outcome: "unknown", commitAcknowledged: false } }); }
    finally { Database.prototype.exec = original; }
    expect(applied).toBe(true); expect(grant).toBeUndefined(); await expect(app.dispatchDeliveryInvocation(grant)).rejects.toMatchObject({ code: "invalid_credentials" });
    expect((await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1', [f.intent.intentId]))[0].possible_dispatch).toBe(1);
    await rows('UPDATE "__jadpo_delivery_claims_v1" SET lease_until=$1 WHERE intent_id=$2', ["2000-01-01T00:00:00.000Z", f.intent.intentId]);
    expect(await prepare(f.proof, f.intent.intentId)).toBeNull(); expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("outcome_unknown");
  });
});

describe(`native durable singleton activation storage (${sql ? "postgres" : "sqlite"})`, () => {
  const profile = Object.freeze({ executionMs: 120_000, leaseMs: 90_000 });
  const interval = 900_000;
  const tick = (binding: string, at: string) => persistence.transaction((tx: any) => tx.tick_delivery_schedule(binding, interval, at));
  const claim = (binding: string, at: string) => persistence.transaction((tx: any) => tx.claim_delivery_activation(binding, at, profile));
  const stage = (handle: any, ids: string[], after: unknown = null) => persistence.transaction((tx: any) => tx.stage_delivery_activation_page(handle, ids, after));
  const finish = (handle: any) => persistence.transaction((tx: any) => tx.finish_delivery_activation(handle));
  const renew = (handle: any) => persistence.transaction((tx: any) => tx.renew_delivery_activation(handle));
  const state = async (binding: string) => (await rows('SELECT * FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [binding]))[0];
  test("UTC interval floor coalesces downtime into one occurrence and activation time remains distinct", async () => {
    const binding = crypto.randomUUID();
    for (const at of ["2026-10-05T08:01:02.003Z", "2026-10-05T09:16:02.003Z", "2026-10-05T09:31:02.003Z"]) await tick(binding, at);
    expect((await state(binding)).pending_for).toBe("2026-10-05T09:30:00.000Z");
    expect(await rows('SELECT binding FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [binding])).toHaveLength(1);
    const handle = await claim(binding, "2026-10-05T10:01:02.003Z");
    expect(handle.scheduledFor).toBe("2026-10-05T09:30:00.000Z");
    expect(handle.operationTime).toBe("2026-10-05T10:01:02.003Z");
    expect(await stage(handle, [])).toBe(true); expect(await finish(handle)).toBe(true);
    await tick(binding, "2026-10-05T09:31:02.003Z"); await tick(binding, "2026-10-05T08:01:02.003Z");
    expect(await claim(binding, scanTime)).toBeNull();
  });
  test("concurrent claim has one winner; overlapping ticks retain one latest follow-up", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant);
    const claims = await Promise.all([claim(binding, instant), claim(binding, instant)]), handle = claims.find(Boolean);
    expect(claims.filter(Boolean)).toHaveLength(1);
    for (const at of ["2026-10-05T08:15:00.000Z", "2026-10-05T08:30:00.000Z", "2026-10-05T08:45:00.000Z"]) await tick(binding, at);
    expect(await claim(binding, scanTime)).toBeNull();
    expect((await state(binding)).pending_for).toBe("2026-10-05T08:45:00.000Z");
    expect(await stage(handle, [])).toBe(true); expect(await finish(handle)).toBe(true);
    const followup = await claim(binding, scanTime); expect(followup.scheduledFor).toBe("2026-10-05T08:45:00.000Z");
    expect(await stage(followup, [])).toBe(true); expect(await finish(followup)).toBe(true); expect(await claim(binding, scanTime)).toBeNull();
  });
  test("page commits atomically, is bounded and immutable; continuation gets a fresh activation snapshot", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant); const first = await claim(binding, instant);
    const ids = Array.from({ length: 500 }, () => crypto.randomUUID()), cursor = { binding: bindingIdentity, operationTime: instant, dueAt: dueA, id: ids[499] };
    await expect(persistence.transaction(async (tx: any) => { await tx.stage_delivery_activation_page(first, ids, cursor); throw new Error("page rollback"); })).rejects.toThrow("page rollback");
    expect((await state(binding)).page).toBeNull();
    await expect(stage(first, [...ids, crypto.randomUUID()], cursor)).rejects.toMatchObject({ operation: "delivery.activation_page" });
    await expect(stage(first, [ids[0], ids[0]], cursor)).rejects.toMatchObject({ operation: "delivery.activation_page" });
    expect(await stage(first, ids, cursor)).toBe(true);
    await expect(stage(first, [], null)).rejects.toMatchObject({ operation: "delivery.activation_page_staged" });
    expect(await finish(first)).toBe(true);
    const second = await claim(binding, scanTime); expect(second.cursor).toEqual(cursor); expect(second.operationTime).toBe(scanTime); expect(second.scheduledFor).toBe(first.scheduledFor);
    expect(await finish(first)).toBe(false); expect(await stage(second, [], null)).toBe(true); expect(await finish(second)).toBe(true);
    expect(await claim(binding, scanTime)).toBeNull();
  });
  test("fresh process recovers a staged page after lease expiry; stale activation cannot checkpoint or finish", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant); const first = await claim(binding, instant), ids = [crypto.randomUUID(), crypto.randomUUID()];
    expect(await stage(first, ids, null)).toBe(true);
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", binding]);
    const child = Bun.spawn(["bun", "--env-file=/dev/null", "-e", `const { persistence } = await import(${JSON.stringify(persistencePath)}); const result = await persistence.transaction(tx => tx.claim_delivery_activation(${JSON.stringify(binding)}, ${JSON.stringify(scanTime)}, ${JSON.stringify(profile)})); console.log(JSON.stringify(result)); process.exit(0);`], { env: { ...Bun.env }, stdout: "pipe", stderr: "pipe" });
    const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
    expect(code).toBe(0); expect(err).toBe(""); const recovered = JSON.parse(out);
    expect(recovered.generation).toBe("2"); expect(recovered.activationId).not.toBe(first.activationId); expect(recovered.page.intentIds).toEqual(ids); expect(recovered.operationTime).toBe(scanTime);
    expect(await stage(first, [])).toBe(false); expect(await finish(first)).toBe(false);
    expect(await finish(recovered)).toBe(true);
  });
  test("renewal preserves exact page/cursor/fence/profile/deadline, ignores forged handle deadlines and rolls back with its transaction", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant); const handle = await claim(binding, instant);
    await stage(handle, [crypto.randomUUID()], { dueAt: dueA, id: crypto.randomUUID() });
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', [new Date(Date.now() + 10_000).toISOString(), binding]);
    const before = await state(binding);
    await expect(persistence.transaction(async (tx: any) => { expect(await tx.renew_delivery_activation(handle)).not.toBeNull(); throw new Error("renew rollback"); })).rejects.toThrow("renew rollback");
    expect(await state(binding)).toEqual(before);
    const renewed = await renew({ ...handle, leaseUntil: "bad", executionDeadline: "9999-12-31T23:59:59.999Z", leaseMs: 999_999 });
    expect(renewed.leaseUntil > before.lease_until).toBe(true);
    expect(renewed.executionDeadline).toBe(handle.executionDeadline);
    const after = await state(binding);
    expect({ ...after, lease_until: before.lease_until }).toEqual(before);
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET execution_deadline=$1, lease_until=$1 WHERE binding=$2', [new Date(Date.now() + 20_000).toISOString(), binding]);
    const capped = await renew(handle); expect(capped.leaseUntil).toBe(capped.executionDeadline);
    await expect(persistence.renew_delivery_activation(handle)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
  });
  test("expired/stale/wrong-generation renewal never changes the replacement activation or its immutable work", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant); const first = await claim(binding, instant);
    await stage(first, [crypto.randomUUID()]);
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", binding]);
    const expired = await state(binding); expect(await renew(first)).toBeNull(); expect(await state(binding)).toEqual(expired);
    const replacement = await claim(binding, scanTime), before = await state(binding);
    expect(replacement.generation).toBe("2"); expect(replacement.page).toEqual(first.page ?? JSON.parse(expired.page));
    for (const wrong of [first, { ...replacement, activationId: crypto.randomUUID() }, { ...replacement, generation: "9007199254740993" }]) {
      expect(await renew(wrong)).toBeNull(); expect(await state(binding)).toEqual(before);
    }
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1,execution_deadline=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", binding]);
    const exhausted = await state(binding); expect(await renew(replacement)).toBeNull(); expect(await state(binding)).toEqual(exhausted);
  });
  test("unmodified native renewal SQL refuses delayed statement/ack authority and preserves an ahead-of-clock old lease", async () => {
    // The factory body is the exact compiler runtime template, evaluated with
    // real adapter SQL and fault injection around the renewal driver promise.
    // Generated scheduler integration above uses untouched generated modules.
    const source = readFileSync(join(repository, "jadpo/crates/core/src/runtime/delivery_scheduler.ts"), "utf8");
    const compiled = new Bun.Transpiler({ loader: "ts" }).transformSync(source);
    class HarnessFault extends Error { constructor(public operation: string) { super(operation); } }
    const factory = new Function("PersistenceFault", `${compiled}\nreturn createDeliverySchedulePrimitives;`)(HarnessFault);
    const memory = sql === null ? new Database(":memory:") : null;
    let mode: "before" | "after" | null = null;
    const transact = async (work: (native: any, execute: any) => Promise<any>) => {
      const run = async (connection: any) => {
        const execute = async (operation: string, statement: string, values: any[] = []) => {
          if (operation === "delivery.activation.renew" && mode === "before") await Bun.sleep(150);
          const result = sql ? await connection.unsafe(statement, values) : memory!.prepare(statement).all(...values);
          if (operation === "delivery.activation.renew" && mode === "after") await Bun.sleep(150);
          return result;
        };
        const native = factory(execute, () => {}, sql !== null, (nested: any) => nested());
        return work(native, execute);
      };
      if (sql) return sql.begin(run);
      memory!.exec("BEGIN IMMEDIATE");
      try { const result = await run(memory); memory!.exec("COMMIT"); return result; }
      catch (error) { memory!.exec("ROLLBACK"); throw error; }
    };
    try {
      for (const delayed of ["before", "after"] as const) {
        const binding = crypto.randomUUID(), duration = { executionMs: 1_000, leaseMs: 80 };
        const first = await transact(async native => { await native.tick_delivery_schedule(binding, interval, instant); const handle = await native.claim_delivery_activation(binding, instant, duration); await native.stage_delivery_activation_page(handle, [binding], null); return handle; });
        const before = await transact((_native, execute) => execute("read", 'SELECT * FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [binding]));
        mode = delayed;
        expect(await transact(native => native.renew_delivery_activation(first))).toBeNull();
        mode = null;
        const after = await transact((_native, execute) => execute("read", 'SELECT * FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [binding]));
        if (delayed === "before") expect(after).toEqual(before);
        else expect({ ...after[0], lease_until: before[0].lease_until }).toEqual(before[0]);
        expect(await transact(native => native.finish_delivery_activation(first))).toBe(false);
        const replacement = await transact(native => native.claim_delivery_activation(binding, scanTime, duration));
        expect(replacement.generation).toBe("2"); expect(replacement.page.intentIds).toEqual([binding]);
        expect(await transact(native => native.renew_delivery_activation(first))).toBeNull();
      }
      const binding = crypto.randomUUID();
      const first = await transact(async (native, execute) => {
        await native.tick_delivery_schedule(binding, interval, instant);
        const handle = await native.claim_delivery_activation(binding, instant, profile);
        await execute("clock-regression-fixture", 'UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', [handle.executionDeadline, binding]);
        return handle;
      });
      const before = await transact((_native, execute) => execute("read", 'SELECT * FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [binding]));
      expect((await transact(native => native.renew_delivery_activation(first))).leaseUntil).toBe(first.executionDeadline);
      expect(await transact((_native, execute) => execute("read", 'SELECT * FROM "__jadpo_delivery_activations_v1" WHERE binding=$1', [binding]))).toEqual(before);
    } finally { memory?.close(); }
  });
  test.skipIf(sql === null)("PostgreSQL renewal row-lock wait past the old lease returns no authority and cannot change the durable work", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant);
    const short = { executionMs: 5_000, leaseMs: 250 };
    const first = await persistence.transaction((tx: any) => tx.claim_delivery_activation(binding, instant, short));
    await stage(first, [crypto.randomUUID()]); const before = await state(binding);
    let release!: () => void, entered!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; }), observed = new Promise<void>(resolve => { entered = resolve; });
    const blocker = sql!.begin(async tx => {
      await tx.unsafe('UPDATE "__jadpo_delivery_activations_v1" SET generation=generation WHERE binding=$1', [binding]);
      entered(); await held;
    });
    await observed;
    const pending = renew(first);
    try { await Bun.sleep(350); }
    finally { release(); await blocker; }
    expect(await pending).toBeNull(); expect(await state(binding)).toEqual(before);
    const replacement = await claim(binding, scanTime);
    expect(replacement.generation).toBe("2"); expect(replacement.page).toEqual(JSON.parse(before.page));
    expect(await renew(first)).toBeNull();
  });
  test("tick/claim/page rollback retain durable state and configuration mismatch refuses", async () => {
    const binding = crypto.randomUUID();
    await expect(persistence.transaction(async (tx: any) => { await tx.tick_delivery_schedule(binding, interval, instant); throw new Error("tick rollback"); })).rejects.toThrow("tick rollback");
    expect(await state(binding)).toBeUndefined(); await tick(binding, instant);
    await expect(persistence.transaction((tx: any) => tx.tick_delivery_schedule(binding, interval * 2, instant))).rejects.toMatchObject({ operation: "delivery.activation_interval" });
    await expect(persistence.transaction(async (tx: any) => { await tx.claim_delivery_activation(binding, instant, profile); throw new Error("claim rollback"); })).rejects.toThrow("claim rollback");
    expect((await state(binding)).activation_id).toBeNull();
    expect(await claim(binding, instant)).not.toBeNull();
    await expect(persistence.tick_delivery_schedule(binding, interval, instant)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
  });
  test("corrupt persisted Instant authority and incoherent active state refuse atomically instead of granting a stale lease", async () => {
    const cases: Record<string, unknown>[] = [
      { lease_until: "not_an_instant" }, { execution_deadline: "not_an_instant" },
      { lease_until: "not_an_instant", execution_deadline: "not_an_instant" },
      { lease_until: null }, { execution_deadline: null }, { operation_time: null }, { scheduled_for: null },
      { lease_until: "2026-10-05T08:00:00Z" }, { execution_deadline: "2026-10-05T08:00:00.000+00:00" },
      { lease_until: "2026-02-30T08:00:00.000Z" }, { execution_deadline: "99999-10-05T08:00:00.000Z" },
      { operation_time: "0000-01-01T00:00:00.000Z" },
      { lease_until: "2099-01-01T00:00:00.000Z", execution_deadline: "2098-01-01T00:00:00.000Z" },
      { activation_id: null }, { continuation_for: instant, cursor: null },
      { lease_ms: "9007199254740992" },
    ];
    for (const corrupt of cases) {
      const binding = crypto.randomUUID(); await tick(binding, instant); const handle = await claim(binding, instant);
      await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", binding]);
      const entries = Object.entries(corrupt), assignments = entries.map(([field], i) => `"${field}"=$${i + 1}`).join(",");
      await rows(`UPDATE "__jadpo_delivery_activations_v1" SET ${assignments} WHERE binding=$${entries.length + 1}`, [...entries.map(([, value]) => value), binding]);
      const before = await state(binding);
      for (const method of [() => claim(binding, scanTime), () => stage(handle, [crypto.randomUUID()]), () => finish(handle), () => renew(handle)]) {
        await expect(method()).rejects.toBeDefined(); expect(await state(binding)).toEqual(before);
      }
    }
  });
  test.skipIf(sql !== null)("SQLite TEXT/REAL duration corruption refuses claim, page, finish and renewal before native SQL interprets it", async () => {
    for (const field of ["lease_ms", "interval_ms"]) {
      for (const corrupt of ["0x10", "0b10", "NaN", "duration", 1.5]) {
        const binding = crypto.randomUUID(); await tick(binding, instant); const handle = await claim(binding, instant);
        await stage(handle, [crypto.randomUUID()]);
        await rows(`UPDATE "__jadpo_delivery_activations_v1" SET ${field}=$1 WHERE binding=$2`, [corrupt, binding]);
        const type = (await rows(`SELECT typeof(${field}) AS storage_type FROM "__jadpo_delivery_activations_v1" WHERE binding=$1`, [binding]))[0].storage_type;
        expect(type).toBe(typeof corrupt === "string" ? "text" : "real");
        const before = await state(binding);
        for (const operation of [() => claim(binding, scanTime), () => stage(handle, []), () => finish(handle), () => renew(handle)]) {
          await expect(operation()).rejects.toMatchObject({ operation: "delivery.activation_state" });
          expect(await state(binding)).toEqual(before);
        }
      }
    }
  });
  test("durable Instant decoding preserves the portable years0001 and9999 boundaries", async () => {
    for (const at of ["0001-01-01T00:00:00.000Z", "9999-12-31T23:59:59.999Z"]) {
      const binding = crypto.randomUUID(); await tick(binding, at); const handle = await claim(binding, at);
      expect(handle.operationTime).toBe(at);
      expect(await stage(handle, [])).toBe(true);
      expect(await finish(handle)).toBe(true);
      expect((await state(binding)).activation_id).toBeNull();
    }
  });
  test("legacy schema migration preserves live authority without inventing a renewal duration; expired reclaim installs the explicit profile", async () => {
    const binding = crypto.randomUUID(); await tick(binding, instant); const first = await claim(binding, instant);
    await stage(first, [crypto.randomUUID()]); const before = await state(binding);
    await rows('ALTER TABLE "__jadpo_delivery_activations_v1" DROP COLUMN lease_ms');
    await tick(binding, instant);
    expect(await state(binding)).toEqual({ ...before, lease_ms: null });
    expect(await renew(first)).toBeNull(); expect(await claim(binding, scanTime)).toBeNull();
    expect(await state(binding)).toEqual({ ...before, lease_ms: null });
    await rows('UPDATE "__jadpo_delivery_activations_v1" SET lease_until=$1 WHERE binding=$2', ["2000-01-01T00:00:00.000Z", binding]);
    const replacement = await claim(binding, scanTime);
    expect(Number((await state(binding)).lease_ms)).toBe(profile.leaseMs);
    expect(replacement.page).toEqual(JSON.parse(before.page)); expect(await renew(first)).toBeNull();
    expect(await renew(replacement)).not.toBeNull();
  });
});

describe(`opaque-binding generated schedule hook component (${sql ? "postgres" : "sqlite"})`, () => {
  test("public ordinary target encodes owning-reference Instant patch input with its actual destination representation", async () => {
    const id = crypto.randomUUID();
    await ordinary.create_Alarm({ id, due: dueA });
    await ordinary.update_required_Alarm_by_id_patch_due(id, { due: dueB });
    expect(new Date((await rows('SELECT due FROM "alarm" WHERE id=$1', [id]))[0].due as any).toISOString()).toBe(dueB);
    await expect(ordinary.update_required_Alarm_by_id_patch_due(id, { due: "invalid-instant" })).rejects.toMatchObject({ operation: "encode.instant" });
    expect(new Date((await rows('SELECT due FROM "alarm" WHERE id=$1', [id]))[0].due as any).toISOString()).toBe(dueB);
    await ordinary.update_required_Alarm_by_id_patch_due(id, { due: null });
    expect((await rows('SELECT due FROM "alarm" WHERE id=$1', [id]))[0].due).toBeNull();
  });
  test("complete checked binding remains nonexecuting and unrelated entities have no schedule hook", async () => {
    const binding = JSON.parse(readFileSync(join(root, "binding.json"), "utf8"));
    expect(binding.runtime_lowering_supported).toBe(false);
    expect(binding.execution_profile).toBeNull();
    expect(binding.live_authority_established).toBe(false);
    expect(await client.transaction((tx: any) => tx.read_delivery_schedule_revision("User", owner))).toBeNull();
  });
  test("successful actual generated create establishes revision1 atomically and duplicate creation cannot reset it", async () => {
    const value = todo();
    await client.create_Todo(value);
    expect(await revision(value.id)).toBe("1");
    await expect(client.create_Todo(value)).rejects.toBeDefined();
    expect(await revision(value.id)).toBe("1");
    const rolled = todo();
    await expect(client.transaction(async (tx: any) => { await tx.create_Todo(rolled); throw new Error("abort create"); })).rejects.toThrow("abort create");
    expect(await stored(rolled.id)).toEqual([]);
    expect(await revision(rolled.id)).toBeNull();
  });
  test("supplied due, equal-value due and explicit clear all advance and atomically clear sent", async () => {
    const value = todo(); await client.create_Todo(value);
    for (const [due, expected] of [[dueB, "2"], [dueB, "3"], [null, "4"]]) {
      await setSent(value.id);
      expect(await patch(value.id, { due_at: due })).not.toBeNull();
      expect(await revision(value.id)).toBe(expected);
      expect((await stored(value.id))[0].reminder_sent_at).toBeNull();
    }
  });
  test("omitted due and disjoint generated status writes preserve schedule and sent", async () => {
    const value = todo(); await client.create_Todo(value); await setSent(value.id);
    const before = (await stored(value.id))[0];
    await patch(value.id, { title: "changed title" });
    await patch(value.id, { title: "changed title" });
    await client.update_required_Todo_by_id_set_status(value.id, "done");
    expect(await revision(value.id)).toBe("1");
    const after = (await stored(value.id))[0];
    expect(after.due_at).toEqual(before.due_at);
    expect(after.reminder_sent_at).toEqual(before.reminder_sent_at);
  });
  test("failed encoding, missing rows and source rollback neither advance nor clear sent", async () => {
    const value = todo(); await client.create_Todo(value); await setSent(value.id);
    const before = await stored(value.id);
    await expect(patch(value.id, { due_at: "not-an-instant" })).rejects.toMatchObject({ operation: "encode.instant" });
    const absent = crypto.randomUUID();
    expect(await patch(absent, { due_at: dueB })).toBeNull();
    expect(await revision(absent)).toBeNull();
    await expect(client.transaction(async (tx: any) => { await tx[method](value.id, { due_at: dueB }, null); throw new Error("abort patch"); })).rejects.toThrow("abort patch");
    expect(await revision(value.id)).toBe("1");
    expect(await stored(value.id)).toEqual(before);
  });
  test("unchanged ordinary owner policy refuses another principal without creating or advancing private state", async () => {
    const value = todo(); await client.create_Todo(value); await setSent(value.id);
    const before = await stored(value.id);
    const other = crypto.randomUUID();
    const denied = client.withPolicy({ kind: "user", subject: other, values: { user_id: other } }, "Todo.patch_todo");
    expect(await denied[method](value.id, { due_at: dueB }, null)).toBeNull();
    expect(await revision(value.id)).toBe("1");
    expect(await stored(value.id)).toEqual(before);
    const absent = todo();
    await expect(denied.create_Todo(absent)).rejects.toMatchObject({ operation: "Todo.patch_todo" });
    expect(await stored(absent.id)).toEqual([]);
    expect(await revision(absent.id)).toBeNull();
  });
  test("A-to-B-to-A retains distinct business revisions even when the value returns", async () => {
    const value = todo(); await client.create_Todo(value);
    await patch(value.id, { due_at: dueB }); expect(await revision(value.id)).toBe("2");
    await patch(value.id, { due_at: dueA }); expect(await revision(value.id)).toBe("3");
    const row = (await stored(value.id))[0];
    expect(new Date(row.due_at as any).toISOString()).toBe(dueA);
  });
  test("caught hook exhaustion rolls back the source patch and sent reset, not surrounding source work", async () => {
    const value = todo(); await client.create_Todo(value); await setSent(value.id);
    await rows('UPDATE "__jadpo_delivery_schedules_v1" SET schedule_revision = 9223372036854775807 WHERE source_entity_id = $1', [value.id]);
    const before = await stored(value.id);
    await client.transaction(async (tx: any) => {
      await expect(tx[method](value.id, { due_at: dueB }, null)).rejects.toMatchObject({ operation: "delivery.schedule_missing_or_exhausted" });
      await tx.update_required_Todo_by_id_set_title(value.id, "surrounding work");
    });
    const after = (await stored(value.id))[0];
    expect(after.title).toBe("surrounding work");
    expect(after.due_at).toEqual(before[0].due_at);
    expect(after.reminder_sent_at).toEqual(before[0].reminder_sent_at);
    expect(await revision(value.id)).toBe("9223372036854775807");
  });
  test("caught schedule identity collision rolls back successful INSERT inside its create savepoint", async () => {
    const value = todo();
    await client.transaction((tx: any) => tx.establish_delivery_schedule_revision("Todo", value.id));
    await client.transaction(async (tx: any) => {
      await expect(tx.create_Todo(value)).rejects.toMatchObject({ operation: "delivery.schedule_identity_conflict" });
      expect(await tx.read_delivery_schedule_revision("Todo", value.id)).toBe("1");
    });
    expect(await stored(value.id)).toEqual([]);
    expect(await revision(value.id)).toBe("1");
  });
  test("actual generated patches serialize concurrent revisions and survive fresh process restart", async () => {
    const value = todo(); await client.create_Todo(value);
    const changed = await Promise.all([patch(value.id, { due_at: dueB }), patch(value.id, { due_at: dueA })]);
    expect(changed.every(row => row !== null)).toBe(true);
    expect(await revision(value.id)).toBe("3");
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", `const {persistence}=await import(${JSON.stringify(pathToFileURL(persistencePath).href)});console.log(JSON.stringify(await persistence.transaction(tx=>tx.read_delivery_schedule_revision("Todo",${JSON.stringify(value.id)}))));process.exit(0);`], { env: { ...Bun.env, DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath }, stdout: "pipe", stderr: "pipe" });
    const [out, err, exit] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
    expect(exit, err).toBe(0);
    expect(JSON.parse(out)).toBe("3");
  });
});
