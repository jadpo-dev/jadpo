// Real migrated-application observations for the bounded HTTP slice.
// Receives the full frozen contract; never reads expected values to emit results.
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { beginSqlObservation, endSqlObservation, recordStatement, authenticationAttemptCount, authenticatedPrincipals, issuedCredentialCount, setSessionStorageUnavailable, authenticationFaultConfiguration, authenticationStorageReads, observedJwtVerifications, resolvedAuthorityPrincipals } from "./golden_sql_observer.ts";

const supported = ["AUTH-001", "AUTH-005", "AUTH-012", "PUBLIC-001", "CREATE-001", "CREATE-002", "CREATE-003", "CREATE-004", "READ-001", "READ-002", "READ-003", "LIST-001", "LIST-002", "LIST-003", "LIST-004", "REL-001", "PATCH-001", "PATCH-002", "PATCH-003", "PATCH-004", "PATCH-005", "DELETE-001", "DELETE-002", "AUTH-002", "AUTH-006", "AUTH-010", "USER-001", "AUTH-003", "AUTH-004", "AUTH-007", "AUTH-008", "AUTH-009", "AUTH-011"];
const partial = ["REL-002", "REL-003", "REL-004"];
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
const nativeFetch = globalThis.fetch;
const { SignJWT, generateKeyPair, exportJWK, decodeJwt, decodeProtectedHeader } = await import(pathToFileURL(join(project, "build/target/node_modules/jose/dist/webapi/index.js")).href);
const jwtKeys = await generateKeyPair("RS256", { modulusLength: 2048, extractable: true });
const publicJwk = { ...await exportJWK(jwtKeys.publicKey), kid: "checkpoint-rsa", alg: "RS256", use: "sig" };
const fixtureCertificate = readFileSync(join(project, "../jwks-fixture-cert.pem"));
const certificateSha256 = createHash("sha256").update(fixtureCertificate).digest("hex");
const providerRequests: any[] = [];
const jwksServer = Bun.serve({ hostname: "127.0.0.1", port: 0,
  tls: { cert: fixtureCertificate, key: readFileSync(join(project, "../jwks-fixture-key.pem")) },
  fetch(request) {
    const path = new URL(request.url).pathname;
    const body = path === "/.well-known/openid-configuration" ? { issuer: jwksServer.url.origin, jwks_uri: new URL("/jwks", jwksServer.url).href } : path === "/jwks" ? { keys: [publicJwk] } : null;
    providerRequests.push({ method: request.method, path, authorizationPresent: request.headers.has("authorization"), status: body ? 200 : 404, body });
    return Response.json(body, { status: body ? 200 : 404 });
  },
});
// Trust only this ephemeral fixture CA for its exact local issuer origin.
// The compiler's verifier still performs real discovery/JWKS HTTP fetches,
// JOSE signature/claim verification and native authority resolution.
globalThis.fetch = ((input: any, init?: any) => {
  const url = new URL(input instanceof Request ? input.url : String(input));
  return nativeFetch(input, url.origin === jwksServer.url.origin ? { ...init, tls: { ca: fixtureCertificate } } : init);
}) as typeof fetch;
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const persistenceModule = await import(pathToFileURL(join(project, "build/target/persistence.ts")).href);
const routes = JSON.parse(readFileSync(join(project, "build/inventory/routes.json"), "utf8")).routes;
const queryPlans = JSON.parse(readFileSync(join(project, "build/persistence/entities.json"), "utf8")).query_plans.filter((plan: any) => plan.entity === "Todo" && plan.strategy === "indexed_keyset_page");
const declaredFailures = JSON.parse(readFileSync(join(project, "build/audit/failures.json"), "utf8")).failures;
const db = backend === "sqlite" ? new Database(Bun.env.SQLITE_PATH, { strict: true }) : undefined;
db?.exec("PRAGMA foreign_keys = ON");
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
  BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: jwksServer.url.origin, OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"), MAIL_SENDER: "todo@example.test",
};
const reporting = "00000000-0000-4000-8000-000000000041";
let serviceKey: any;
let serviceIssuance: any;
let session: any;
let bearer: any;
let issuanceObservation: any;
let jwtCredentials: Record<string, string> = {};
let jwtFixtures: Record<string, any> = {};
let secrets: string[] = Object.values(environment).filter(value => value === environment.SESSION_SIGNING_KEY || value === environment.MAIL_API_KEY);
async function reset(case_?: any) {
  setSessionStorageUnavailable(false);
  providerRequests.length = 0;
  clockNow = nativeDate.parse(case_?.timeline?.issuedAt ?? contract.clock);
  await app.initializeApplication(environment);
  for (const table of ["service_credential", "__jadpo_auth_service_credentials", "service", "todo", "__jadpo_auth_sessions", "user"]) await execute(`DELETE FROM "${table}"`);
  const now = Date.now();
  for (const [id, subject] of [[alice, "alice"], [bob, "bob"]]) await execute('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [id, subject, `${subject}@example.test`, "active", pg ? new Date(now).toISOString() : now]);
  const statements = beginSqlObservation();
  try { session = await app.authenticationHost().issue("browser_session", "alice", case_?.timeline ? nativeDate.parse(case_.timeline.credentialExpiresAt) : now + 300_000, now); }
  finally { endSqlObservation(); }
  issuanceObservation = { operation: "compiler-owned authentication issuance", strategy: "browser_session", statements, credentialsIssued: issuedCredentialCount() };
  if (["AUTH-010", "AUTH-008"].includes(case_?.id)) {
    const statements = beginSqlObservation();
    try { bearer = await app.authenticationHost().issue("api_bearer", "alice", now + 300_000, now); }
    finally { endSqlObservation(); }
    issuanceObservation = { operation: "compiler-owned authentication issuance", strategy: "api_bearer", statements, credentialsIssued: issuedCredentialCount() };
    secrets.push(bearer.credential, bearer.csrfToken);
  }
  secrets = [...secrets, session.credential, session.csrfToken];
  jwtCredentials = {}; jwtFixtures = {};
  if (["AUTH-003", "AUTH-004", "AUTH-007"].includes(case_?.id)) {
    const name = case_.id === "AUTH-004" ? "bob-jwt" : "alice-jwt";
    const seconds = Math.floor(clockNow / 1000);
    jwtCredentials[name] = await new SignJWT({ iss: environment.OIDC_ISSUER, aud: environment.OIDC_AUDIENCE, sub: name === "bob-jwt" ? "bob" : "alice", iat: seconds, exp: seconds + 120, ...(case_.id === "AUTH-007" ? { email: "attacker@example.test" } : {}) })
      .setProtectedHeader({ alg: "RS256", kid: publicJwk.kid, typ: "JWT" }).sign(jwtKeys.privateKey);
    secrets.push(jwtCredentials[name]);
    jwtFixtures[name] = { name, header: decodeProtectedHeader(jwtCredentials[name]), claims: decodeJwt(jwtCredentials[name]), publicJwk };
  }
}
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
const raw: any[] = [];
const persistedInstant = (value: unknown) => value === null ? null : new Date(value as any).toISOString();
const stringDigest = (value: string) => createHash("sha256").update(value).digest("hex");
async function serviceSnapshot() {
  const rows = await execute('SELECT id, owner_id, name, status, disabled_at FROM service WHERE id = $1', [reporting]);
  if (rows.length !== 1) throw new Error("Missing service fixture");
  return { ...rows[0], disabled_at: persistedInstant(rows[0].disabled_at) };
}
async function serviceCredentialSnapshot() {
  const rows = await execute('SELECT id, service_id, status, expires_at, revoked_at FROM service_credential WHERE id = $1', [serviceKey.credentialId]);
  if (rows.length !== 1) throw new Error("Missing declared service credential fixture");
  const privateRows = await execute('SELECT revoked FROM "__jadpo_auth_service_credentials" WHERE id = $1', [serviceKey.credentialId]);
  if (privateRows.length !== 1) throw new Error("Missing native service credential metadata");
  return { ...rows[0], expires_at: persistedInstant(rows[0].expires_at), revoked_at: persistedInstant(rows[0].revoked_at), privateMetadataRevoked: privateRows[0].revoked === true || privateRows[0].revoked === 1 };
}
async function prepareServiceFixture(case_: any) {
  await execute('INSERT INTO service (id, owner_id, name, status, created_at, disabled_at) VALUES ($1, $2, $3, $4, $5, NULL)', [reporting, alice, "reporting", "active", pg ? new Date(clockNow).toISOString() : clockNow]);
  const statements = beginSqlObservation();
  try { serviceKey = await app.authenticationHost().issueServiceCredential("api_bearer", "reporting", clockNow + 300_000, clockNow); }
  finally { endSqlObservation(); }
  secrets.push(serviceKey.credential);
  serviceIssuance = { statements, credentialsIssued: issuedCredentialCount(), authorityPrincipals: resolvedAuthorityPrincipals() };
  const issuedService = await serviceSnapshot();
  const controlStatements = beginSqlObservation();
  let validProof = false;
  try { const proof = await app.authenticationHost().prepareDeliveryCredential("api_bearer", serviceKey.credential, clockNow); validProof = Object.isFrozen(proof) && Object.getPrototypeOf(proof) === null; }
  finally { endSqlObservation(); }
  const validityControl = { completed: validProof, statements: controlStatements, authorityPrincipals: resolvedAuthorityPrincipals(), at: new Date(clockNow).toISOString() };
  if (case_.id === "AUTH-011") await execute('UPDATE service SET status = $1, disabled_at = $2 WHERE id = $3', ["disabled", pg ? new Date(clockNow).toISOString() : clockNow, reporting]);
  raw.push({ serviceFixture: { issuedService, service: await serviceSnapshot(), credential: await serviceCredentialSnapshot(), issuance: serviceIssuance, validityControl, at: new Date(clockNow).toISOString(),
    specification: { fixture: case_.fixture, request: case_.request, clock: contract.clock } } });
}
async function revokeServiceControl() {
  const before = await serviceCredentialSnapshot();
  const statements = beginSqlObservation();
  try { await app.authenticationHost().revokeServiceCredential("api_bearer", serviceKey.credentialId, clockNow); }
  finally { endSqlObservation(); }
  raw.push({ serviceRevocation: { before, after: await serviceCredentialSnapshot(), statements } });
  await send({ method: "POST", path: "/auth/exchange", auth: "reporting-service-key" }, "serviceRevokedControl");
}
async function relationshipOperation(case_: any) {
  const missingUser = "00000000-0000-4000-8000-000000000099";
  if (case_.id === "REL-004") {
    await seedTodo(t1, alice);
    await execute('UPDATE "user" SET status = $1, disabled_at = $2 WHERE id = $3', ["disabled", pg ? new Date(clockNow).toISOString() : clockNow, alice]);
  }
  const before = { users: await execute('SELECT id, status FROM "user" ORDER BY id'), todos: await execute('SELECT id, owner_id FROM todo ORDER BY id') };
  const statements = beginSqlObservation();
  let accepted = false; let fault: any = null;
  try {
    const executeOperation = () => case_.id === "REL-002" ? seedTodo(t1, missingUser) : execute('DELETE FROM "user" WHERE id = $1', [alice]);
    await persistenceModule.persistenceAsync("checkpoint.relationship", executeOperation);
    accepted = true;
  } catch (error: any) {
    let cause: any = error; const driverCodes: any[] = []; const seen = new Set();
    while (cause && typeof cause === "object" && !seen.has(cause) && seen.size < 8) {
      seen.add(cause); if (typeof cause.code === "string") driverCodes.push(cause.code);
      if (typeof cause.errno === "number" || typeof cause.errno === "string" && /^[A-Z0-9_]{1,64}$/u.test(cause.errno)) driverCodes.push(String(cause.errno));
      cause = cause.cause;
    }
    fault = { name: error.name, kind: error.kind, operation: error.operation, driverCodes };
  } finally { endSqlObservation(); }
  raw.push({ persistenceOperation: case_.operation, at: new Date(clockNow).toISOString(), accepted, fault, statements,
    publicBoundaryObserved: false, before,
    after: { users: await execute('SELECT id, status FROM "user" ORDER BY id'), todos: await execute('SELECT id, owner_id FROM todo ORDER BY id') } });
}
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
  const credential = ["AUTH-010", "AUTH-008"].includes(case_.id) ? bearer : session;
  raw.push({ authenticationFixture: { user: await userSnapshot(), sessionRecord: await sessionSnapshot(["AUTH-010", "AUTH-008"].includes(case_.id) ? "api_bearer" : "browser_session"),
    issuedAt: new Date(clockNow).toISOString(), credentialExpiresAt: new Date(credential.expires).toISOString(),
    issuanceObservation, specification: { fixture: case_.fixture, timeline: case_.timeline ?? null } } });
}
async function captureJwtFixture(case_: any) {
  const users = await execute('SELECT id, authentication_subject, email, status FROM "user" ORDER BY id');
  raw.push({ jwtFixture: { credentials: Object.values(jwtFixtures), users, issuer: jwksServer.url.origin, audience: environment.OIDC_AUDIENCE, certificateSha256,
    specification: { fixture: case_.fixture, request: case_.request, clock: contract.clock } } });
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
async function send(request: any, retainKey?: string) {
  const headers: Record<string, string> = {};
  const auth = Array.isArray(request.auth) ? request.auth : [request.auth];
  if (auth.includes("alice-session")) Object.assign(headers, { cookie: session.setCookie.split(";", 1)[0], origin: environment.BROWSER_ORIGIN, "x-jadpo-csrf": session.csrfToken });
  for (const name of ["alice-jwt", "bob-jwt"]) if (auth.includes(name)) headers.authorization = `Bearer ${jwtCredentials[name]}`;
  if (auth.includes("reporting-service-key")) headers.authorization = `Bearer ${serviceKey.credential}`;
  if (auth.includes("alice-bounded-bearer")) headers.authorization = `Bearer ${bearer.credential}`;
  if (auth.includes("invalid-jwt") || auth.includes("invalid-bearer")) headers.authorization = "Bearer INVALID_CHECKPOINT_CREDENTIAL";
  const path = request.path.replace("/todos/t2", `/todos/${t2}`).replace("/todos/t1", `/todos/${t1}`).replace("/users/alice/todos", `/users/${alice}/todos`).replace(/^\/users\/alice$/u, `/users/${alice}`);
  if (request.json !== undefined) headers["content-type"] = "application/json";
  const providerStart = providerRequests.length;
  const statements = beginSqlObservation();
  let response: Response;
  let body: any;
  let exchangeDisclosure: any;
  try {
    response = await fetch(new URL(path, server.url), { method: request.method, headers, ...(request.json === undefined ? {} : { body: JSON.stringify(request.json) }) });
    const text = await response.text();
    body = text.length ? JSON.parse(text) : null;
    if (request.path === "/auth/exchange" && response.status === 200) {
      if (typeof body?.access_token !== "string") throw new Error("Missing actual exchanged bearer");
      exchangeDisclosure = { incomingSecretInResponse: text.includes(serviceKey.credential), responseFields: Object.keys(body), returnedCredential: { sha256: stringDigest(body.access_token), byteLength: Buffer.byteLength(body.access_token), representation: body.access_token.split(".", 1)[0] } };
      secrets.push(body.access_token);
      body = { ...body, access_token: { ...exchangeDisclosure.returnedCredential } };
    }
  } finally { endSqlObservation(); }
  const authenticationAttempts = authenticationAttemptCount();
  const authorityQueries = authorityQueryAccounting(statements);
  const captured = { method: request.method, path, requestedPath: request.requestedPath ?? request.path, auth: request.auth ?? null, requestJson: request.json ?? null, at: new Date(clockNow).toISOString(), status: response!.status, body, statements, authenticationAttempts, principals: authenticatedPrincipals(), credentialsIssued: issuedCredentialCount(), authorityQueries, faultConfiguration: authenticationFaultConfiguration(), storageReads: authenticationStorageReads(), jwtVerifications: observedJwtVerifications(), providerRequests: providerRequests.slice(providerStart), authorityPrincipals: resolvedAuthorityPrincipals(), ...(exchangeDisclosure ? { exchangeDisclosure } : {}) };
  if (secrets.some(secret => JSON.stringify(captured).includes(secret))) throw new Error("Unsafe raw observation refused");
  raw.push(retainKey ? { [retainKey]: captured } : captured);
  return { status: response!.status, code: body?.error?.code, body, databaseQueries: statements.length, authenticationAttempts, writes: statements.filter(row => ["INSERT", "UPDATE", "DELETE", "REPLACE"].includes(row.operation)).length };
}
const results: any[] = [];
const partialEvidence: any[] = [];
try {
  for (const case_ of contract.cases.filter((case_: any) => supported.includes(case_.id) || partial.includes(case_.id))) {
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
    if (["AUTH-009", "AUTH-011"].includes(case_.id)) await prepareServiceFixture(case_);
    if (["REL-002", "REL-004"].includes(case_.id)) {
      await relationshipOperation(case_); partialEvidence.push({ id: case_.id, raw: raw.slice(start), disposition: "not_run_public_relationship_failure_mapping_missing" }); continue;
    }
    if (case_.id === "REL-003") {
      await seedTodo(t1, alice); raw.push({ ownerTransferFixture: await todoSnapshot(t1), operation: case_.operation });
      await send({ method: "POST", path: "/todos", auth: "alice-session", json: { title: "owner injection", owner_id: bob } });
      await send({ method: "PATCH", path: "/todos/t1", auth: "alice-session", json: { owner_id: bob } });
      raw.push({ ownerTransferPersistence: await todoSnapshot(t1), sourceProbeSha256: stringDigest(readFileSync(join(project, "../owner-partial-probe.json"), "utf8")) });
      partialEvidence.push({ id: case_.id, raw: raw.slice(start), disposition: "not_run_exact_transfer_human_decision_missing" }); continue;
    }
    if (["AUTH-003", "AUTH-004", "AUTH-007"].includes(case_.id)) {
      await captureJwtFixture(case_);
      if (case_.id === "AUTH-004") await send({ method: "GET", path: "/todos", auth: "bob-jwt" }, "jwtValidityControl");
    }
    if (["AUTH-002", "AUTH-006", "AUTH-010", "USER-001", "AUTH-008"].includes(case_.id)) await captureAuthenticationFixture(case_);
    if (case_.id === "AUTH-008") {
      setSessionStorageUnavailable(true);
      await send({ method: "DELETE", path: "/users/alice", auth: "alice-bounded-bearer" }, "cacheFaultControl");
    }
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
      if (case_.id === "AUTH-009") await revokeServiceControl();
      if (["CREATE-001", "READ-001", "AUTH-002", "AUTH-003", "AUTH-007"].includes(case_.id)) await validateOutput(case_.request, reply);
      if (case_.id === "AUTH-007") raw.push({ provisioningPersistence: await execute('SELECT id, authentication_subject, email, status FROM "user" ORDER BY id') });
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
      (partial.includes(case_.id) ? partialEvidence : results).push({ id: case_.id, adapterFailure: true, raw: raw.slice(start) });
    }
  }
  writeFileSync(output, JSON.stringify({ backend, supported, results, partialEvidence }, null, 2) + "\n", { flag: "wx" });
} finally {
  server.stop(true); jwksServer.stop(true); globalThis.fetch = nativeFetch; setSessionStorageUnavailable(false); db?.close(); if (pg) await pg.close(); prototype.prepare = originalPrepare; prototype.exec = originalExec; globalThis.Date = nativeDate;
}
