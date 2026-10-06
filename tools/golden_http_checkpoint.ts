// Real migrated-application observations for the reviewed eight-case slice.
// Receives the full frozen contract; never reads expected values to emit results.
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { beginSqlObservation, endSqlObservation, recordStatement, authenticationAttemptCount } from "./golden_sql_observer.ts";

const supported = ["AUTH-001", "AUTH-005", "AUTH-012", "PUBLIC-001", "CREATE-002", "CREATE-004", "READ-002", "READ-003"];
const [project, backend, contractPath, output] = Bun.argv.slice(2);
if (!project || !["sqlite", "postgres"].includes(backend) || !output) throw new Error("Missing checkpoint arguments");
const prototype = Database.prototype as any;
const originalPrepare = prototype.prepare;
const originalExec = prototype.exec;
prototype.exec = function(sql: string) { recordStatement(sql); return originalExec.call(this, sql); };
prototype.prepare = function(sql: string, ...options: unknown[]) {
  const statement = originalPrepare.call(this, sql, ...options);
  return new Proxy(statement, { get(target, property) {
    const member = Reflect.get(target, property, target);
    if (["all", "get", "run", "values"].includes(String(property))) return (...values: unknown[]) => {
      recordStatement(sql); return member.apply(target, values);
    };
    return typeof member === "function" ? member.bind(target) : member;
  } });
};
if (backend === "sqlite") { Bun.env.SQLITE_PATH = join(project, "checkpoint.sqlite"); delete Bun.env.DATABASE_URL; }
else { if (!Bun.env.DATABASE_URL) throw new Error("Missing isolated PostgreSQL URL"); delete Bun.env.SQLITE_PATH; }
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
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
const environment = {
  DATABASE_URL: "https://db.test/golden", SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: "https://issuer.test", OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"), MAIL_SENDER: "todo@example.test",
};
let session: any;
let secrets: string[] = Object.values(environment).filter(value => value === environment.SESSION_SIGNING_KEY || value === environment.MAIL_API_KEY);
async function reset() {
  await app.initializeApplication(environment);
  for (const table of ["service_credential", "__jadpo_auth_service_credentials", "service", "todo", "__jadpo_auth_sessions", "user"]) await execute(`DELETE FROM "${table}"`);
  const now = Date.now();
  for (const [id, subject] of [[alice, "alice"], [bob, "bob"]]) await execute('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [id, subject, `${subject}@example.test`, "active", pg ? new Date(now).toISOString() : now]);
  session = await app.authenticationHost().issue("browser_session", "alice", now + 300_000, now);
  secrets = [...secrets, session.credential, session.csrfToken];
}
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
const raw: any[] = [];
async function send(request: any) {
  const headers: Record<string, string> = {};
  const auth = Array.isArray(request.auth) ? request.auth : [request.auth];
  if (auth.includes("alice-session")) Object.assign(headers, { cookie: session.setCookie.split(";", 1)[0], origin: environment.BROWSER_ORIGIN, "x-jadpo-csrf": session.csrfToken });
  if (auth.includes("invalid-jwt") || auth.includes("invalid-bearer")) headers.authorization = "Bearer INVALID_CHECKPOINT_CREDENTIAL";
  const path = request.path.replace("/todos/t2", `/todos/${t2}`);
  if (request.json !== undefined) headers["content-type"] = "application/json";
  const statements = beginSqlObservation();
  let response: Response;
  let body: any;
  try {
    response = await fetch(new URL(path, server.url), { method: request.method, headers, ...(request.json === undefined ? {} : { body: JSON.stringify(request.json) }) });
    body = await response.json();
  } finally { endSqlObservation(); }
  const authenticationAttempts = authenticationAttemptCount();
  const captured = { method: request.method, path, auth: request.auth ?? null, requestJson: request.json ?? null, status: response!.status, body, statements, authenticationAttempts };
  if (secrets.some(secret => JSON.stringify(captured).includes(secret))) throw new Error("Unsafe raw observation refused");
  raw.push(captured);
  return { status: response!.status, code: body?.error?.code, body, databaseQueries: statements.length, authenticationAttempts, writes: statements.filter(row => ["INSERT", "UPDATE", "DELETE", "REPLACE"].includes(row.operation)).length };
}
const results: any[] = [];
try {
  const contract = JSON.parse(readFileSync(contractPath, "utf8"));
  for (const case_ of contract.cases.filter((case_: any) => supported.includes(case_.id))) {
    if (case_.fixtureVariants || case_.followUps) throw new Error("Unsupported scenario extension");
    const start = raw.length;
    try {
    await reset();
    if (case_.id === "READ-002") {
      const now = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)', [t2, bob, "private bob todo", "open", pg ? new Date(now).toISOString() : now]);
      for (const request of case_.requests) await send(request);
    } else {
      const reply = await send(case_.request);
      if (case_.id === "CREATE-004") {
        const rows = await execute('SELECT due_at FROM todo WHERE id = $1', [reply.body.id]);
        if (rows.length !== 1) throw new Error("Missing persisted create observation");
        raw.push({ persistence: { id: reply.body.id, storedDueAt: rows[0].due_at } });
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
  server.stop(true); db?.close(); if (pg) await pg.close(); prototype.prepare = originalPrepare; prototype.exec = originalExec;
}
