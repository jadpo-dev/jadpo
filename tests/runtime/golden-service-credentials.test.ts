import { copyGoldenFixture } from "./golden-fixture";
import { afterAll, beforeEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { connect } from "node:net";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// RM-102/RM-104: generated declared credential authority and exchange on
// SQLite and PostgreSQL. Trusted provisioning remains host-only.
const root = mkdtempSync(join(tmpdir(), "jadpo-golden-service-"));
const project = join(root, "application");
const previousSqlitePath = Bun.env.SQLITE_PATH;
const previousDatabaseUrl = Bun.env.DATABASE_URL;
const postgresUrl = Bun.env.JADPO_GOLDEN_SERVICE_DATABASE_URL;
let sqlite: Database | undefined;
let postgres: SQL | undefined;
let captureSql = false;
let capturedSql: string[] = [];
const originalPrepare = Database.prototype.prepare;
(Database.prototype as any).prepare = function (sql: string, ...parameters: any[]) {
  const statement = originalPrepare.call(this, sql, ...parameters);
  if (!captureSql) return statement;
  return new Proxy(statement, {
    get(target, property) {
      const value = Reflect.get(target, property, target);
      if (typeof value !== "function" || !["all", "get", "run", "values", "iterate"].includes(String(property))) return value;
      return (...arguments_: any[]) => {
        capturedSql.push(sql);
        return Reflect.apply(value, target, arguments_);
      };
    },
  });
};
afterAll(async () => {
  sqlite?.close();
  if (postgres) await postgres.close();
  if (previousSqlitePath === undefined) delete Bun.env.SQLITE_PATH;
  else Bun.env.SQLITE_PATH = previousSqlitePath;
  if (previousDatabaseUrl === undefined) delete Bun.env.DATABASE_URL;
  else Bun.env.DATABASE_URL = previousDatabaseUrl;
  rmSync(root, { recursive: true, force: true });
});
copyGoldenFixture(project);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Golden service fixture failed to build:\n${build.stdout}\n${build.stderr}`);
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
if (!existsSync(join(dependency, "node_modules/jose/package.json"))) throw new Error("Install the pinned JWT dependency before running this suite");
symlinkSync(join(dependency, "node_modules"), join(project, "build/target/node_modules"), "dir");
if (postgresUrl) {
  const url = new URL(postgresUrl);
  if (!["localhost", "127.0.0.1"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw new Error("Requires a disposable local authentication database");
  Bun.env.DATABASE_URL = postgresUrl;
} else delete Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = join(root, "golden.sqlite");
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
if (postgresUrl) postgres = new SQL({ url: postgresUrl, prepare: false });
else sqlite = new Database(Bun.env.SQLITE_PATH, { strict: true });
const db = {
  async exec(sql: string) { if (postgres) await postgres.unsafe(sql); else sqlite!.exec(sql); },
  async all(sql: string, ...values: any[]): Promise<any[]> {
    let index = 0;
    return postgres ? [...await postgres.unsafe(sql.replace(/\?/gu, () => `$${++index}`), values)] : sqlite!.prepare(sql).all(...values);
  },
};
const owner = "00000000-0000-4000-8000-000000000001";
const service = "00000000-0000-4000-8000-000000000002";
const otherService = "00000000-0000-4000-8000-000000000003";
const environment = {
  DATABASE_URL: "https://db.test/golden",
  SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test",
  OIDC_ISSUER: "https://issuer.test",
  OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"),
  MAIL_SENDER: "todo@example.test",
};
let now: number;
const instant = (value: number) => postgres ? new Date(value).toISOString() : value;
const millis = (value: unknown) => typeof value === "number" ? value : new Date(value as string).getTime();
beforeEach(async () => {
  await app.initializeApplication(environment);
  await db.exec('DELETE FROM "__jadpo_auth_service_credentials"; DELETE FROM "__jadpo_auth_sessions"; DELETE FROM service_credential; DELETE FROM todo; DELETE FROM service; DELETE FROM "user";');
  now = Date.now();
  await db.all('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)', owner, "owner", "owner@example.test", "active", instant(now), null);
  for (const [id, name] of [[service, "todo-service"], [otherService, "other-service"]]) {
    await db.all('INSERT INTO service (id, owner_id, name, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)', id, owner, name, "active", instant(now), null);
  }
});
const host = () => app.authenticationHost();
const issue = (subject = "todo-service", expires = now + 3_600_000) => host().issueServiceCredential("api_bearer", subject, expires, now);
const exchange = (credential: string, at = now) => host().exchangeServiceCredential("api_bearer", credential, at);
const rawRequest = (credential: string) => new Request("https://todo.test/todos", { headers: { authorization: `Bearer ${credential}` } });
const authenticate = (credential: string, fresh = false, at = now) => host().authenticate(rawRequest(credential), fresh, at);
const expectedPrincipal = { kind: "service", subject: "todo-service", authenticationStrength: "primary", values: { service_id: service } };
const declared = async (id: string) => (await db.all("SELECT * FROM service_credential WHERE id = ?", id))[0];
const metadata = async (id: string) => JSON.parse((await db.all('SELECT data FROM "__jadpo_auth_service_credentials" WHERE id = ?', id))[0].data);
async function denied(credential: string, code = "invalid_credentials", fresh = false, at = now) {
  await expect(authenticate(credential, fresh, at)).rejects.toMatchObject({ code });
}

test("trusted issuance atomically persists one declared verifier and private metadata without disclosing secrets", async () => {
  const events: string[] = [];
  const original = console.error;
  console.error = (...args: unknown[]) => { events.push(args.map(String).join(" ")); };
  try {
    const key = await issue();
    const row = await declared(key.credentialId);
    const privateRow = await metadata(key.credentialId);
    expect(row).toMatchObject({ id: key.credentialId, service_id: service, status: "active", revoked_at: null });
    expect(millis(row.expires_at)).toBe(now + 3_600_000);
    expect(millis(row.created_at)).toBe(now);
    expect(row.verifier).toMatch(/^[A-Za-z0-9_-]{43}$/u);
    expect(privateRow).not.toHaveProperty("verifier");
    const hmacKey = await crypto.subtle.importKey("raw", Buffer.from(environment.SESSION_SIGNING_KEY, "base64url"), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const verifier = Buffer.from(await crypto.subtle.sign("HMAC", hmacKey, new TextEncoder().encode(`service:todo-service-key:${key.credential}`))).toString("base64url");
    expect(row.verifier).toBe(verifier);
    const token = await exchange(key.credential);
    const principal = await authenticate(key.credential);
    expect(principal).toEqual(expectedPrincipal);
    expect(await authenticate(token.credential)).toEqual(expectedPrincipal);
    const tokenPayload = Buffer.from(token.credential.split(".")[2], "base64url").toString();
    for (const secret of [key.credential, environment.SESSION_SIGNING_KEY]) {
      expect(JSON.stringify([row, privateRow, principal, events, tokenPayload])).not.toContain(secret);
    }
    expect(JSON.stringify([principal, events, tokenPayload])).not.toContain(row.verifier);
    expect(await db.all("SELECT id FROM service_credential")).toHaveLength(1);
    expect(await db.all('SELECT id FROM "__jadpo_auth_service_credentials"')).toHaveLength(1);
  } finally { console.error = original; }
});

test("generated HTTP exchange returns the bounded bearer and proves same-call service authority without leaking the key", async () => {
  await db.all("UPDATE service SET name = ? WHERE id = ?", "reporting", service);
  const key = await issue("reporting", now + 120_000);
  const row = await declared(key.credentialId);
  const messages: string[] = [];
  const originalError = console.error;
  console.error = (...args: unknown[]) => { messages.push(args.map(String).join(" ")); };
  capturedSql = [];
  captureSql = true;
  let response: Response;
  try {
    response = await app.handleRequest(new Request("https://todo.test/auth/exchange", {
      method: "POST",
      headers: { authorization: `Bearer ${key.credential}` },
    }));
  } finally { captureSql = false; console.error = originalError; }

  if (!postgres) {
    const serviceAuthorityQueries = capturedSql.filter(sql => /FROM "service" WHERE "name" = \?/iu.test(sql));
    expect(serviceAuthorityQueries).toHaveLength(1);
    expect(serviceAuthorityQueries[0]).toContain('SELECT "id", "owner_id", "name", "status", "created_at", "disabled_at"');
    expect(serviceAuthorityQueries[0]).not.toMatch(/"email"/iu);
    expect(capturedSql.filter(sql => sql.includes('FROM "__jadpo_auth_service_credentials"'))).toHaveLength(1);
    expect(capturedSql.filter(sql => /FROM "service_credential"/iu.test(sql))).toHaveLength(1);
  }

  expect(response!.status).toBe(200);
  const body = await response!.json();
  expect(Object.keys(body).sort()).toEqual(["access_token", "expires_at", "token_type"]);
  expect(body.token_type).toBe("Bearer");
  expect(body.expires_at).toBe(new Date(millis(row.expires_at)).toISOString());
  expect(body.access_token).toMatch(/^jdx1\./u);
  expect(body).not.toHaveProperty("subject");
  expect(body).not.toHaveProperty("service_id");

  const audit = messages.map(message => JSON.parse(message)).filter(event => event.eventName === "service_credential.exchanged");
  expect(audit).toHaveLength(1);
  expect(audit[0]).toMatchObject({ kind: "authentication_audit", strategy: "api_bearer", serviceId: service, credentialId: key.credentialId });
  expect(audit[0]).not.toHaveProperty("subject");
  expect(audit[0]).not.toHaveProperty("credential");
  expect(audit[0]).not.toHaveProperty("verifier");

  const followed = await host().authenticate(new Request("https://todo.test/todos", {
    headers: { authorization: `Bearer ${body.access_token}` },
  }), false, Date.now());
  expect(followed).toEqual({ kind: "service", subject: "reporting", authenticationStrength: "primary", values: { service_id: service } });

  const inventory = JSON.parse(readFileSync(join(project, "build/inventory/routes.json"), "utf8"));
  expect(inventory.routes.find((route: any) => route.route === "POST /auth/exchange")).toMatchObject({
    behavior: "service_credential_exchange",
    success: { kind: "ok", http_status: 200 },
  });
  const openapi = JSON.parse(readFileSync(join(project, "build/openapi/openapi.json"), "utf8"));
  const operation = openapi.paths["/auth/exchange"].post;
  expect(operation.responses["200"].content["application/json"].schema.required).toEqual(["access_token", "token_type", "expires_at"]);
  expect(operation.responses).toHaveProperty("401");
  expect(operation.responses).toHaveProperty("403");
  expect(operation.responses).toHaveProperty("503");
  const authentication = JSON.parse(readFileSync(join(project, "build/audit/authentication.json"), "utf8"));
  expect(authentication.strategies.find((strategy: any) => strategy.name === "api_bearer").exchange).toMatchObject({
    method: "POST", path: "/auth/exchange", key_validator: "service_key", signed_validator: "service_signed",
    credential_selection: "exactly_one", raw_credential: "compiler_private_only",
  });
  const published = JSON.stringify({ inventory, openapi, authentication, body, audit });
  const storedVerifier = row.verifier;
  expect(published).not.toContain(key.credential);
  expect(published).not.toContain(storedVerifier);
  expect(published).not.toContain(environment.SESSION_SIGNING_KEY);
});

test("generated HTTP exchange remains valid after starting a fresh target process with the same credential store", async () => {
  const key = await issue();
  const exchanged = await app.handleRequest(new Request("https://todo.test/auth/exchange", {
    method: "POST", headers: { authorization: `Bearer ${key.credential}` },
  }));
  expect(exchanged.status).toBe(200);
  const body = await exchanged.json();

  const portProbe = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
  const port = portProbe.port;
  portProbe.stop(true);
  const childEnvironment: Record<string, string | undefined> = {
    PATH: Bun.env.PATH,
    ...(postgresUrl ? { DATABASE_URL: postgresUrl } : { SQLITE_PATH: join(root, "golden.sqlite") }),
    PORT: String(port),
    JADPO_APP_PATH: join(project, "build/target/app.ts"),
    JADPO_APP_CONFIG: JSON.stringify(environment),
  };
  const childScript = [
    'import { pathToFileURL } from "node:url";',
    'const app = await import(pathToFileURL(Bun.env.JADPO_APP_PATH!).href);',
    'await app.initializeApplication(JSON.parse(Bun.env.JADPO_APP_CONFIG!));',
    // Test-owned observation invokes the real generated credential validator.
    // The child cannot issue/provision credentials or replace its principal.
    'Bun.serve({ hostname: "127.0.0.1", port: Number(Bun.env.PORT), async fetch(request) {',
    '  if (new URL(request.url).pathname !== "/__fixture/service-principal") return app.handleRequest(request);',
    '  try { return Response.json(await app.authenticationHost().authenticate(request, new URL(request.url).searchParams.get("fresh") === "1", Date.now())); }',
    '  catch { return new Response(null, { status: 401 }); }',
    '} });',
    'console.log(JSON.stringify({kind:"service_restart_child",pid:process.pid,backend:Bun.env.DATABASE_URL?"postgres":"sqlite"}));',
  ].join("\n");
  const generatedPins = Object.fromEntries(["app.ts", "persistence.ts", "authentication.ts", "first-party-authentication.ts", "startup-owner.ts"].map(name => [name, createHash("sha256").update(readFileSync(join(project, "build/target", name))).digest("hex")]));
  const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", childScript], {
    cwd: project, env: childEnvironment, stdout: "pipe", stderr: "pipe",
  });
  const childStdout = new Response(child.stdout).text();
  const childStderr = new Response(child.stderr).text();
  const endpoint = (path: string, init?: RequestInit) => fetch(`http://127.0.0.1:${port}${path}`, { ...init, signal: AbortSignal.timeout(1000) });
  const principal = (credential: string, fresh = false) => endpoint(`/__fixture/service-principal?fresh=${fresh ? 1 : 0}`, { headers: { authorization: `Bearer ${credential}` } });
  let freshBody: any;
  try {
    let health: Response | undefined;
    const deadline = performance.now() + 2500;
    while (performance.now() < deadline && child.exitCode === null) {
      try { health = await endpoint("/health/live"); if (health.status === 200) break; }
      catch { /* Wait briefly for the generated target fixture to bind. */ }
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    expect(health?.status).toBe(200);
    // Both the original key and pre-restart signed envelope resolve the exact
    // generated service principal in the new process using the shared store.
    for (const credential of [key.credential, body.access_token]) {
      for (const fresh of [false, true]) {
        const response = await principal(credential, fresh);
        expect(response.status).toBe(200);
        expect(await response.json()).toEqual(expectedPrincipal);
      }
    }
    const freshExchange = await endpoint("/auth/exchange", { method: "POST", headers: { authorization: `Bearer ${key.credential}` } });
    expect(freshExchange.status).toBe(200);
    freshBody = await freshExchange.json();
    expect(Object.keys(freshBody).sort()).toEqual(["access_token", "expires_at", "token_type"]);
    expect(freshBody.token_type).toBe("Bearer");
    expect(freshBody.access_token).toMatch(/^jdx1\./u);
    const protectedResponse = await endpoint("/todos", {
      method: "POST", headers: { authorization: `Bearer ${body.access_token}`, "content-type": "application/json" },
      body: JSON.stringify({ title: "must stay service-separated" }),
    });
    expect(protectedResponse.status).toBe(403);
    expect((await protectedResponse.json()).error.code).toBe("not_permitted");

    await db.all("UPDATE service_credential SET status = 'revoked' WHERE id = ?", key.credentialId);
    expect((await principal(key.credential)).status).toBe(401);
    for (const credential of [body.access_token, freshBody.access_token]) {
      expect((await principal(credential, true)).status).toBe(401);
      const bounded = await principal(credential);
      expect(bounded.status).toBe(200);
      expect(await bounded.json()).toEqual(expectedPrincipal);
    }
    const revokedExchange = await endpoint("/auth/exchange", { method: "POST", headers: { authorization: `Bearer ${key.credential}` } });
    expect(revokedExchange.status).toBe(401);
    expect((await revokedExchange.json()).error.code).toBe("invalid_credentials");
    expect(child.exitCode).toBeNull();
  } finally {
    child.kill("SIGTERM");
    expect(await child.exited).toBe(143);
    expect(() => process.kill(child.pid, 0)).toThrow();
    await expect(endpoint("/health/live")).rejects.toThrow();
    const stdout = await childStdout, stderr = await childStderr;
    expect(JSON.parse(stdout.trim())).toEqual({ kind: "service_restart_child", pid: child.pid, backend: postgresUrl ? "postgres" : "sqlite" });
    for (const secret of [key.credential, body.access_token, freshBody?.access_token, environment.SESSION_SIGNING_KEY, environment.MAIL_API_KEY, postgresUrl].filter(Boolean)) {
      expect(stdout + stderr).not.toContain(secret!);
    }
    expect(stderr).not.toContain("SELECT");
  }
  for (const [name, digest] of Object.entries(generatedPins)) expect(createHash("sha256").update(readFileSync(join(project, "build/target", name))).digest("hex")).toBe(digest);
  console.log(JSON.stringify({ kind: "native_service_credential_restart", backend: postgresUrl ? "postgres" : "sqlite", parentPid: process.pid, childPid: child.pid, exchangeStatus: 200, revokedExchangeStatus: 401, freshSignedRevokedStatus: 401, boundedSignedStatus: 200, protectedMutationStatus: 403, exit: 143, childExited: true, generatedPins }));
}, 10000);

test("generated exchange rejects missing, malformed, competing, user and disabled credentials before returning a token", async () => {
  const key = await issue();
  const send = (headers?: HeadersInit) => app.handleRequest(new Request("https://todo.test/auth/exchange", { method: "POST", headers }));
  const expectError = async (response: Response, status: number, code: string) => {
    expect(response.status).toBe(status);
    const body = await response.json();
    expect(body.error.code).toBe(code);
    expect(body).not.toHaveProperty("access_token");
    expect(JSON.stringify(body)).not.toContain(key.credential);
  };
  await expectError(await send(), 401, "authentication_required");
  await expectError(await send({ authorization: "Basic invalid" }), 401, "invalid_credentials");
  await expectError(await send({ authorization: `Bearer ${key.credential}, Bearer ${key.credential}` }), 401, "ambiguous_credentials");
  await expectError(await send({ authorization: `Bearer ${key.credential}`, cookie: "todo_session=irrelevant" }), 401, "ambiguous_credentials");
  const user = await host().issue("api_bearer", "owner", now + 60_000, now);
  await expectError(await send({ authorization: `Bearer ${user.credential}` }), 401, "invalid_credentials");

  await db.all("UPDATE service SET status = 'disabled', disabled_at = ? WHERE id = ?", instant(now), service);
  await expectError(await send({ authorization: `Bearer ${key.credential}` }), 403, "service_disabled");
});

test("generated HTTP exchange rejects expired, revoked and wrong-audience service keys", async () => {
  const send = (credential: string) => app.handleRequest(new Request("https://todo.test/auth/exchange", {
    method: "POST", headers: { authorization: `Bearer ${credential}` },
  }));
  const expired = await issue();
  await db.all("UPDATE service_credential SET expires_at = ? WHERE id = ?", instant(now), expired.credentialId);
  expect((await send(expired.credential)).status).toBe(401);

  const revoked = await issue();
  await host().revokeServiceCredential("api_bearer", revoked.credentialId, now + 1);
  expect((await send(revoked.credential)).status).toBe(401);

  const wrongAudience = await issue();
  const hmacKey = await crypto.subtle.importKey(
    "raw", Buffer.from(environment.SESSION_SIGNING_KEY, "base64url"),
    { name: "HMAC", hash: "SHA-256" }, false, ["sign"],
  );
  const wrongVerifier = Buffer.from(await crypto.subtle.sign(
    "HMAC", hmacKey, new TextEncoder().encode(`service:wrong-audience:${wrongAudience.credential}`),
  )).toString("base64url");
  const privateRecord = await metadata(wrongAudience.credentialId);
  privateRecord.verifierDigest = Buffer.from(await crypto.subtle.digest(
    "SHA-256", new TextEncoder().encode(wrongVerifier),
  )).toString("hex");
  await db.all("UPDATE service_credential SET verifier = ? WHERE id = ?", wrongVerifier, wrongAudience.credentialId);
  await db.all('UPDATE "__jadpo_auth_service_credentials" SET data = ? WHERE id = ?', JSON.stringify(privateRecord), wrongAudience.credentialId);
  const wrongAudienceResponse = await send(wrongAudience.credential);
  expect(wrongAudienceResponse.status).toBe(401);
  expect((await wrongAudienceResponse.json()).error.code).toBe("invalid_credentials");
});

test("Bun-coalesced duplicate raw Authorization headers are ambiguous and audit failure returns no token", async () => {
  const key = await issue();
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
  try {
    const responseText = await new Promise<string>((resolve, reject) => {
      const socket = connect(server.port, "127.0.0.1");
      let received = "";
      socket.on("connect", () => socket.write(
        "POST /auth/exchange HTTP/1.1\r\n" +
        "Host: 127.0.0.1\r\n" +
        `Authorization: Bearer ${key.credential}\r\n` +
        `Authorization: Bearer ${key.credential}\r\n` +
        "Connection: close\r\n" +
        "Content-Length: 0\r\n\r\n",
      ));
      socket.on("data", chunk => { received += chunk.toString(); });
      socket.on("end", () => resolve(received));
      socket.on("error", reject);
    });
    const [headers, responseBody] = responseText.split("\r\n\r\n", 2);
    expect(headers).toMatch(/^HTTP\/1\.1 401\b/u);
    const duplicateResult = JSON.parse(responseBody);
    expect(duplicateResult.error.code).toBe("ambiguous_credentials");
    expect(JSON.stringify(duplicateResult)).not.toContain(key.credential);
  } finally {
    server.stop(true);
  }

  const originalError = console.error;
  console.error = () => { throw new Error("audit sink failure canary"); };
  let response: Response;
  try {
    response = await app.handleRequest(new Request("https://todo.test/auth/exchange", {
      method: "POST", headers: { authorization: `Bearer ${key.credential}` },
    }));
  } finally { console.error = originalError; }
  expect(response!.status).toBe(503);
  const failedAuditBody = await response!.json();
  expect(failedAuditBody.error.code).toBe("authentication_unavailable");
  expect(JSON.stringify(failedAuditBody)).not.toContain(key.credential);
});

test("declared credential expiry, active state and revocation remain authoritative on direct and fresh access", async () => {
  for (const [field, value] of [["expires_at", instant(now)], ["status", "revoked"], ["revoked_at", instant(now)]]) {
    const key = await issue();
    const token = await exchange(key.credential);
    await db.all(`UPDATE service_credential SET ${field} = ? WHERE id = ?`, value, key.credentialId);
    await denied(key.credential);
    await denied(token.credential, "invalid_credentials", true);
    await expect(exchange(key.credential)).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(host().refresh("api_bearer", token.credential, now)).rejects.toMatchObject({ code: "invalid_credentials" });
    expect(await authenticate(token.credential)).toEqual(expectedPrincipal);
    await denied(token.credential, "invalid_credentials", false, token.expires);
  }
});

test("disabled service blocks issuance, direct, exchange, refresh and fresh tokens while bounded tokens retain their cap", async () => {
  const key = await issue();
  const token = await exchange(key.credential);
  await db.all("UPDATE service SET status = 'disabled', disabled_at = ? WHERE id = ?", instant(now), service);
  for (const operation of [() => issue(), () => authenticate(key.credential), () => exchange(key.credential), () => authenticate(token.credential, true), () => host().refresh("api_bearer", token.credential, now)]) {
    await expect(operation()).rejects.toMatchObject({ code: "principal_inactive", declaredFailure: "ServiceDisabled" });
  }
  expect(await authenticate(token.credential)).toEqual(expectedPrincipal);
  expect(token.expires).toBe(now + 300_000);
  await denied(token.credential, "invalid_credentials", false, token.expires);
});

test("declared expiry caps exchanged and refreshed tokens while existing signed envelopes retain their own expiry", async () => {
  const key = await issue();
  const initial = await exchange(key.credential);
  const shortened = now + 60_000;
  await db.all("UPDATE service_credential SET expires_at = ? WHERE id = ?", instant(shortened), key.credentialId);
  const token = await exchange(key.credential);
  const refreshed = await host().refresh("api_bearer", initial.credential, now + 1);
  expect(token.expires).toBe(shortened);
  expect(refreshed.expires).toBe(shortened);
  expect(await authenticate(key.credential, false, shortened - 1)).toEqual(expectedPrincipal);
  for (const credential of [key.credential, token.credential, refreshed.credential]) await denied(credential, "invalid_credentials", false, shortened);
  // Extending authored expiry cannot extend an already signed envelope.
  const shortKey = await issue("todo-service", now + 120_000);
  const shortToken = await exchange(shortKey.credential);
  await db.all("UPDATE service_credential SET expires_at = ? WHERE id = ?", instant(now + 3_600_000), shortKey.credentialId);
  expect(shortToken.expires).toBe(now + 120_000);
  await denied(shortToken.credential, "invalid_credentials", false, shortToken.expires);
});

test("service owner disablement does not introduce an undeclared owner-active permission", async () => {
  await db.all('UPDATE "user" SET status = \'disabled\', disabled_at = ? WHERE id = ?', instant(now), owner);
  const key = await issue();
  const token = await exchange(key.credential);
  expect(await authenticate(key.credential)).toEqual(expectedPrincipal);
  expect(await authenticate(token.credential, true)).toEqual(expectedPrincipal);
});

test("rotation creates independent declared credentials and revocation updates only the selected credential", async () => {
  const first = await issue();
  const second = await issue();
  const token = await exchange(first.credential);
  expect(first.credentialId).not.toBe(second.credentialId);
  expect(first.credential).not.toBe(second.credential);
  await host().revokeServiceCredential("api_bearer", first.credentialId, now + 1);
  const row = await declared(first.credentialId);
  expect(row.status).toBe("active");
  expect(millis(row.revoked_at)).toBe(now + 1);
  expect((await declared(second.credentialId)).status).toBe("active");
  await denied(first.credential);
  await denied(token.credential, "invalid_credentials", true);
  expect(await authenticate(second.credential)).toEqual(expectedPrincipal);
  expect(await authenticate(token.credential)).toEqual(expectedPrincipal);
});

test("missing declared or private credential records fail closed on direct and fresh requests", async () => {
  for (const table of ["service_credential", "__jadpo_auth_service_credentials"]) {
    const key = await issue();
    const token = await exchange(key.credential);
    await db.all(`DELETE FROM "${table}" WHERE id = ?`, key.credentialId);
    await denied(key.credential);
    await denied(token.credential, "invalid_credentials", true);
    await expect(exchange(key.credential)).rejects.toMatchObject({ code: "invalid_credentials" });
  }
});

test("generic host revocation updates the declared record for direct and exchanged service credentials", async () => {
  for (const form of ["direct", "signed"]) {
    const key = await issue();
    const token = await exchange(key.credential);
    await host().revoke("api_bearer", form === "direct" ? key.credential : token.credential, now + 1);
    expect(millis((await declared(key.credentialId)).revoked_at)).toBe(now + 1);
    await denied(key.credential);
    await denied(token.credential, "invalid_credentials", true);
    expect(await authenticate(token.credential)).toEqual(expectedPrincipal);
  }
});

test("declared identity and verifier cannot disagree with the original private metadata", async () => {
  for (const [field, value, code] of [["service_id", otherService, "authentication_unavailable"], ["verifier", "A".repeat(43), "invalid_credentials"]]) {
    const key = await issue();
    const token = await exchange(key.credential);
    await db.all(`UPDATE service_credential SET ${field} = ? WHERE id = ?`, value, key.credentialId);
    await denied(key.credential, code);
    await denied(token.credential, code, true);
    await expect(exchange(key.credential)).rejects.toMatchObject({ code });
  }
});

test("malformed private binding metadata cannot fall back to legacy credentials or transfer service identity", async () => {
  for (const mutate of [(row: any) => { delete row.binding; }, (row: any) => { row.serviceId = otherService; }]) {
    const key = await issue();
    const token = await exchange(key.credential);
    const row = await metadata(key.credentialId);
    mutate(row);
    await db.all('UPDATE "__jadpo_auth_service_credentials" SET data = ? WHERE id = ?', JSON.stringify(row), key.credentialId);
    await denied(key.credential, "authentication_unavailable");
    await denied(token.credential, "authentication_unavailable", true);
    await expect(exchange(key.credential)).rejects.toMatchObject({ code: "authentication_unavailable" });
  }
});

test("missing or substituted service identity cannot acquire an existing service credential", async () => {
  const key = await issue();
  const token = await exchange(key.credential);
  await db.all("UPDATE service SET name = 'retired' WHERE id = ?", service);
  await db.all("UPDATE service SET name = 'todo-service' WHERE id = ?", otherService);
  await denied(key.credential, "authentication_unavailable");
  await denied(token.credential, "authentication_unavailable", true);
  await expect(exchange(key.credential)).rejects.toMatchObject({ code: "authority_invariant" });
  expect(await authenticate(token.credential)).toEqual(expectedPrincipal);
});

test("failure inserting either credential record rolls back both records and returns no credential", async () => {
  for (const table of ["service_credential", "__jadpo_auth_service_credentials"]) {
    if (postgres) {
      await db.exec(`CREATE FUNCTION reject_test_credential() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test insertion failure'; END $$; CREATE TRIGGER reject_test_credential BEFORE INSERT ON "${table}" FOR EACH ROW EXECUTE FUNCTION reject_test_credential();`);
    } else await db.exec(`CREATE TRIGGER reject_test_credential BEFORE INSERT ON "${table}" BEGIN SELECT RAISE(ABORT, 'test insertion failure'); END;`);
    try {
      await expect(issue()).rejects.toMatchObject({ code: "authentication_unavailable" });
      expect(await db.all("SELECT id FROM service_credential")).toHaveLength(0);
      expect(await db.all('SELECT id FROM "__jadpo_auth_service_credentials"')).toHaveLength(0);
    } finally {
      if (postgres) await db.exec(`DROP TRIGGER reject_test_credential ON "${table}"; DROP FUNCTION reject_test_credential();`);
      else await db.exec("DROP TRIGGER reject_test_credential;");
    }
  }
  expect(await authenticate((await issue()).credential)).toEqual(expectedPrincipal);
});

test("service principal stays separate from user ownership on real protected mutation routes", async () => {
  const key = await issue();
  const token = await exchange(key.credential);
  for (const credential of [key.credential, token.credential]) {
    const response = await app.handleRequest(new Request("https://todo.test/todos", { method: "POST", headers: { authorization: `Bearer ${credential}`, "content-type": "application/json" }, body: JSON.stringify({ title: "Denied service todo" }) }));
    expect(response.status).toBe(403);
    const text = await response.text();
    expect(JSON.parse(text).error.code).toBe("not_permitted");
    expect(text).not.toContain(credential);
  }
  expect(await db.all("SELECT id FROM todo")).toHaveLength(0);
  await expect(issue("unknown-service")).rejects.toMatchObject({ code: "invalid_credentials" });
});
