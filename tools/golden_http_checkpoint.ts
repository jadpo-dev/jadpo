// Real migrated-application observations for the bounded HTTP slice.
// Receives the full frozen contract; never reads expected values to emit results.
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { beginSqlObservation, endSqlObservation, recordStatement, authenticationAttemptCount, authenticatedPrincipals, issuedCredentialCount } from "./golden_sql_observer.ts";

const supported = ["AUTH-001", "AUTH-005", "AUTH-012", "PUBLIC-001", "CREATE-001", "CREATE-002", "CREATE-003", "CREATE-004", "READ-001", "READ-002", "READ-003", "LIST-001", "LIST-002", "LIST-003", "LIST-004", "REL-001", "PATCH-001", "PATCH-002", "PATCH-003", "PATCH-004", "PATCH-005", "DELETE-001", "DELETE-002", "AUTH-002", "AUTH-006", "AUTH-010", "USER-001"];
const [project, backend, contractPath, output] = Bun.argv.slice(2);
if (!project || !["sqlite", "postgres"].includes(backend) || !output) throw new Error("Missing checkpoint arguments");
const contract = JSON.parse(readFileSync(contractPath, "utf8"));
const nativeDate = globalThis.Date;
let clockNow = nativeDate.parse(contract.clock);
if (!Number.isFinite(clockNow)) throw new Error("Missing frozen fixture clock");
// Existing runtime suites use the same controlled Date seam. Only time reads
// change; real handlers, SQL, authentication and generated validators execute.
globalThis.Date = new Proxy(nativeDate, {
  construct(target, arguments_) { return Reflect.construct(target, arguments_.length ? arguments_ : [clockNow]); },
  apply() { return new nativeDate(clockNow).toString(); },
  get(target, property) { return property === "now" ? () => clockNow : Reflect.get(target, property); },
});
const prototype = Database.prototype as any;
const originalPrepare = prototype.prepare;
const originalExec = prototype.exec;
prototype.exec = function(sql: string) { recordStatement(sql); return originalExec.call(this, sql); };
prototype.prepare = function(sql: string, ...options: unknown[]) {
  const statement = originalPrepare.call(this, sql, ...options);
  return new Proxy(statement, { get(target, property) {
    const member = Reflect.get(target, property, target);
    if (["all", "get", "run", "values"].includes(String(property))) return (...values: unknown[]) => {
      recordStatement(sql, values); return member.apply(target, values);
    };
    return typeof member === "function" ? member.bind(target) : member;
  } });
};
if (backend === "sqlite") { Bun.env.SQLITE_PATH = join(project, "checkpoint.sqlite"); delete Bun.env.DATABASE_URL; }
else { if (!Bun.env.DATABASE_URL) throw new Error("Missing isolated PostgreSQL URL"); delete Bun.env.SQLITE_PATH; }
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const routes = JSON.parse(readFileSync(join(project, "build/inventory/routes.json"), "utf8")).routes;
const queryPlans = JSON.parse(readFileSync(join(project, "build/persistence/entities.json"), "utf8")).query_plans.filter((plan: any) => plan.entity === "Todo" && plan.strategy === "indexed_keyset_page");
const declaredFailures = JSON.parse(readFileSync(join(project, "build/audit/failures.json"), "utf8")).failures;
const db = backend === "sqlite" ? new Database(Bun.env.SQLITE_PATH, { strict: true }) : undefined;
const pg = backend === "postgres" ? new SQL({ url: Bun.env.DATABASE_URL!, prepare: false }) : undefined;
async function execute(sql: string, values: unknown[] = []) {
  if (pg) return await pg.unsafe(sql, values);
  const statement = db!.prepare(sql.replace(/\$(\d+)/gu, "?$1"));
  try { return statement.all(...values); } finally { statement.finalize(); }
}
const alice = "00000000-0000-4000-8000-000000000001";
const bob = "00000000-0000-4000-8000-000000000002";
const t2 = "00000000-0000-4000-8000-000000000022";
const t1 = "00000000-0000-4000-8000-000000000021";
const environment = {
  DATABASE_URL: "https://db.test/golden", SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: "https://issuer.test", OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"), MAIL_SENDER: "todo@example.test",
};
let session: any;
let bearer: any;
let issuanceObservation: any;
let secrets: string[] = Object.values(environment).filter(value => value === environment.SESSION_SIGNING_KEY || value === environment.MAIL_API_KEY);
async function reset(case_?: any) {
  clockNow = nativeDate.parse(case_?.timeline?.issuedAt ?? contract.clock);
  await app.initializeApplication(environment);
  for (const table of ["service_credential", "__jadpo_auth_service_credentials", "service", "todo", "__jadpo_auth_sessions", "user"]) await execute(`DELETE FROM "${table}"`);
  const now = Date.now();
  for (const [id, subject] of [[alice, "alice"], [bob, "bob"]]) await execute('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [id, subject, `${subject}@example.test`, "active", pg ? new Date(now).toISOString() : now]);
  const statements = beginSqlObservation();
  try { session = await app.authenticationHost().issue("browser_session", "alice", case_?.timeline ? nativeDate.parse(case_.timeline.credentialExpiresAt) : now + 300_000, now); }
  finally { endSqlObservation(); }
  issuanceObservation = { operation: "compiler-owned authentication issuance", strategy: "browser_session", statements, credentialsIssued: issuedCredentialCount() };
  if (case_?.id === "AUTH-010") {
    const statements = beginSqlObservation();
    try { bearer = await app.authenticationHost().issue("api_bearer", "alice", now + 300_000, now); }
    finally { endSqlObservation(); }
    issuanceObservation = { operation: "compiler-owned authentication issuance", strategy: "api_bearer", statements, credentialsIssued: issuedCredentialCount() };
    secrets.push(bearer.credential, bearer.csrfToken);
  }
  secrets = [...secrets, session.credential, session.csrfToken];
}
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
const raw: any[] = [];
const persistedInstant = (value: unknown) => value === null ? null : new Date(value as any).toISOString();
async function userSnapshot() {
  const rows = await execute('SELECT id, authentication_subject, status, disabled_at FROM "user" WHERE id = $1', [alice]);
  if (rows.length !== 1) throw new Error("Missing authentication user fixture");
  return { ...rows[0], disabled_at: persistedInstant(rows[0].disabled_at) };
}
async function sessionSnapshot(strategy = "browser_session") {
  const rows = await execute('SELECT data, revoked FROM "__jadpo_auth_sessions"');
  const records = rows.map((row: any) => ({ data: typeof row.data === "string" ? JSON.parse(row.data) : row.data, revoked: row.revoked }));
  const record = records.find((row: any) => row.data.strategy === strategy);
  if (!record) throw new Error("Missing otherwise valid native session record");
  const { data } = record;
  return { strategy: data.strategy, subject: data.subject, userId: data.userId,
    expires: new Date(data.expires).toISOString(), revoked: record.revoked === true || record.revoked === 1 || data.revoked };
}
async function captureAuthenticationFixture(case_: any) {
  const credential = case_.id === "AUTH-010" ? bearer : session;
  raw.push({ authenticationFixture: { user: await userSnapshot(), sessionRecord: await sessionSnapshot(case_.id === "AUTH-010" ? "api_bearer" : "browser_session"),
    issuedAt: new Date(clockNow).toISOString(), credentialExpiresAt: new Date(credential.expires).toISOString(),
    issuanceObservation, specification: { fixture: case_.fixture, timeline: case_.timeline ?? null } } });
}
function authorityQueryAccounting(statements: any[]) {
  return { principal: statements.flatMap((row, index) => row.operation === "SELECT" && /FROM "user" WHERE "authentication_subject"/u.test(row.sql) ? [index] : []),
    credential: statements.flatMap((row, index) => row.operation === "SELECT" && /FROM "__jadpo_auth_sessions"/u.test(row.sql) ? [index] : []) };
}
async function refresh(request: any) {
  if (request.operation !== "compiler-owned authentication refresh" || request.auth !== "alice-session") throw new Error("Unsupported refresh operation");
  const statements = beginSqlObservation();
  let fault: any = null;
  let completed = false;
  try {
    const credential = await app.authenticationHost().refresh("browser_session", session.credential, clockNow);
    secrets.push(credential.credential, credential.csrfToken); completed = true;
  } catch (error: any) {
    // Retain only the closed native fault classification, never error text.
    fault = { failure: error.declaredFailure ?? null, nativeCode: error.code ?? null, status: error.status ?? null };
  } finally { endSqlObservation(); }
  const failureMetadata = declaredFailures.find((item: any) => item.failure === fault?.failure) ?? null;
  const captured = { operation: request.operation, auth: request.auth, at: new Date(clockNow).toISOString(), completed, fault, failureMetadata,
    statements, authenticationAttempts: authenticationAttemptCount(), principals: authenticatedPrincipals(), credentialsIssued: issuedCredentialCount(), authorityQueries: authorityQueryAccounting(statements) };
  if (secrets.some(secret => JSON.stringify(captured).includes(secret))) throw new Error("Unsafe refresh observation refused");
  raw.push(captured);
}
async function userTimeline(case_: any) {
  await seedTodo(t1, alice);
  raw.push({ disableFixture: { user: await userSnapshot(), todo: await todoSnapshot(t1) } });
  clockNow = nativeDate.parse(case_.timeline.disabledAt);
  await send(case_.request);
  raw.push({ disabledPersistence: { user: await userSnapshot(), todo: await todoSnapshot(t1), sessionRecord: await sessionSnapshot() } });
  for (const followUp of case_.followUps) {
    clockNow = nativeDate.parse(followUp.at);
    if (followUp.request) await send(followUp.request);
    else await refresh(followUp);
  }
}
async function todoSnapshot(id: string) {
  const rows = await execute('SELECT id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [id]);
  if (rows.length !== 1) throw new Error("Missing fixture/persistence row");
  const row = rows[0];
  return { ...row, due_at: persistedInstant(row.due_at), reminder_sent_at: persistedInstant(row.reminder_sent_at), created_at: persistedInstant(row.created_at), updated_at: persistedInstant(row.updated_at), deleted_at: persistedInstant(row.deleted_at) };
}
async function seedTodo(id: string, owner: string, dueAt: string | null = null, options: { createdAt?: string; status?: string; deletedAt?: string } = {}) {
  const createdAt = options.createdAt ?? "2026-01-15T11:00:00Z";
  await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, $5, NULL, $6, $6, $7)', [id, owner, "original fixture title", options.status ?? "open", dueAt === null ? null : pg ? dueAt : Date.parse(dueAt), pg ? createdAt : Date.parse(createdAt), options.deletedAt === undefined ? null : pg ? options.deletedAt : Date.parse(options.deletedAt)]);
}
async function captureCollectionFixture() {
  const ids = await execute('SELECT id FROM todo ORDER BY id');
  const rows = [];
  for (const row of ids) rows.push(await todoSnapshot(row.id));
  raw.push({ fixtureRows: rows, queryMetadata: queryPlans });
}
async function validateOutput(request: any, reply: any) {
  const route = routes.find((route: any) => route.method === request.method && new RegExp("^" + route.path.replace(/\{[^}]+\}/gu, "[^/]+") + "$", "u").test(request.path));
  if (!route?.output) throw new Error("Missing generated route output");
  const validator = `validate_${route.output}`;
  if (typeof app[validator] !== "function") throw new Error("Missing exported generated output validator");
  try { raw.push({ outputValidation: { route: route.route, schema: route.output, validator, accepted: true, validatedBody: app[validator](reply.body, "checkpoint.response") } }); }
  catch { raw.push({ outputValidation: { route: route.route, schema: route.output, validator, accepted: false } }); }
}
async function send(request: any) {
  const headers: Record<string, string> = {};
  const auth = Array.isArray(request.auth) ? request.auth : [request.auth];
  if (auth.includes("alice-session")) Object.assign(headers, { cookie: session.setCookie.split(";", 1)[0], origin: environment.BROWSER_ORIGIN, "x-jadpo-csrf": session.csrfToken });
  if (auth.includes("alice-bounded-bearer")) headers.authorization = `Bearer ${bearer.credential}`;
  if (auth.includes("invalid-jwt") || auth.includes("invalid-bearer")) headers.authorization = "Bearer INVALID_CHECKPOINT_CREDENTIAL";
  const path = request.path.replace("/todos/t2", `/todos/${t2}`).replace("/todos/t1", `/todos/${t1}`).replace("/users/alice/todos", `/users/${alice}/todos`).replace(/^\/users\/alice$/u, `/users/${alice}`);
  if (request.json !== undefined) headers["content-type"] = "application/json";
  const statements = beginSqlObservation();
  let response: Response;
  let body: any;
  try {
    response = await fetch(new URL(path, server.url), { method: request.method, headers, ...(request.json === undefined ? {} : { body: JSON.stringify(request.json) }) });
    const text = await response.text();
    body = text.length ? JSON.parse(text) : null;
  } finally { endSqlObservation(); }
  const authenticationAttempts = authenticationAttemptCount();
  const authorityQueries = authorityQueryAccounting(statements);
  const captured = { method: request.method, path, requestedPath: request.requestedPath ?? request.path, auth: request.auth ?? null, requestJson: request.json ?? null, at: new Date(clockNow).toISOString(), status: response!.status, body, statements, authenticationAttempts, principals: authenticatedPrincipals(), credentialsIssued: issuedCredentialCount(), authorityQueries };
  if (secrets.some(secret => JSON.stringify(captured).includes(secret))) throw new Error("Unsafe raw observation refused");
  raw.push(captured);
  return { status: response!.status, code: body?.error?.code, body, databaseQueries: statements.length, authenticationAttempts, writes: statements.filter(row => ["INSERT", "UPDATE", "DELETE", "REPLACE"].includes(row.operation)).length };
}
const results: any[] = [];
try {
  for (const case_ of contract.cases.filter((case_: any) => supported.includes(case_.id))) {
    if ((case_.fixtureVariants && case_.id !== "REL-001") || (case_.followUps && case_.id !== "USER-001")) throw new Error("Unsupported scenario extension");
    if (case_.id === "REL-001") {
      const scenarios = [];
      for (const [variant_index, fixture_variant] of case_.fixtureVariants.entries()) {
        const start = raw.length;
        try {
          const count = ({ "alice zero todos": 0, "alice one todo": 1, "alice 100 todos": 100 } as Record<string, number>)[String(fixture_variant)];
          if (count === undefined) throw new Error("Unsupported relationship fixture variant");
          await reset();
          for (let index = 0; index < count; index++) await seedTodo(`00000000-0000-4000-8000-${String(1000 + index).padStart(12, "0")}`, alice, null, { createdAt: new Date(clockNow - 500_000 + Math.floor(index / 2)).toISOString() });
          await seedTodo(t2, bob, null, { createdAt: "2026-01-01T00:00:00Z" });
          await seedTodo(t1, alice, null, { deletedAt: "2026-01-15T11:59:00Z" });
          await captureCollectionFixture();
          const reply = await send(case_.request); await validateOutput(case_.request, reply);
          scenarios.push({ variant_index, fixture_variant, raw: raw.slice(start) });
        } catch { scenarios.push({ variant_index, fixture_variant, adapterFailure: true, raw: raw.slice(start) }); }
      }
      results.push({ id: case_.id, scenarios });
      continue;
    }
    const start = raw.length;
    try {
    await reset(case_);
    if (["AUTH-002", "AUTH-006", "AUTH-010", "USER-001"].includes(case_.id)) await captureAuthenticationFixture(case_);
    if (case_.id === "USER-001") { await userTimeline(case_); results.push({ id: case_.id, raw: raw.slice(start) }); continue; }
    if (case_.id === "AUTH-006") {
      clockNow = nativeDate.parse(case_.timeline.disabledAt);
      await execute('UPDATE "user" SET status = $1, disabled_at = $2 WHERE id = $3', ["disabled", pg ? new Date(clockNow).toISOString() : clockNow, alice]);
      raw.push({ disabledUser: await userSnapshot(), at: new Date(clockNow).toISOString(), sessionRecord: await sessionSnapshot() });
      clockNow = nativeDate.parse(case_.timeline.requestAt);
    }
    if (case_.id.startsWith("PATCH-") || ["READ-001", "DELETE-001", "DELETE-002"].includes(case_.id)) {
      const id = case_.id === "PATCH-004" ? t2 : t1;
      await seedTodo(id, case_.id === "PATCH-004" ? bob : alice, ["PATCH-001", "PATCH-002", "PATCH-005"].includes(case_.id) ? "2026-02-01T00:00:00Z" : null);
      if (case_.id === "PATCH-005") await execute('UPDATE todo SET reminder_sent_at = $1 WHERE id = $2', [pg ? "2026-01-15T11:59:00Z" : Date.parse("2026-01-15T11:59:00Z"), id]);
      if (case_.id === "DELETE-002") await execute('UPDATE todo SET deleted_at = $1 WHERE id = $2', [pg ? "2026-01-15T11:59:00Z" : Date.parse("2026-01-15T11:59:00Z"), id]);
      raw.push({ fixture: await todoSnapshot(id) });
    }
    if (["LIST-001", "LIST-002"].includes(case_.id)) {
      for (let index = 0; index < 5; index++) await seedTodo(`00000000-0000-4000-8000-${String(100 + index).padStart(12, "0")}`, alice, null, { createdAt: case_.id === "LIST-001" ? "2026-01-15T11:00:00Z" : new Date(clockNow - 10000 + Math.floor(index / 2)).toISOString() });
      if (case_.id === "LIST-001") {
        await seedTodo(t2, bob); await seedTodo(t1, alice, null, { deletedAt: "2026-01-15T11:59:00Z" });
      }
      await captureCollectionFixture();
    }
    if (case_.id === "LIST-003") {
      await seedTodo(t1, alice, "2026-01-30T00:00:00Z");
      await seedTodo(t2, bob, "2026-01-30T00:00:00Z");
      await seedTodo("00000000-0000-4000-8000-000000000023", alice, "2026-01-30T00:00:00Z", { status: "completed" });
      await seedTodo("00000000-0000-4000-8000-000000000024", alice, "2026-02-02T00:00:00Z");
      await seedTodo("00000000-0000-4000-8000-000000000025", alice);
      await captureCollectionFixture();
    }
    if (case_.id === "LIST-002") {
      const replies = [];
      for (const request of case_.requests) {
        let path = request.path;
        if (path.includes("{first.next}")) path = path.replace("{first.next}", encodeURIComponent(JSON.stringify(replies[0].body.next)));
        if (path.includes("{second.next}")) path = path.replace("{second.next}", encodeURIComponent(JSON.stringify(replies[1].body.next)));
        replies.push(await send({ ...request, path, requestedPath: request.path }));
      }
      await send(case_.requests[0]);
    } else if (case_.id === "READ-002") {
      const now = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)', [t2, bob, "private bob todo", "open", pg ? new Date(now).toISOString() : now]);
      for (const request of case_.requests) await send(request);
    } else {
      const reply = await send(case_.request);
      if (["CREATE-001", "READ-001", "AUTH-002"].includes(case_.id)) await validateOutput(case_.request, reply);
      if (case_.id === "CREATE-004") {
        const rows = await execute('SELECT due_at FROM todo WHERE id = $1', [reply.body.id]);
        if (rows.length !== 1) throw new Error("Missing persisted create observation");
        raw.push({ persistence: { id: reply.body.id, storedDueAt: rows[0].due_at } });
      }
      if (case_.id.startsWith("PATCH-")) raw.push({ persistence: await todoSnapshot(case_.id === "PATCH-004" ? t2 : t1) });
      if (case_.id === "DELETE-002") raw.push({ persistence: await todoSnapshot(t1) });
      if (case_.id === "CREATE-001") raw.push({ persistence: await todoSnapshot(reply.body.id) });
      if (case_.id === "DELETE-001") {
        raw.push({ persistence: await todoSnapshot(t1) });
        await send({ ...case_.request, method: "GET" });
      }
    }
    results.push({ id: case_.id, raw: raw.slice(start) });
    } catch {
      // Keep later IDs executable and retain an opaque failure disposition;
      // exception/provider text and credentials never enter the raw ledger.
      results.push({ id: case_.id, adapterFailure: true, raw: raw.slice(start) });
    }
  }
  writeFileSync(output, JSON.stringify({ backend, supported, results }, null, 2) + "\n", { flag: "wx" });
} finally {
  server.stop(true); db?.close(); if (pg) await pg.close(); prototype.prepare = originalPrepare; prototype.exec = originalExec; globalThis.Date = nativeDate;
}
