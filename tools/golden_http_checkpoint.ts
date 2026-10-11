// Real migrated-application observations for the reviewed supporting HTTP slice.
// Receives the full frozen contract; never reads expected values to emit results.
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { beginSqlObservation, endSqlObservation, recordStatement, authenticationAttemptCount, authenticatedPrincipalObservation, todoLimitBindingObservation } from "./golden_sql_observer.ts";
import { installCheckpointFetchPolicy } from "./golden_checkpoint_fetch.ts";

const supported = ["AUTH-001", "AUTH-002", "AUTH-003", "AUTH-004", "AUTH-005", "AUTH-006", "AUTH-007", "AUTH-008", "AUTH-009", "AUTH-010", "AUTH-011", "AUTH-012", "PUBLIC-001", "CREATE-001", "CREATE-002", "CREATE-003", "CREATE-004", "READ-001", "READ-002", "READ-003", "LIST-001", "LIST-002", "LIST-003", "LIST-004", "REL-001", "PATCH-001", "PATCH-003", "PATCH-004", "DELETE-001", "DELETE-002", "USER-001"];
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
      recordStatement(sql, values); return member.apply(target, values);
    };
    return typeof member === "function" ? member.bind(target) : member;
  } });
};
if (backend === "sqlite") { Bun.env.SQLITE_PATH = join(project, "checkpoint.sqlite"); delete Bun.env.DATABASE_URL; }
else { if (!Bun.env.DATABASE_URL) throw new Error("Missing isolated PostgreSQL URL"); delete Bun.env.SQLITE_PATH; }
const db = backend === "sqlite" ? new Database(Bun.env.SQLITE_PATH, { strict: true }) : undefined;
const pg = backend === "postgres" ? new SQL({ url: Bun.env.DATABASE_URL!, prepare: false }) : undefined;
async function execute(sql: string, values: unknown[] = []) {
  if (pg) return await pg.unsafe(sql, values);
  const statement = db!.prepare(sql.replace(/\$(\d+)/gu, "?$1"));
  try { return statement.all(...values); } finally { statement.finalize(); }
}
const alice = "00000000-0000-4000-8000-000000000001";
const bob = "00000000-0000-4000-8000-000000000002";
const t1 = "00000000-0000-4000-8000-000000000021";
const t2 = "00000000-0000-4000-8000-000000000022";
const listFixtureIds = [
  "00000000-0000-4000-8000-000000000031",
  "00000000-0000-4000-8000-000000000032",
  "00000000-0000-4000-8000-000000000033",
  "00000000-0000-4000-8000-000000000034",
  "00000000-0000-4000-8000-000000000099",
];
const listTieTime = "2026-01-15T11:00:00.000Z";
const listOlderTime = "2026-01-15T10:00:00.000Z";
const reportingService = "00000000-0000-4000-8000-000000000003";
const fixedCreateClockNow = "2026-01-15T12:00:00.000Z";
const userSessionIssuedAt = "2026-01-15T11:59:00.000Z";
const userDisabledAt = "2026-01-15T12:00:00.000Z";
const userRepeatedDisableAt = "2026-01-15T12:00:01.000Z";
const userRefreshAt = "2026-01-15T12:00:02.000Z";
const userSessionExpiresAt = "2026-01-15T12:04:00.000Z";
const environment = {
  DATABASE_URL: "https://db.test/golden", SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: "https://issuer.test", OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"), MAIL_SENDER: "todo@example.test",
};
let session: any;
let sessionIssuedAt = 0;
let sessionExpiresAt = 0;
let apiBearer: any;
let apiBearerIssuedAt = 0;
let apiBearerExpiresAt = 0;
let serviceCredential: any;
let serviceCredentialIssuedAt = 0;
let serviceCredentialExpiresAt = 0;
let aliceJwt: string | undefined;
let bobJwt: string | undefined;
let aliceJwtProviderCalls: string[] = [];
let aliceJwtRejectedProviderCalls: string[] = [];
let aliceJwk: any;
let secrets: string[] = Object.values(environment).filter(value => value === environment.SESSION_SIGNING_KEY || value === environment.MAIL_API_KEY);
const oidcDiscoveryUrl = `${environment.OIDC_ISSUER}/.well-known/openid-configuration`;
const oidcJwksUrl = `${environment.OIDC_ISSUER}/jwks`;
const originalProviderFetch = globalThis.fetch;
let localTestServerOrigin: string | undefined;
installCheckpointFetchPolicy({ issuer: environment.OIDC_ISSUER, jwksUri: oidcJwksUrl,
  localOrigin: () => localTestServerOrigin, jwk: () => aliceJwk,
  providerCalls: aliceJwtProviderCalls, rejectedCalls: aliceJwtRejectedProviderCalls,
  nativeFetch: originalProviderFetch });
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
async function reset() {
  await app.initializeApplication(environment);
  for (const table of ["service_credential", "__jadpo_auth_service_credentials", "service", "todo", "__jadpo_auth_sessions", "user"]) await execute(`DELETE FROM "${table}"`);
  const now = Date.now();
  sessionIssuedAt = now;
  sessionExpiresAt = now + 300_000;
  apiBearer = undefined;
  apiBearerIssuedAt = 0;
  apiBearerExpiresAt = 0;
  serviceCredential = undefined;
  serviceCredentialIssuedAt = 0;
  serviceCredentialExpiresAt = 0;
  aliceJwt = undefined;
  bobJwt = undefined;
  aliceJwk = undefined;
  aliceJwtProviderCalls.length = 0;
  aliceJwtRejectedProviderCalls.length = 0;
  for (const [id, subject] of [[alice, "alice"], [bob, "bob"]]) await execute('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [id, subject, `${subject}@example.test`, "active", pg ? new Date(now).toISOString() : now]);
  session = await app.authenticationHost().issue("browser_session", "alice", sessionExpiresAt, sessionIssuedAt);
  secrets = [...secrets, session.credential, session.csrfToken];
}
function installFixedClock(instant: string) {
  const actualDate = globalThis.Date;
  const fixedNow = actualDate.parse(instant);
  if (!Number.isFinite(fixedNow)) throw new Error("Invalid checkpoint fixed clock");
  const FixedDate = new Proxy(actualDate, {
    construct(target, arguments_) {
      return Reflect.construct(target, arguments_.length === 0 ? [fixedNow] : arguments_, target);
    },
    get(target, property) {
      if (property === "now") return () => fixedNow;
      return Reflect.get(target, property, target);
    },
  });
  globalThis.Date = FixedDate;
  return () => { globalThis.Date = actualDate; };
}
async function withFixedClock(instant: string, operation: () => Promise<any>) {
  const restore = installFixedClock(instant);
  try { return await operation(); } finally { restore(); }
}
function timestamp(value: any) {
  const millis = value instanceof Date ? value.getTime() : typeof value === "number" ? value : Date.parse(value);
  if (!Number.isFinite(millis)) throw new Error("Invalid USER-001 persisted timestamp");
  return new Date(millis).toISOString();
}
async function userTodoReadback(stage: string) {
  const users = await execute('SELECT id, authentication_subject, status, disabled_at FROM "user" WHERE id = $1', [alice]);
  const todos = await execute('SELECT id, owner_id, title, status, due_at, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
  if (users.length !== 1 || todos.length !== 1) throw new Error("Missing USER-001 user/Todo persistence readback");
  return { persistence: { stage,
    user: { id: users[0].id, subject: users[0].authentication_subject, status: users[0].status,
      disabledAt: users[0].disabled_at === null ? null : timestamp(users[0].disabled_at) },
    todo: { id: todos[0].id, ownerId: todos[0].owner_id, title: todos[0].title, status: todos[0].status,
      dueAt: todos[0].due_at, createdAt: timestamp(todos[0].created_at), updatedAt: timestamp(todos[0].updated_at),
      deletedAt: todos[0].deleted_at === null ? null : timestamp(todos[0].deleted_at) } } };
}
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
localTestServerOrigin = server.url.origin;
const raw: any[] = [];
async function inspectServiceBearer(token: string) {
  const parts = token.split(".");
  if (parts.length !== 4 || parts[0] !== "jdx1") throw new Error("Invalid AUTH-009 exchanged credential format");
  const message = parts.slice(0, 3).join(".");
  const key = await crypto.subtle.importKey("raw", Buffer.from(environment.SESSION_SIGNING_KEY, "base64url"), { name: "HMAC", hash: "SHA-256" }, false, ["verify"]);
  const signatureValid = await crypto.subtle.verify("HMAC", key, Buffer.from(parts[3], "base64url"), new TextEncoder().encode(message));
  const payload = JSON.parse(Buffer.from(parts[2], "base64url").toString("utf8"));
  if (!Array.isArray(payload) || payload.length !== 10) throw new Error("Invalid AUTH-009 exchanged credential claims");
  return { signatureValid, version: payload[0], audience: payload[1], strategy: payload[2], credentialId: payload[3],
    subject: payload[4], serviceId: payload[5], issuedAt: payload[6], expiresAt: payload[7],
    principalKind: payload[8], authenticationStrength: payload[9] };
}
async function inspectUserBearer(token: string) {
  const parts = token.split(".");
  if (parts.length !== 4 || parts[0] !== "jds1") throw new Error("Invalid AUTH-008 signed bearer format");
  const message = parts.slice(0, 3).join(".");
  const key = await crypto.subtle.importKey("raw", Buffer.from(environment.SESSION_SIGNING_KEY, "base64url"), { name: "HMAC", hash: "SHA-256" }, false, ["verify"]);
  const signatureValid = await crypto.subtle.verify("HMAC", key, Buffer.from(parts[3], "base64url"), new TextEncoder().encode(message));
  const payload = JSON.parse(Buffer.from(parts[2], "base64url").toString("utf8"));
  if (!Array.isArray(payload) || payload.length !== 8) throw new Error("Invalid AUTH-008 signed bearer claims");
  return { signatureValid, version: payload[0], audience: payload[1], strategy: payload[2], credentialId: payload[3],
    subject: payload[4], userId: payload[5], issuedAt: payload[6], expiresAt: payload[7] };
}
async function issueAliceJwtWithUntrustedEmail() {
  const josePath = pathToFileURL(join(project, "build/target/node_modules/jose/dist/webapi/index.js")).href;
  const { SignJWT, generateKeyPair, exportJWK, jwtVerify } = await import(josePath);
  const keys = await generateKeyPair("RS256", { modulusLength: 2048, extractable: true });
  const issuer = environment.OIDC_ISSUER;
  const audience = environment.OIDC_AUDIENCE;
  const discovery = `${issuer}/.well-known/openid-configuration`;
  const jwksUri = `${issuer}/jwks`;
  const publicJwk = { ...await exportJWK(keys.publicKey), kid: "golden-rsa", alg: "RS256", use: "sig" };
  const issuedAt = Math.floor(Date.now() / 1000);
  const expiresAt = issuedAt + 120;
  const credential = await new SignJWT({ iss: issuer, aud: audience, sub: "alice", email: "attacker@example.test", iat: issuedAt, exp: expiresAt })
    .setProtectedHeader({ alg: "RS256", kid: "golden-rsa", typ: "JWT" }).sign(keys.privateKey);
  const verified = await jwtVerify(credential, keys.publicKey, { issuer, audience });
  const claims = { signatureValid: true, issuer: verified.payload.iss, audience: verified.payload.aud,
    subject: verified.payload.sub, email: verified.payload.email, issuedAt: (verified.payload.iat ?? 0) * 1000,
    expiresAt: (verified.payload.exp ?? 0) * 1000 };
  aliceJwtProviderCalls.length = 0;
  aliceJwk = publicJwk;
  aliceJwt = credential;
  secrets = [...secrets, credential];
  return { claims };
}
async function issueBobJwtForAmbiguity() {
  const josePath = pathToFileURL(join(project, "build/target/node_modules/jose/dist/webapi/index.js")).href;
  const { SignJWT, generateKeyPair, exportJWK, jwtVerify } = await import(josePath);
  const keys = await generateKeyPair("RS256", { modulusLength: 2048, extractable: true });
  const issuer = environment.OIDC_ISSUER;
  const audience = environment.OIDC_AUDIENCE;
  const publicJwk = { ...await exportJWK(keys.publicKey), kid: "golden-bob-rsa", alg: "RS256", use: "sig" };
  const issuedAt = Math.floor(Date.now() / 1000);
  const expiresAt = issuedAt + 120;
  const credential = await new SignJWT({ iss: issuer, aud: audience, sub: "bob", iat: issuedAt, exp: expiresAt })
    .setProtectedHeader({ alg: "RS256", kid: "golden-bob-rsa", typ: "JWT" }).sign(keys.privateKey);
  const verified = await jwtVerify(credential, keys.publicKey, { issuer, audience });
  bobJwt = credential;
  aliceJwk = publicJwk;
  secrets = [...secrets, credential];
  return { credential, claims: { signatureValid: true, issuer: verified.payload.iss,
    audience: verified.payload.aud, subject: verified.payload.sub,
    issuedAt: (verified.payload.iat ?? 0) * 1000, expiresAt: (verified.payload.exp ?? 0) * 1000 } };
}
async function send(request: any, resolveAliceUserPath = false, captureTodoLimit = false) {
  const headers: Record<string, string> = {};
  const auth = Array.isArray(request.auth) ? request.auth : [request.auth];
  if (auth.includes("alice-session")) Object.assign(headers, { cookie: session.setCookie.split(";", 1)[0], origin: environment.BROWSER_ORIGIN, "x-jadpo-csrf": session.csrfToken });
  if (auth.includes("reporting-service-key")) {
    if (!serviceCredential?.credential) throw new Error("Missing service credential for exchange checkpoint");
    headers.authorization = `Bearer ${serviceCredential.credential}`;
  }
  if (auth.includes("alice-bounded-bearer")) {
    if (!apiBearer?.credential) throw new Error("Missing bounded signed API bearer");
    headers.authorization = `Bearer ${apiBearer.credential}`;
  }
  if (auth.includes("alice-jwt")) {
    if (!aliceJwt) throw new Error("Missing AUTH-007 signed JWT with untrusted email claim");
    headers.authorization = `Bearer ${aliceJwt}`;
  }
  if (auth.includes("bob-jwt")) {
    if (!bobJwt) throw new Error("Missing AUTH-004 signed Bob JWT");
    headers.authorization = `Bearer ${bobJwt}`;
  }
  if (auth.includes("invalid-jwt") || auth.includes("invalid-bearer")) headers.authorization = "Bearer INVALID_CHECKPOINT_CREDENTIAL";
  const path = request.path.replace("/todos/t1", `/todos/${t1}`).replace("/todos/t2", `/todos/${t2}`);
  const routedPath = auth.includes("alice-jwt") || auth.includes("alice-bounded-bearer") || resolveAliceUserPath
    ? path.replace("/users/alice", `/users/${alice}`) : path;
  if (request.json !== undefined) headers["content-type"] = "application/json";
  const statements = beginSqlObservation(captureTodoLimit);
  let response: Response;
  let body: any;
  let serviceExchange: any;
  const requestStartedAt = Date.now();
  let responseCompletedAt: number | undefined;
  try {
    response = await fetch(new URL(routedPath, server.url), { method: request.method, headers, ...(request.json === undefined ? {} : { body: JSON.stringify(request.json) }) });
    body = response.status === 204 ? null : await response.json();
    responseCompletedAt = Date.now();
    if (auth.includes("reporting-service-key") && response.status === 200) {
      if (typeof body?.access_token !== "string" || !serviceCredential?.credential) throw new Error("Missing AUTH-009 exchange response credential");
      const exchangedToken = body.access_token;
      const secretInOutput = JSON.stringify(body).includes(serviceCredential.credential);
      const tokenClaims = await inspectServiceBearer(exchangedToken);
      secrets = [...secrets, exchangedToken];
      body = { ...body, access_token: "<redacted>" };
      serviceExchange = { ...tokenClaims, secretInOutput, outputTokenRedacted: true };
    }
  } finally { endSqlObservation(); }
  const authenticationAttempts = authenticationAttemptCount();
  const principal = authenticatedPrincipalObservation();
  const todoLimitBindings = captureTodoLimit ? todoLimitBindingObservation() : undefined;
  const captured = { method: request.method, path: routedPath, auth: request.auth ?? null, requestJson: request.json ?? null, requestStartedAt, responseCompletedAt, status: response!.status, body, statements, authenticationAttempts,
    ...(principal ? { principal } : {}),
    ...(todoLimitBindings ? { todoLimitBindings } : {}),
    ...(serviceExchange ? { serviceExchange } : {}) };
  if (secrets.some(secret => JSON.stringify(captured).includes(secret))) throw new Error("Unsafe raw observation refused");
  raw.push(captured);
  return { status: response!.status, code: body?.error?.code, body, databaseQueries: statements.length, authenticationAttempts,
    writes: statements.filter(row => ["INSERT", "UPDATE", "DELETE", "REPLACE"].includes(row.operation)).length,
    ...(principal ? { principal } : {}),
    ...(serviceExchange ? { serviceExchange } : {}) };
}
const results: any[] = [];
try {
  const contract = JSON.parse(readFileSync(contractPath, "utf8"));
  for (const case_ of contract.cases.filter((case_: any) => supported.includes(case_.id))) {
    if ((case_.fixtureVariants && case_.id !== "REL-001") || (case_.followUps && case_.id !== "USER-001")) throw new Error("Unsupported scenario extension");
    const start = raw.length;
    let restoreClock: (() => void) | undefined;
    try {
    if (["CREATE-001", "CREATE-003", "DELETE-001"].includes(case_.id)) restoreClock = installFixedClock(fixedCreateClockNow);
    else if (case_.id === "USER-001") restoreClock = installFixedClock(userSessionIssuedAt);
    await reset();
    if (case_.id === "USER-001") { restoreClock?.(); restoreClock = undefined; }
    if (case_.id === "AUTH-002") {
      const rows = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (rows.length !== 1 || rows[0].status !== "active" || !session?.credential || !session?.csrfToken || !session?.setCookie) throw new Error("Missing AUTH-002 active-user browser session fixture");
      raw.push({ fixture: { subject: rows[0].authentication_subject, userId: rows[0].id, status: rows[0].status,
        sessionIssuedAt, sessionExpiresAt, sessionAvailable: true, csrfAvailable: true } });
      await send(case_.request);
    } else if (case_.id === "AUTH-003") {
      const rows = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (rows.length !== 1 || rows[0].status !== "active") throw new Error("Missing AUTH-003 active Alice fixture");
      const jwt = await issueAliceJwtWithUntrustedEmail();
      const fixture = { subject: rows[0].authentication_subject, userId: rows[0].id, status: rows[0].status,
        tokenClaims: jwt.claims, providerCalls: [] as string[], rejectedProviderCalls: [] as string[] };
      raw.push({ fixture });
      await send(case_.request);
      fixture.providerCalls = [...aliceJwtProviderCalls];
      fixture.rejectedProviderCalls = [...aliceJwtRejectedProviderCalls];
    } else if (case_.id === "AUTH-004") {
      const aliceRows = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      const bobRows = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["bob"]);
      if (aliceRows.length !== 1 || aliceRows[0].status !== "active" || bobRows.length !== 1 || bobRows[0].status !== "active") {
        throw new Error("Missing AUTH-004 active Alice/Bob identity fixtures");
      }
      const jwt = await issueBobJwtForAmbiguity();
      const fixture = { aliceSession: { subject: aliceRows[0].authentication_subject, userId: aliceRows[0].id,
        status: aliceRows[0].status, issuedAt: sessionIssuedAt, expiresAt: sessionExpiresAt, sessionAvailable: true },
        bobJwt: { subject: bobRows[0].authentication_subject, userId: bobRows[0].id, status: bobRows[0].status,
          tokenClaims: jwt.claims }, providerCalls: [] as string[], rejectedProviderCalls: [] as string[] };
      raw.push({ fixture });
      await send(case_.request);
      fixture.providerCalls = [...aliceJwtProviderCalls];
      fixture.rejectedProviderCalls = [...aliceJwtRejectedProviderCalls];
    } else if (case_.id === "AUTH-007") {
      const rows = await execute('SELECT id, authentication_subject, email, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (rows.length !== 1 || rows[0].status !== "active" || rows[0].email !== "alice@example.test") throw new Error("Missing AUTH-007 persisted Alice email fixture");
      const jwt = await issueAliceJwtWithUntrustedEmail();
      const fixture = { subject: rows[0].authentication_subject, userId: rows[0].id, status: rows[0].status,
        storedEmail: rows[0].email, tokenClaims: jwt.claims, providerCalls: [] as string[], rejectedProviderCalls: [] as string[] };
      raw.push({ fixture });
      await send(case_.request);
      fixture.providerCalls = [...aliceJwtProviderCalls];
      fixture.rejectedProviderCalls = [...aliceJwtRejectedProviderCalls];
    } else if (case_.id === "LIST-001") {
      const fixtureRows = [
        { id: listFixtureIds[0], ownerId: alice, title: "alice tie lower id", status: "open", createdAt: listTieTime, deletedAt: null },
        { id: listFixtureIds[1], ownerId: alice, title: "alice tie higher id", status: "open", createdAt: listTieTime, deletedAt: null },
        { id: listFixtureIds[2], ownerId: alice, title: "alice older active", status: "open", createdAt: listOlderTime, deletedAt: null },
        { id: listFixtureIds[3], ownerId: bob, title: "bob newer active", status: "open", createdAt: listTieTime, deletedAt: null },
        { id: listFixtureIds[4], ownerId: alice, title: "alice newer deleted", status: "open", createdAt: listTieTime, deletedAt: listTieTime },
      ];
      for (const row of fixtureRows) {
        const createdAt = pg ? row.createdAt : Date.parse(row.createdAt);
        const deletedAt = row.deletedAt === null ? null : pg ? row.deletedAt : Date.parse(row.deletedAt);
        await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, $6)',
          [row.id, row.ownerId, row.title, row.status, createdAt, deletedAt]);
      }
      const rows = await execute(`SELECT id, owner_id, title, status, created_at, deleted_at FROM "todo" WHERE id IN (${listFixtureIds.map((_, index) => `$${index + 1}`).join(", ")}) ORDER BY id ASC`, listFixtureIds);
      if (rows.length !== fixtureRows.length) throw new Error("Missing LIST-001 persisted fixture rows");
      const persistedFixtures = rows.map((row: any) => ({ id: row.id, ownerId: row.owner_id, title: row.title,
        status: row.status, createdAt: timestamp(row.created_at),
        deletedAt: row.deleted_at === null ? null : timestamp(row.deleted_at) }));
      raw.push({ fixture: { rows: persistedFixtures } });
      await send(case_.request);
    } else if (case_.id === "LIST-002") {
      const fixtureRows = Array.from({ length: 5 }, (_, index) => ({
        id: `00000000-0000-4000-8000-0000000000${41 + index}`,
        ownerId: alice,
        title: `cursor row ${index + 1}`,
        status: "open",
        createdAt: `2026-01-15T${String(10 + index).padStart(2, "0")}:00:00.000Z`,
        deletedAt: null,
      }));
      if (case_.requests.length !== 3
        || case_.requests[0].path !== "/todos?page_size=2"
        || case_.requests[1].path !== "/todos?page_size=2&after={first.next}"
        || case_.requests[2].path !== "/todos?page_size=2&after={second.next}") {
        throw new Error("Changed LIST-002 frozen cursor request sequence");
      }
      for (const row of fixtureRows) {
        await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)',
          [row.id, row.ownerId, row.title, row.status, pg ? row.createdAt : Date.parse(row.createdAt)]);
      }
      const rows = await execute(`SELECT id, owner_id, title, status, created_at, deleted_at FROM "todo" WHERE id IN (${fixtureRows.map((_, index) => `$${index + 1}`).join(", ")}) ORDER BY id ASC`, fixtureRows.map(row => row.id));
      if (rows.length !== fixtureRows.length) throw new Error("Missing LIST-002 persisted fixture rows");
      const persistedFixtures = rows.map((row: any) => ({ id: row.id, ownerId: row.owner_id, title: row.title,
        status: row.status, createdAt: timestamp(row.created_at),
        deletedAt: row.deleted_at === null ? null : timestamp(row.deleted_at) }));
      raw.push({ fixture: { rows: persistedFixtures } });
      const runTraversal = async () => {
        let cursor: { created_at: string; id: string } | null = null;
        for (let pageIndex = 0; pageIndex < case_.requests.length; pageIndex++) {
          const template = case_.requests[pageIndex];
          let path = template.path;
          if (pageIndex > 0) {
            if (!cursor) throw new Error("LIST-002 page ended before frozen continuation request");
            const marker = pageIndex === 1 ? "{first.next}" : "{second.next}";
            path = path.replace(marker, encodeURIComponent(JSON.stringify({ created_at: cursor.created_at, id: cursor.id })));
          }
          const reply = await send({ ...template, path });
          cursor = reply.body?.next ?? null;
        }
      };
      await runTraversal();
      await runTraversal();
    } else if (case_.id === "LIST-003") {
      const fixtureRows = [
        { id: "00000000-0000-4000-8000-000000000061", ownerId: alice, title: "filter fixture 1", status: "open", dueAt: "2026-01-30T00:00:00.000Z", createdAt: "2026-01-15T10:00:00.000Z", deletedAt: null },
        { id: "00000000-0000-4000-8000-000000000062", ownerId: alice, title: "filter fixture 2", status: "open", dueAt: "2026-01-31T00:00:00.000Z", createdAt: "2026-01-15T11:00:00.000Z", deletedAt: null },
        { id: "00000000-0000-4000-8000-000000000063", ownerId: alice, title: "filter fixture 3", status: "closed", dueAt: "2026-01-20T00:00:00.000Z", createdAt: "2026-01-15T12:00:00.000Z", deletedAt: null },
        { id: "00000000-0000-4000-8000-000000000064", ownerId: alice, title: "filter fixture 4", status: "open", dueAt: "2026-02-02T00:00:00.000Z", createdAt: "2026-01-15T13:00:00.000Z", deletedAt: null },
        { id: "00000000-0000-4000-8000-000000000065", ownerId: alice, title: "filter fixture 5", status: "open", dueAt: null, createdAt: "2026-01-15T14:00:00.000Z", deletedAt: null },
        { id: "00000000-0000-4000-8000-000000000066", ownerId: bob, title: "filter fixture 6", status: "open", dueAt: "2026-01-25T00:00:00.000Z", createdAt: "2026-01-15T15:00:00.000Z", deletedAt: null },
        { id: "00000000-0000-4000-8000-000000000067", ownerId: alice, title: "filter fixture 7", status: "open", dueAt: "2026-01-31T00:00:00.000Z", createdAt: "2026-01-15T16:00:00.000Z", deletedAt: "2026-01-16T00:00:00.000Z" },
      ];
      if (case_.request.path !== "/todos?status=open&due_before=2026-02-01T00:00:00Z") {
        throw new Error("Changed LIST-003 frozen filtered request");
      }
      for (const row of fixtureRows) {
        const dueAt = row.dueAt === null ? null : pg ? row.dueAt : Date.parse(row.dueAt);
        const createdAt = pg ? row.createdAt : Date.parse(row.createdAt);
        const deletedAt = row.deletedAt === null ? null : pg ? row.deletedAt : Date.parse(row.deletedAt);
        await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, $5, NULL, $6, $6, $7)',
          [row.id, row.ownerId, row.title, row.status, dueAt, createdAt, deletedAt]);
      }
      const rows = await execute(`SELECT id, owner_id, title, status, due_at, created_at, deleted_at FROM "todo" WHERE id IN (${fixtureRows.map((_, index) => `$${index + 1}`).join(", ")}) ORDER BY id ASC`, fixtureRows.map(row => row.id));
      if (rows.length !== fixtureRows.length) throw new Error("Missing LIST-003 persisted fixture rows");
      const persistedFixtures = rows.map((row: any) => ({ id: row.id, ownerId: row.owner_id, title: row.title,
        status: row.status, dueAt: row.due_at === null ? null : timestamp(row.due_at),
        createdAt: timestamp(row.created_at),
        deletedAt: row.deleted_at === null ? null : timestamp(row.deleted_at) }));
      raw.push({ fixture: { rows: persistedFixtures } });
      await send(case_.request);
    } else if (case_.id === "REL-001") {
      const variants = ["alice zero todos", "alice one todo", "alice 100 todos"];
      if (JSON.stringify(case_.fixtureVariants) !== JSON.stringify(variants)
          || case_.request?.method !== "GET" || case_.request?.path !== "/users/alice/todos"
          || case_.request?.auth !== "alice-session") {
        throw new Error("Changed REL-001 frozen UserWithTodos scenario");
      }
      for (let variantIndex = 0; variantIndex < variants.length; variantIndex++) {
        if (variantIndex > 0) await reset();
        const activeCount = variantIndex === 0 ? 0 : variantIndex === 1 ? 1 : 100;
        const fixtureRows: any[] = [];
        for (let index = 0; index < activeCount; index++) {
          const createdAt = new Date(Date.parse("2026-01-15T10:00:00.000Z") + Math.floor(index / 2) * 60_000).toISOString();
          fixtureRows.push({ id: `00000000-0000-4000-8000-${String(3000 + index).padStart(12, "0")}`,
            ownerId: alice, title: `alice todo ${index + 1}`, status: "open", dueAt: null,
            createdAt, updatedAt: createdAt, deletedAt: null });
        }
        if (variantIndex === 2) {
          fixtureRows.push({ id: "00000000-0000-4000-8000-000000000201", ownerId: alice,
            title: "deleted Alice todo", status: "open", dueAt: null,
            createdAt: "2026-01-15T09:00:00.000Z", updatedAt: "2026-01-15T09:00:00.000Z",
            deletedAt: "2026-01-16T00:00:00.000Z" });
        }
        fixtureRows.push({ id: "00000000-0000-4000-8000-000000000202", ownerId: bob,
          title: "Bob foreign todo", status: "open", dueAt: null,
          createdAt: "2026-01-15T08:00:00.000Z", updatedAt: "2026-01-15T08:00:00.000Z",
          deletedAt: null });
        for (const row of fixtureRows) {
          await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $8)',
            [row.id, row.ownerId, row.title, row.status, row.dueAt,
              pg ? new Date(row.createdAt).toISOString() : Date.parse(row.createdAt),
              pg ? new Date(row.updatedAt).toISOString() : Date.parse(row.updatedAt),
              row.deletedAt === null ? null : pg ? new Date(row.deletedAt).toISOString() : Date.parse(row.deletedAt)]);
        }
        const persisted = await execute(`SELECT id, owner_id, title, status, due_at, created_at, updated_at, deleted_at FROM todo WHERE id IN (${fixtureRows.map((_, index) => `$${index + 1}`).join(", ")}) ORDER BY id ASC`, fixtureRows.map(row => row.id));
        if (persisted.length !== fixtureRows.length) throw new Error(`Missing REL-001 persisted fixture rows for variant ${variantIndex}`);
        raw.push({ fixture: { variantIndex, variant: variants[variantIndex], rows: persisted.map((row: any) => ({
          id: row.id, ownerId: row.owner_id, title: row.title, status: row.status,
          dueAt: row.due_at === null ? null : timestamp(row.due_at),
          createdAt: timestamp(row.created_at), updatedAt: timestamp(row.updated_at),
          deletedAt: row.deleted_at === null ? null : timestamp(row.deleted_at),
        })) } });
        await send(case_.request, true, true);
      }
    } else if (case_.id === "PATCH-001") {
      const expectedFixture = ["alice todo t1 due 2026-02-01"];
      const expectedRequest = { method: "PATCH", path: "/todos/t1", auth: "alice-session", json: { title: "Changed" } };
      const expected = { status: 200, storedDueAt: "2026-02-01T00:00:00Z", storedTitle: "Changed" };
      if (JSON.stringify(case_.fixture) !== JSON.stringify(expectedFixture)
          || JSON.stringify(case_.request) !== JSON.stringify(expectedRequest)
          || JSON.stringify(case_.expect) !== JSON.stringify(expected)) {
        throw new Error("Changed PATCH-001 frozen due-date omission case");
      }
      const dueAt = "2026-02-01T00:00:00.000Z";
      const reminderSentAt = "2026-01-20T00:00:00.000Z";
      const createdAt = "2026-01-15T10:00:00.000Z";
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $7, NULL)',
        [t1, alice, "alice todo t1", "open", pg ? dueAt : Date.parse(dueAt),
          pg ? reminderSentAt : Date.parse(reminderSentAt), pg ? createdAt : Date.parse(createdAt)]);
      const projectRow = (row: any) => ({ id: row.id, ownerId: row.owner_id, title: row.title, status: row.status,
        dueAt: row.due_at === null ? null : timestamp(row.due_at),
        reminderSentAt: row.reminder_sent_at === null ? null : timestamp(row.reminder_sent_at),
        createdAt: timestamp(row.created_at), updatedAt: timestamp(row.updated_at),
        deletedAt: row.deleted_at === null ? null : timestamp(row.deleted_at) });
      const fixtureRows = await execute('SELECT id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (fixtureRows.length !== 1 || fixtureRows[0].owner_id !== alice || fixtureRows[0].title !== "alice todo t1"
          || fixtureRows[0].status !== "open" || timestamp(fixtureRows[0].due_at) !== dueAt
          || timestamp(fixtureRows[0].reminder_sent_at) !== reminderSentAt || fixtureRows[0].deleted_at !== null) {
        throw new Error("Missing PATCH-001 persisted Alice todo fixture");
      }
      raw.push({ fixture: projectRow(fixtureRows[0]) });
      await send(case_.request);
      const persistedRows = await execute('SELECT id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (persistedRows.length !== 1) throw new Error("Missing PATCH-001 persisted todo readback");
      raw.push({ persistence: projectRow(persistedRows[0]) });
    } else if (case_.id === "READ-001") {
      const now = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)', [t1, alice, "active fixture todo", "open", pg ? new Date(now).toISOString() : now]);
      const rows = await execute('SELECT id, owner_id, title, status, deleted_at FROM todo WHERE id = $1', [t1]);
      if (rows.length !== 1 || rows[0].owner_id !== alice || rows[0].status !== "open" || rows[0].deleted_at !== null) throw new Error("Missing READ-001 active Alice-owned todo fixture");
      raw.push({ fixture: { id: rows[0].id, ownerId: rows[0].owner_id, title: rows[0].title,
        status: rows[0].status, deletedAt: rows[0].deleted_at } });
      await send(case_.request);
    } else if (case_.id === "CREATE-001") {
      const users = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (users.length !== 1 || users[0].status !== "active" || !session?.credential || !session?.csrfToken || !session?.setCookie) throw new Error("Missing CREATE-001 active Alice session fixture");
      raw.push({ fixture: { subject: users[0].authentication_subject, userId: users[0].id, status: users[0].status,
        clockNow: fixedCreateClockNow, sessionAvailable: true, csrfAvailable: true } });
      const reply = await send(case_.request);
      const rows = await execute('SELECT id, owner_id, title, status, due_at, created_at, updated_at, deleted_at FROM "todo" WHERE id = $1', [reply.body.id]);
      if (rows.length !== 1) throw new Error("Missing CREATE-001 persisted todo readback");
      raw.push({ persistence: { id: rows[0].id, ownerId: rows[0].owner_id, title: rows[0].title,
        status: rows[0].status, dueAt: rows[0].due_at, createdAt: rows[0].created_at,
        updatedAt: rows[0].updated_at, deletedAt: rows[0].deleted_at } });
    } else if (case_.id === "CREATE-003") {
      raw.push({ fixture: { clockNow: fixedCreateClockNow } });
      await send(case_.request);
    } else if (case_.id === "READ-002") {
      const now = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)', [t2, bob, "private bob todo", "open", pg ? new Date(now).toISOString() : now]);
      for (const request of case_.requests) await send(request);
    } else if (case_.id === "AUTH-008") {
      const rows = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (rows.length !== 1 || rows[0].status !== "active") throw new Error("Missing AUTH-008 active Alice fixture");
      apiBearerIssuedAt = Date.now();
      apiBearerExpiresAt = apiBearerIssuedAt + 300_000;
      apiBearer = await app.authenticationHost().issue("api_bearer", "alice", apiBearerExpiresAt, apiBearerIssuedAt);
      if (!apiBearer?.credential) throw new Error("Could not issue AUTH-008 signed API bearer");
      const claims = await inspectUserBearer(apiBearer.credential);
      if (!claims.signatureValid || claims.subject !== rows[0].authentication_subject || claims.userId !== rows[0].id) throw new Error("AUTH-008 signed bearer does not match active Alice");
      secrets = [...secrets, apiBearer.credential];
      raw.push({ fixture: { subject: rows[0].authentication_subject, userId: rows[0].id, status: rows[0].status,
        credentialId: claims.credentialId, credentialIssuedAt: claims.issuedAt, credentialExpiresAt: claims.expiresAt, tokenClaims: claims,
        cacheAdapter: "unavailable; no cache adapter is configured" } });
      await send(case_.request);
    } else if (case_.id === "AUTH-009") {
      const createdAt = Date.now();
      await execute('INSERT INTO "service" (id, owner_id, name, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [reportingService, alice, "reporting", "active", pg ? new Date(createdAt).toISOString() : createdAt]);
      const rows = await execute('SELECT id, name, status, disabled_at FROM "service" WHERE id = $1', [reportingService]);
      if (rows.length !== 1 || rows[0].status !== "active" || rows[0].disabled_at !== null) throw new Error("Missing AUTH-009 persisted active service fixture");
      serviceCredentialIssuedAt = Date.now();
      serviceCredentialExpiresAt = serviceCredentialIssuedAt + 300_000;
      serviceCredential = await app.authenticationHost().issueServiceCredential("api_bearer", "reporting", serviceCredentialExpiresAt, serviceCredentialIssuedAt);
      if (!serviceCredential?.credential || serviceCredential.serviceId !== rows[0].id) throw new Error("Could not issue AUTH-009 service key for persisted fixture");
      secrets = [...secrets, serviceCredential.credential];
      raw.push({ fixture: { subject: rows[0].name, serviceId: rows[0].id, status: rows[0].status, disabledAt: rows[0].disabled_at,
        credentialId: serviceCredential.credentialId, credentialIssuedAt: serviceCredentialIssuedAt, credentialExpiresAt: serviceCredentialExpiresAt } });
      const waitUntil = serviceCredentialIssuedAt + 1;
      for (let attempt = 0; Date.now() <= waitUntil && attempt < 100; attempt++) await new Promise(resolve => setTimeout(resolve, 1));
      if (Date.now() <= waitUntil) throw new Error("Could not sequence AUTH-009 request after credential issuance");
      await send(case_.request);
    } else if (case_.id === "AUTH-010") {
      const rows = await execute('SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (rows.length !== 1 || rows[0].status !== "active") throw new Error("Missing AUTH-010 active Alice fixture");
      apiBearerIssuedAt = Date.now();
      apiBearerExpiresAt = apiBearerIssuedAt + 300_000;
      apiBearer = await app.authenticationHost().issue("api_bearer", "alice", apiBearerExpiresAt, apiBearerIssuedAt);
      if (!apiBearer?.credential) throw new Error("Could not issue AUTH-010 bounded Alice bearer");
      const claims = await inspectUserBearer(apiBearer.credential);
      if (!claims.signatureValid || claims.subject !== rows[0].authentication_subject || claims.userId !== rows[0].id) throw new Error("AUTH-010 signed bearer does not match active Alice");
      secrets = [...secrets, apiBearer.credential];
      raw.push({ fixture: { subject: rows[0].authentication_subject, userId: rows[0].id, status: rows[0].status,
        credentialId: claims.credentialId, credentialIssuedAt: claims.issuedAt, credentialExpiresAt: claims.expiresAt,
        tokenClaims: claims } });
      await send(case_.request);
      const persistedRows = await execute('SELECT id, authentication_subject, status, disabled_at FROM "user" WHERE id = $1', [rows[0].id]);
      if (persistedRows.length !== 1) throw new Error("Missing AUTH-010 disabled Alice persistence readback");
      const disabledAt = persistedRows[0].disabled_at instanceof Date ? persistedRows[0].disabled_at.getTime()
        : typeof persistedRows[0].disabled_at === "number" ? persistedRows[0].disabled_at : Date.parse(persistedRows[0].disabled_at);
      if (!Number.isFinite(disabledAt)) throw new Error("Invalid AUTH-010 persisted disable timestamp");
      raw.push({ persistence: { userId: persistedRows[0].id, subject: persistedRows[0].authentication_subject,
        status: persistedRows[0].status, disabledAt } });
    } else if (case_.id === "AUTH-006") {
      const disabledAt = Math.max(Date.now(), sessionIssuedAt + 10);
      const updated = await execute('UPDATE "user" SET status = $1, disabled_at = $2 WHERE authentication_subject = $3 RETURNING id', ["disabled", pg ? new Date(disabledAt).toISOString() : disabledAt, "alice"]);
      if (updated.length !== 1) throw new Error("Missing AUTH-006 user update");
      const rows = await execute('SELECT authentication_subject, status, disabled_at FROM "user" WHERE authentication_subject = $1', ["alice"]);
      if (rows.length !== 1) throw new Error("Missing AUTH-006 persisted user fixture");
      const storedDisabledAt = rows[0].disabled_at instanceof Date ? rows[0].disabled_at.getTime() : typeof rows[0].disabled_at === "number" ? rows[0].disabled_at : Date.parse(rows[0].disabled_at);
      if (!Number.isFinite(storedDisabledAt)) throw new Error("Invalid AUTH-006 persisted disable timestamp");
      raw.push({ fixture: { subject: rows[0].authentication_subject, status: rows[0].status, disabledAt: storedDisabledAt, issuedAt: sessionIssuedAt, credentialExpiresAt: sessionExpiresAt } });
      for (let attempt = 0; Date.now() <= storedDisabledAt && attempt < 1000; attempt++) await new Promise(resolve => setTimeout(resolve, 1));
      if (Date.now() <= storedDisabledAt) throw new Error("Could not sequence AUTH-006 request after persisted disable time");
      await send(case_.request);
    } else if (case_.id === "AUTH-011") {
      const createdAt = Date.now();
      await execute('INSERT INTO "service" (id, owner_id, name, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [reportingService, alice, "reporting", "active", pg ? new Date(createdAt).toISOString() : createdAt]);
      serviceCredentialIssuedAt = Date.now();
      serviceCredentialExpiresAt = serviceCredentialIssuedAt + 300_000;
      serviceCredential = await app.authenticationHost().issueServiceCredential("api_bearer", "reporting", serviceCredentialExpiresAt, serviceCredentialIssuedAt);
      if (!serviceCredential?.credential) throw new Error("Could not issue AUTH-011 service credential");
      secrets = [...secrets, serviceCredential.credential];
      const disabledAt = Math.max(Date.now(), serviceCredentialIssuedAt + 10);
      const updated = await execute('UPDATE "service" SET status = $1, disabled_at = $2 WHERE id = $3 RETURNING id', ["disabled", pg ? new Date(disabledAt).toISOString() : disabledAt, reportingService]);
      if (updated.length !== 1) throw new Error("Missing AUTH-011 service update");
      const rows = await execute('SELECT name, status, disabled_at FROM "service" WHERE id = $1', [reportingService]);
      if (rows.length !== 1) throw new Error("Missing AUTH-011 persisted service fixture");
      const storedDisabledAt = rows[0].disabled_at instanceof Date ? rows[0].disabled_at.getTime() : typeof rows[0].disabled_at === "number" ? rows[0].disabled_at : Date.parse(rows[0].disabled_at);
      if (!Number.isFinite(storedDisabledAt)) throw new Error("Invalid AUTH-011 persisted disable timestamp");
      raw.push({ fixture: { subject: rows[0].name, status: rows[0].status, disabledAt: storedDisabledAt, credentialIssuedAt: serviceCredentialIssuedAt, credentialExpiresAt: serviceCredentialExpiresAt } });
      for (let attempt = 0; Date.now() <= storedDisabledAt && attempt < 1000; attempt++) await new Promise(resolve => setTimeout(resolve, 1));
      if (Date.now() <= storedDisabledAt) throw new Error("Could not sequence AUTH-011 request after persisted disable time");
      await send(case_.request);
    } else if (case_.id === "PATCH-003") {
      const now = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)', [t1, alice, "active fixture todo", "open", pg ? new Date(now).toISOString() : now]);
      const rows = await execute('SELECT id, owner_id, status, deleted_at FROM todo WHERE id = $1', [t1]);
      if (rows.length !== 1) throw new Error("Missing PATCH-003 fixture todo");
      raw.push({ fixture: { id: rows[0].id, ownerId: rows[0].owner_id, status: rows[0].status, deletedAt: rows[0].deleted_at } });
      await send(case_.request);
    } else if (case_.id === "PATCH-004") {
      const now = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)', [t2, bob, "private bob todo", "open", pg ? new Date(now).toISOString() : now]);
      const rows = await execute('SELECT id, owner_id, title, status, deleted_at FROM todo WHERE id = $1', [t2]);
      if (rows.length !== 1 || rows[0].owner_id !== bob || rows[0].status !== "open" || rows[0].deleted_at !== null) throw new Error("Missing PATCH-004 persisted Bob-owned todo fixture");
      raw.push({ fixture: { id: rows[0].id, ownerId: rows[0].owner_id, title: rows[0].title,
        status: rows[0].status, deletedAt: rows[0].deleted_at } });
      await send(case_.request);
      const persisted = await execute('SELECT id, owner_id, title, status, deleted_at FROM todo WHERE id = $1', [t2]);
      if (persisted.length !== 1) throw new Error("Missing PATCH-004 Bob todo persistence readback");
      raw.push({ persistence: { id: persisted[0].id, ownerId: persisted[0].owner_id, title: persisted[0].title,
        status: persisted[0].status, deletedAt: persisted[0].deleted_at } });
    } else if (case_.id === "DELETE-001") {
      const createdAt = Date.now();
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)',
        [t1, alice, "active fixture todo", "open", pg ? new Date(createdAt).toISOString() : createdAt]);
      const rows = await execute('SELECT id, owner_id, title, status, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (rows.length !== 1 || rows[0].owner_id !== alice || rows[0].status !== "open" || rows[0].deleted_at !== null) {
        throw new Error("Missing DELETE-001 persisted active Alice-owned todo fixture");
      }
      const timestamp = (value: any) => {
        const millis = value instanceof Date ? value.getTime() : typeof value === "number" ? value : Date.parse(value);
        if (!Number.isFinite(millis)) throw new Error("Invalid DELETE-001 persisted timestamp");
        return new Date(millis).toISOString();
      };
      raw.push({ fixture: { id: rows[0].id, ownerId: rows[0].owner_id, title: rows[0].title,
        status: rows[0].status, createdAt: timestamp(rows[0].created_at),
        updatedAt: timestamp(rows[0].updated_at), deletedAt: rows[0].deleted_at, clockNow: fixedCreateClockNow } });
      await send(case_.request);
      const persisted = await execute('SELECT id, owner_id, title, status, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (persisted.length !== 1) throw new Error("Missing DELETE-001 soft-deleted todo persistence readback");
      raw.push({ persistence: { id: persisted[0].id, ownerId: persisted[0].owner_id, title: persisted[0].title,
        status: persisted[0].status, createdAt: timestamp(persisted[0].created_at),
        updatedAt: timestamp(persisted[0].updated_at),
        deletedAt: persisted[0].deleted_at === null ? null : timestamp(persisted[0].deleted_at) } });
      await send({ method: "GET", path: case_.request.path, auth: case_.request.auth });
    } else if (case_.id === "DELETE-002") {
      const toIso = (value: any) => {
        const millis = value instanceof Date ? value.getTime() : typeof value === "number" ? value : Date.parse(value);
        if (!Number.isFinite(millis)) throw new Error("Invalid DELETE-002 persisted timestamp");
        return new Date(millis).toISOString();
      };
      const createdAt = "2026-01-15T11:00:00.000Z";
      const updatedAt = "2026-01-15T11:59:00.000Z";
      const deletedAt = "2026-01-15T11:59:00.000Z";
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $6, $7)',
        [t1, alice, "previously deleted todo", "open", pg ? createdAt : Date.parse(createdAt),
          pg ? updatedAt : Date.parse(updatedAt), pg ? deletedAt : Date.parse(deletedAt)]);
      const rows = await execute('SELECT id, owner_id, title, status, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (rows.length !== 1 || rows[0].owner_id !== alice || rows[0].status !== "open"
        || rows[0].deleted_at === null || toIso(rows[0].deleted_at) !== deletedAt) {
        throw new Error("Missing DELETE-002 persisted deleted Alice-owned todo fixture");
      }
      const projection = (row: any) => ({ id: row.id, ownerId: row.owner_id, title: row.title,
        status: row.status, createdAt: toIso(row.created_at), updatedAt: toIso(row.updated_at),
        deletedAt: toIso(row.deleted_at) });
      raw.push({ fixture: projection(rows[0]) });
      await send(case_.request);
      const persisted = await execute('SELECT id, owner_id, title, status, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (persisted.length !== 1) throw new Error("Missing DELETE-002 unchanged persistence readback");
      raw.push({ persistence: projection(persisted[0]) });
    } else if (case_.id === "USER-001") {
      const users = await execute('SELECT id, authentication_subject, status, disabled_at FROM "user" WHERE id = $1', [alice]);
      if (users.length !== 1 || users[0].authentication_subject !== "alice" || users[0].status !== "active" || users[0].disabled_at !== null
        || sessionIssuedAt !== Date.parse(userSessionIssuedAt) || sessionExpiresAt !== Date.parse(userSessionExpiresAt)
        || !session?.credential || !session?.csrfToken || !session?.setCookie) throw new Error("Missing USER-001 active Alice/session fixture");
      await execute('INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5, NULL)',
        [t1, alice, "retained user todo", "open", pg ? new Date(sessionIssuedAt).toISOString() : sessionIssuedAt]);
      const todos = await execute('SELECT id, owner_id, title, status, due_at, created_at, updated_at, deleted_at FROM todo WHERE id = $1', [t1]);
      if (todos.length !== 1 || todos[0].owner_id !== alice || todos[0].status !== "open" || todos[0].deleted_at !== null) throw new Error("Missing USER-001 active Alice-owned Todo fixture");
      raw.push({ fixture: { user: { id: users[0].id, subject: users[0].authentication_subject, status: users[0].status, disabledAt: null },
        todo: { id: todos[0].id, ownerId: todos[0].owner_id, title: todos[0].title, status: todos[0].status,
          dueAt: todos[0].due_at, createdAt: timestamp(todos[0].created_at), updatedAt: timestamp(todos[0].updated_at), deletedAt: todos[0].deleted_at },
        session: { issuedAt: sessionIssuedAt, expiresAt: sessionExpiresAt, available: true, csrfAvailable: true } } });
      await withFixedClock(userDisabledAt, () => send(case_.request, true));
      raw.push(await userTodoReadback("after_disable"));
      await withFixedClock(userRepeatedDisableAt, () => send(case_.followUps[0].request, true));
      const refresh = await withFixedClock(userRefreshAt, async () => {
        const statements = beginSqlObservation();
        let outcome = "rejected";
        let declaredFailure: string | null = null;
        let internalCode: string | null = null;
        let credentialReturned = false;
        try {
          const result = await app.authenticationHost().refresh("browser_session", session.credential, Date.now());
          credentialReturned = typeof result?.credential === "string";
          if (credentialReturned) secrets = [...secrets, result.credential];
          outcome = "returned";
        } catch (error: any) {
          declaredFailure = typeof error?.declaredFailure === "string" ? error.declaredFailure : null;
          internalCode = typeof error?.code === "string" ? error.code : null;
        } finally { endSqlObservation(); }
        const compatibility = JSON.parse(readFileSync(join(project, "build/compatibility/public-failure-codes.json"), "utf8"));
        const publicFailures = compatibility.current.filter((row: any) => row.failure === declaredFailure);
        return { refresh: { at: Date.now(), outcome, declaredFailure, internalCode, credentialReturned,
          publicFailureCode: publicFailures.length === 1 ? publicFailures[0].code : null, statements: [...statements] } };
      });
      if (secrets.some(secret => JSON.stringify(refresh).includes(secret))) throw new Error("Unsafe USER-001 refresh observation refused");
      raw.push(refresh);
      await withFixedClock(userSessionExpiresAt, () => send(case_.followUps[2].request));
      raw.push(await userTodoReadback("final"));
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
    } finally { restoreClock?.(); }
  }
  writeFileSync(output, JSON.stringify({ backend, supported, results }, null, 2) + "\n", { flag: "wx" });
} finally {
  server.stop(true); db?.close(); if (pg) await pg.close(); prototype.prepare = originalPrepare; prototype.exec = originalExec;
}
