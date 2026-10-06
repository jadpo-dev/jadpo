import { afterAll, beforeEach, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = mkdtempSync(join(tmpdir(), "jadpo-first-party-auth-"));
const emptyEnvFile = `--env-file=${process.platform === "win32" ? "NUL" : "/dev/null"}`;
const path = join(root, "auth.sqlite");
// Never inherit an unrelated application DATABASE_URL into a destructive suite.
const postgresUrl = Bun.env.JADPO_AUTH_TEST_DATABASE_URL;
if (postgresUrl) {
  const url = new URL(postgresUrl);
  if (!["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) {
    throw new Error("Authentication tests require a disposable local jadpo_auth_test_* database");
  }
  Bun.env.DATABASE_URL = postgresUrl;
} else { delete Bun.env.DATABASE_URL; }
Bun.env.SQLITE_PATH = path;
const app = await import("../../examples/first-party-authentication/build/target/app.ts");
const { AuthenticationFault, firstPartyAuthenticationStrength } = await import("../../examples/first-party-authentication/build/target/authentication.ts");
function testDatabase(sqlitePath: string, url?: string) {
  const sqlite = url ? null : new Database(sqlitePath, { strict: true });
  const postgres = url ? new SQL({ url, prepare: false }) : null;
  const query = (text: string) => { let index = 0; return text.replace(/\?/gu, () => `$${++index}`); };
  return {
    async exec(text: string) { if (postgres) await postgres.unsafe(text); else sqlite!.exec(text); },
    prepare(text: string) { return {
      async run(...values: any[]) { return postgres ? await postgres.unsafe(query(text), values) : sqlite!.prepare(text).run(...values); },
      async all(...values: any[]): Promise<any[]> { return postgres ? [...await postgres.unsafe(query(text), values)] : sqlite!.prepare(text).all(...values); },
      async get(...values: any[]): Promise<any> { return (await this.all(...values))[0] ?? null; },
    }; },
    async close() { if (postgres) await postgres.close(); else sqlite!.close(); },
  };
}
const db = testDatabase(path, postgresUrl);
const alice = "00000000-0000-4000-8000-000000000001";
const bob = "00000000-0000-4000-8000-000000000002";
const note = "00000000-0000-4000-8000-000000000003";
const key = Buffer.alloc(32, 1).toString("base64url");
const previousKey = Buffer.alloc(32, 2).toString("base64url");
const env = { AUTH_SIGNING_KEY: key, AUTH_PREVIOUS_SIGNING_KEY: previousKey, BROWSER_ORIGIN: "https://example.test" };
let now = Date.now();

beforeEach(async () => {
  await db.exec('DELETE FROM "__jadpo_auth_sessions"; DELETE FROM "note"; DELETE FROM "user"');
  const insert = db.prepare('INSERT INTO "user" ("id", "authentication_subject", "enabled") VALUES (?, ?, TRUE)');
  await insert.run(alice, "alice"); await insert.run(bob, "bob");
  await db.prepare('INSERT INTO "note" ("id", "owner_id", "title") VALUES (?, ?, ?)').run(note, alice, "Original");
  await app.initializeApplication(env);
  now = Date.now();
});
afterAll(async () => { await db.close(); rmSync(root, { recursive: true, force: true }); });
const issue = (strategy = "browser_session", subject = "alice") => app.authenticationHost().issue(strategy, subject, now + 3_600_000, now);
const cookie = (credential: string) => ({ cookie: `__Host-jadpo_session=${credential}` });
const bearer = (credential: string) => ({ authorization: `Bearer ${credential}` });
test("built-in strength mapping preserves legacy text and rejects unconfigured principal kinds", () => {
  expect(firstPartyAuthenticationStrength("user", "signed")).toBe("signed");
  expect(firstPartyAuthenticationStrength("user", "opaque")).toBe("opaque");
  expect(() => firstPartyAuthenticationStrength("service", "api_key")).toThrow(AuthenticationFault);
  expect(() => firstPartyAuthenticationStrength("user", "multi_factor")).toThrow(AuthenticationFault);
});
function request(route = "/identity", headers: Record<string, string> = {}, body?: unknown) {
  return new Request(`https://example.test${route}`, { method: body === undefined ? "GET" : "POST", headers, ...(body === undefined ? {} : { body: typeof body === "string" ? body : JSON.stringify(body) }) });
}
async function fault(operation: Promise<unknown>, code: string) {
  try { await operation; throw new Error("Expected rejection"); }
  catch (error) { expect(error).toBeInstanceOf(AuthenticationFault); expect((error as InstanceType<typeof AuthenticationFault>).code).toBe(code); }
}

describe("generated first-party authentication", () => {
  test("signed cookie and opaque bearer reach a protected route with the typed principal", async () => {
    const browser = await issue(); const api = await issue("api_bearer");
    for (const headers of [cookie(browser.credential), bearer(api.credential)]) {
      const response = await app.handleRequest(request("/identity", headers));
      expect(response.status).toBe(200); expect(await response.json()).toEqual({ id: alice });
    }
    expect(browser.setCookie).toContain("; Path=/; Secure; HttpOnly; SameSite=Strict;");
    expect(api.setCookie).toBeNull();
  });
  test("protected routes narrow the current principal to the declared variant", async () => {
    const credential = bearer((await issue("api_bearer")).credential);
    const userResponse = await app.handleRequest(request("/user-identity", credential));
    expect(userResponse.status).toBe(200);
    expect(await userResponse.json()).toEqual({ id: alice });

    const wrongVariant = await app.handleRequest(request("/service-identity", credential));
    expect(wrongVariant.status).toBe(403);
    expect((await wrongVariant.json()).error.code).toBe("not_permitted");
  });
  test("authentication precedes input decoding and public health ignores credentials", async () => {
    const response = await app.handleRequest(request("/identity", {}, "{broken"));
    expect(response.status).toBe(401); expect((await response.json()).error.code).toBe("authentication_required");
    expect((await app.handleRequest(request("/health/live", { cookie: "__Host-jadpo_session=invalid", authorization: "garbage" }))).status).toBe(200);
  });
  test("duplicates, conflicting slots and invalid beside valid all reject", async () => {
    const valid = await issue();
    for (const headers of [
      { cookie: `__Host-jadpo_session=${valid.credential}; __Host-jadpo_session=invalid` },
      { ...cookie(valid.credential), authorization: "Bearer invalid" },
      { authorization: "Bearer first, Bearer second" },
    ]) {
      const response = await app.handleRequest(request("/identity", headers));
      expect(response.status).toBe(401); expect((await response.json()).error.code).toBe("ambiguous_credentials");
    }
  });
  test("cookie mutations require an exact configured origin and a session-bound CSRF token", async () => {
    const valid = await issue(); const other = await issue();
    const headers = { ...cookie(valid.credential), origin: env.BROWSER_ORIGIN, "x-jadpo-csrf": valid.csrfToken };
    for (const bad of [
      cookie(valid.credential), { ...cookie(valid.credential), origin: env.BROWSER_ORIGIN },
      { ...cookie(valid.credential), "x-jadpo-csrf": valid.csrfToken },
      { ...headers, "x-jadpo-csrf": "wrong" },
      { ...headers, origin: "https://example.test.attacker.test" },
      { ...headers, origin: "null" }, { ...headers, "x-jadpo-csrf": other.csrfToken },
      { ...headers, "sec-fetch-site": "cross-site" },
    ]) expect((await app.handleRequest(request("/identity", bad, { label: "ok" }))).status).toBe(403);
    expect((await app.handleRequest(request("/identity", headers, { label: "ok" }))).status).toBe(200);
    expect((await app.handleRequest(request("/identity", bearer((await issue("api_bearer")).credential), { label: "ok" }))).status).toBe(200);
  });
  test("authenticated principal reaches policy-scoped reads and mutations", async () => {
    const a = bearer((await issue("api_bearer")).credential); const b = bearer((await issue("api_bearer", "bob")).credential);
    expect((await app.handleRequest(request("/notes/read", a, { id: note }))).status).toBe(200);
    expect((await app.handleRequest(request("/notes/read", b, { id: note }))).status).toBe(404);
    expect((await app.handleRequest(request("/notes/rename", b, { id: note, title: "Stolen" }))).status).toBe(404);
    expect((await app.handleRequest(request("/notes/rename", a, { id: note, title: "Changed" }))).status).toBe(200);
    expect(await db.prepare('SELECT title FROM note WHERE id = ?').get(note)).toEqual({ title: "Changed" });
  });
  test("concurrent requests never exchange principals", async () => {
    const a = bearer((await issue("api_bearer")).credential); const b = bearer((await issue("api_bearer", "bob")).credential);
    const results = await Promise.all(Array.from({ length: 30 }, async (_, i) => (await app.handleRequest(request("/identity", i % 2 ? a : b))).json()));
    results.forEach((result, i) => expect(result.id).toBe(i % 2 ? alice : bob));
  });
  test("bounded ordinary requests avoid authority while fresh and opaque requests check it", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    await db.exec('UPDATE "user" SET enabled = FALSE');
    expect((await app.handleRequest(request("/identity", cookie(signed.credential)))).status).toBe(200);
    for (const [route, headers] of [["/fresh-identity", cookie(signed.credential)], ["/identity", bearer(opaque.credential)]] as const) {
      const response = await app.handleRequest(request(route, headers));
      expect(response.status).toBe(422); expect((await response.json()).error.code).toBe("user_disabled");
    }
    await fault(app.authenticationHost().refresh("browser_session", signed.credential, now + 1), "principal_inactive");
  });
  test("logout is immediate for opaque credentials and bounded for signed credentials", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    await app.authenticationHost().revoke("browser_session", signed.credential, now);
    await app.authenticationHost().revoke("api_bearer", opaque.credential, now);
    expect((await app.handleRequest(request("/identity", cookie(signed.credential)))).status).toBe(200);
    expect((await app.handleRequest(request("/fresh-identity", cookie(signed.credential)))).status).toBe(401);
    expect((await app.handleRequest(request("/identity", bearer(opaque.credential)))).status).toBe(401);
    await fault(app.authenticationHost().refresh("browser_session", signed.credential, now + 1), "invalid_credentials");
  });
  test("expiry is exclusive and future issued times reject", async () => {
    const signed = await issue();
    const req = request("/identity", cookie(signed.credential));
    expect((await app.authenticationHost().authenticate(req, false, now + 299_999)).values.user_id).toBe(alice);
    await fault(app.authenticationHost().authenticate(req, false, now + 300_000), "invalid_credentials");
    await fault(app.authenticationHost().authenticate(req, false, now - 1), "invalid_credentials");
    const opaque = await issue("api_bearer");
    await fault(app.authenticationHost().authenticate(request("/identity", bearer(opaque.credential)), false, now + 3_600_000), "invalid_credentials");
  });
  test("refresh checks authority and cannot extend the absolute session expiry", async () => {
    const signed = await app.authenticationHost().issue("browser_session", "alice", now + 350_000, now);
    const refreshed = await app.authenticationHost().refresh("browser_session", signed.credential, now + 200_000);
    expect(refreshed.expires).toBe(now + 350_000);
    await fault(app.authenticationHost().authenticate(request("/identity", cookie(refreshed.credential)), false, now + 350_000), "invalid_credentials");
  });
  test("key overlap works for signed and opaque credentials; retiring the old key rejects both", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    const next = Buffer.alloc(32, 3).toString("base64url");
    await app.initializeApplication({ ...env, AUTH_SIGNING_KEY: next, AUTH_PREVIOUS_SIGNING_KEY: key });
    expect((await app.handleRequest(request("/identity", cookie(signed.credential)))).status).toBe(200);
    expect((await app.handleRequest(request("/identity", bearer(opaque.credential)))).status).toBe(200);
    const refreshed = await app.authenticationHost().refresh("browser_session", signed.credential, now + 1);
    await app.initializeApplication({ ...env, AUTH_SIGNING_KEY: next, AUTH_PREVIOUS_SIGNING_KEY: previousKey });
    expect((await app.handleRequest(request("/identity", cookie(signed.credential)))).status).toBe(401);
    expect((await app.handleRequest(request("/identity", bearer(opaque.credential)))).status).toBe(401);
    expect((await app.handleRequest(request("/identity", cookie(refreshed.credential)))).status).toBe(200);
  });
  test("malformed configuration fails before authentication becomes available", async () => {
    for (const update of [{ BROWSER_ORIGIN: undefined }, { AUTH_SIGNING_KEY: "short" }, { BROWSER_ORIGIN: "http://example.test" }, { BROWSER_ORIGIN: "https://example.test/path" }, { AUTH_PREVIOUS_SIGNING_KEY: key }]) {
      await expect(app.initializeApplication({ ...env, ...update })).rejects.toThrow();
      expect((await app.handleRequest(request())).status).toBe(503);
    }
  });
  test("a failed newer initialization cannot be overwritten by an older success", async () => {
    const results = await Promise.allSettled([
      app.initializeApplication(env),
      app.initializeApplication({ ...env, AUTH_SIGNING_KEY: "invalid" }),
    ]);
    expect(results.map(result => result.status)).toEqual(["rejected", "rejected"]);
    expect((await app.handleRequest(request())).status).toBe(503);
    await app.initializeApplication(env);
    const credential = await issue("api_bearer");
    expect((await app.handleRequest(request("/identity", bearer(credential.credential)))).status).toBe(200);
  });
  test("reusing a subject for another identity cannot inherit an existing credential", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    await db.exec('DELETE FROM "note"; DELETE FROM "user"');
    await db.prepare('INSERT INTO "user" ("id", "authentication_subject", "enabled") VALUES (?, ?, TRUE)').run(bob, "alice");
    // Ordinary signed requests retain the old identity only for their declared bound.
    expect(await (await app.handleRequest(request("/identity", cookie(signed.credential)))).json()).toEqual({ id: alice });
    for (const [route, headers] of [["/fresh-identity", cookie(signed.credential)], ["/identity", bearer(opaque.credential)]] as const) {
      const response = await app.handleRequest(request(route, headers));
      expect(response.status).not.toBe(200);
      expect(await response.text()).not.toContain(bob);
    }
    await expect(app.authenticationHost().refresh("browser_session", signed.credential, now)).rejects.toThrow();
  });
  test("persisted credentials and revocation survive a separate runtime process", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    const childFile = join(root, "restart.ts");
    const moduleUrl = new URL("../../examples/first-party-authentication/build/target/app.ts", import.meta.url).href;
    await Bun.write(childFile, `
      const app = await import(${JSON.stringify(moduleUrl)});
      await app.initializeApplication(Bun.env);
      const credentials = await Bun.stdin.json();
      const statuses = [];
      for (const [name, credential, headers] of credentials) {
        statuses.push((await app.handleRequest(new Request("https://example.test/fresh-identity", { headers }))).status);
        await app.authenticationHost().revoke(name, credential, Date.now());
      }
      console.log(JSON.stringify(statuses));
    `);
    const child = Bun.spawn([process.execPath, "--no-install", emptyEnvFile, childFile], {
      env: { ...env, SQLITE_PATH: path, ...(postgresUrl ? { DATABASE_URL: postgresUrl } : {}) },
      stdin: new Blob([JSON.stringify([
        ["browser_session", signed.credential, cookie(signed.credential)],
        ["api_bearer", opaque.credential, bearer(opaque.credential)],
      ])]), stdout: "pipe", stderr: "pipe",
    });
    const output = await new Response(child.stdout).text();
    expect(await child.exited).toBe(0);
    expect(JSON.parse(output)).toEqual([200, 200]);
    expect((await app.handleRequest(request("/fresh-identity", cookie(signed.credential)))).status).toBe(401);
    expect((await app.handleRequest(request("/identity", bearer(opaque.credential)))).status).toBe(401);
  });
  test("tampering, unknown key versions, wrong transports and bounded parser mutations reject", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    const mutations = ["", "garbage", "x".repeat(4097), signed.credential + "=", signed.credential.replace("jds1.", "jds2."), signed.credential.replace(/\.[^.]+\./u, ".unknown."), opaque.credential];
    for (let i = 0; i < signed.credential.length; i += 3) mutations.push(signed.credential.slice(0, i) + (signed.credential[i] === "A" ? "B" : "A") + signed.credential.slice(i + 1));
    for (const credential of mutations) expect((await app.handleRequest(request("/identity", cookie(credential)))).status).toBe(401);
    expect((await app.handleRequest(request("/identity", bearer(signed.credential)))).status).toBe(401);
  });
  test("credentials and keys are absent from stored session records and public errors", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    const records = JSON.stringify(await db.prepare('SELECT * FROM "__jadpo_auth_sessions"').all());
    const response = await app.handleRequest(request("/identity", cookie(signed.credential + "invalid")));
    const error = await response.text();
    for (const secret of [key, previousKey, signed.credential, opaque.credential]) { expect(records).not.toContain(secret); expect(error).not.toContain(secret); }
  });
  test("authority outage returns a safe operational failure; bounded reads keep working", async () => {
    const signed = await issue(); const opaque = await issue("api_bearer");
    await db.exec('ALTER TABLE "user" RENAME TO "unavailable_user"');
    try {
      expect((await app.handleRequest(request("/identity", cookie(signed.credential)))).status).toBe(200);
      const response = await app.handleRequest(request("/identity", bearer(opaque.credential)));
      expect(response.status).toBe(503); expect(await response.text()).not.toContain("unavailable_user");
    } finally { await db.exec('ALTER TABLE "unavailable_user" RENAME TO "user"'); }
  });
  test("generated handler serves an authenticated request over real HTTP", async () => {
    const credential = await issue("api_bearer");
    const server = Bun.serve({ port: 0, fetch: app.handleRequest });
    try { const response = await fetch(`http://127.0.0.1:${server.port}/identity`, { headers: bearer(credential.credential) }); expect(response.status).toBe(200); expect(await response.json()).toEqual({ id: alice }); }
    finally { server.stop(true); }
  });
});

async function generatedVariant(name: string, transform: (source: string) => string) {
  const source = await Bun.file(new URL("../../examples/first-party-authentication/app.jadpo", import.meta.url)).text();
  const project = join(root, name);
  await Bun.write(join(project, "app.jadpo"), transform(source));
  const compiler = new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
  const built = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
  expect(built.exitCode).toBe(0);
  const dbPath = join(project, "test.sqlite");
  const previousPath = Bun.env.SQLITE_PATH;
  const previousUrl = Bun.env.DATABASE_URL;
  let variantUrl: string | undefined;
  if (postgresUrl) {
    const url = new URL(postgresUrl);
    const databaseName = `jadpo_auth_test_${crypto.randomUUID().replaceAll("-", "")}`;
    const admin = new SQL({ url: postgresUrl, prepare: false });
    try { await admin.unsafe(`CREATE DATABASE "${databaseName}"`); } finally { await admin.close(); }
    url.pathname = `/${databaseName}`;
    variantUrl = url.href;
    Bun.env.DATABASE_URL = variantUrl;
  } else { delete Bun.env.DATABASE_URL; }
  Bun.env.SQLITE_PATH = dbPath;
  let variant: typeof app;
  try { variant = await import(join(project, "build/target/app.ts")); } finally { Bun.env.SQLITE_PATH = previousPath; if (previousUrl) Bun.env.DATABASE_URL = previousUrl; }
  const database = testDatabase(dbPath, variantUrl);
  await database.prepare('INSERT INTO "user" ("id", "authentication_subject", "enabled") VALUES (?, ?, TRUE)').run(alice, "alice");
  await variant.initializeApplication(env);
  return { variant, database };
}

test("generated immediate mode uses opaque cookies and checks authority on every request", async () => {
  const { variant, database } = await generatedVariant("immediate", source => source.replace("mode: bounded\n            maximum_delay: 5m", "mode: immediate").replace("mode: signed", "mode: opaque").replace("route POST /notes/rename", "route GET /notes/rename"));
  try {
    const credential = await variant.authenticationHost().issue("browser_session", "alice", now + 60_000, now);
    expect((await variant.handleRequest(request("/identity", cookie(credential.credential)))).status).toBe(200);
    const headers = { ...cookie(credential.credential), origin: env.BROWSER_ORIGIN, "x-jadpo-csrf": credential.csrfToken };
    expect((await variant.handleRequest(request("/identity", headers, { label: "ok" }))).status).toBe(200);
    expect((await variant.handleRequest(request("/notes/rename", cookie(credential.credential)))).status).toBe(403);
    await database.exec('UPDATE "user" SET enabled = FALSE');
    expect((await variant.handleRequest(request("/identity", cookie(credential.credential)))).status).toBe(422);
  } finally { await database.close(); }
});

test("generated signed bearer works and rejects correctly signed wrong-audience envelopes", async () => {
  const { variant, database } = await generatedVariant("signed-bearer", source => source.replace("mode: opaque", "mode: signed"));
  try {
    const credential = await variant.authenticationHost().issue("api_bearer", "alice", now + 60_000, now);
    expect((await variant.handleRequest(request("/identity", bearer(credential.credential)))).status).toBe(200);
    expect((await variant.handleRequest(request("/identity", bearer(credential.credential), { label: "ok" }))).status).toBe(200);
    expect((await variant.handleRequest(request("/identity", cookie(credential.credential)))).status).toBe(401);
    const parts = credential.credential.split(".");
    const payload = JSON.parse(Buffer.from(parts[2], "base64url").toString());
    payload[1] = "wrong-audience";
    parts[2] = Buffer.from(JSON.stringify(payload)).toString("base64url");
    const cryptoKey = await crypto.subtle.importKey("raw", Buffer.from(key, "base64url"), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    parts[3] = Buffer.from(await crypto.subtle.sign("HMAC", cryptoKey, new TextEncoder().encode(parts.slice(0, 3).join(".")))).toString("base64url");
    expect((await variant.handleRequest(request("/identity", bearer(parts.join("."))))).status).toBe(401);
  } finally { await database.close(); }
});

test("OpenAPI preserves both methods and agrees with the authentication audit", async () => {
  const openapi = await Bun.file(new URL("../../examples/first-party-authentication/build/openapi/openapi.json", import.meta.url)).json();
  const audit = await Bun.file(new URL("../../examples/first-party-authentication/build/audit/authentication.json", import.meta.url)).json();
  for (const method of ["get", "post"]) expect(openapi.paths["/identity"][method].security).toEqual([{ browser_session: [] }, { api_bearer: [] }]);
  expect(openapi.paths["/fresh-identity"].get["x-jadpo-fresh-authority"]).toBe(true);
  expect(openapi.components.securitySchemes.browser_session.in).toBe("cookie");
  expect(audit.runtime).toBe("first_party");
  expect(audit.revocation).toEqual({ mode: "bounded", maximum_delay: "5m" });
  expect(JSON.stringify(audit)).not.toContain(key);
});

test("invalid startup secrets cannot open a listener or escape through startup logs", async () => {
  const canary = "test-only-secret-canary-not-a-valid-key";
  const result = Bun.spawnSync([process.execPath, "--no-install", emptyEnvFile, new URL("../../examples/first-party-authentication/build/target/app.ts", import.meta.url).pathname], {
    env: { ...env, AUTH_SIGNING_KEY: canary, SQLITE_PATH: join(root, "startup.sqlite"), PORT: "0", PATH: Bun.env.PATH ?? "" },
    stdout: "pipe", stderr: "pipe",
  });
  expect(result.exitCode).toBe(1);
  const output = result.stdout.toString() + result.stderr.toString();
  expect(output).toContain("RUNTIME_STARTUP_FAILED");
  expect(output).not.toContain("runtime.ready");
  expect(output).not.toContain(canary);
});

test("an incompatible internal session table fails startup with a safe diagnostic", async () => {
  const brokenPath = join(root, "broken-auth-schema.sqlite");
  const broken = new Database(brokenPath);
  broken.exec('CREATE TABLE "__jadpo_auth_sessions" ("unexpected" TEXT)');
  broken.close();
  const result = Bun.spawnSync([process.execPath, "--no-install", emptyEnvFile, new URL("../../examples/first-party-authentication/build/target/app.ts", import.meta.url).pathname], {
    env: { ...env, SQLITE_PATH: brokenPath, PORT: "0" }, stdout: "pipe", stderr: "pipe",
  });
  expect(result.exitCode).toBe(1);
  const output = result.stdout.toString() + result.stderr.toString();
  expect(output).toContain("RUNTIME_STARTUP_FAILED");
  expect(output).not.toContain("runtime.ready");
  expect(output).not.toContain(brokenPath);
  expect(output).not.toContain("no such column");
});
