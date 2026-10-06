import { copyGoldenFixture } from "./golden-fixture";
import { afterAll, beforeAll, beforeEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { existsSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// RM-102/103 regression slice against unchanged migrated source. This is not
// execution of the frozen 44-case acceptance suite or the service lifecycle gate.
const root = mkdtempSync(join(tmpdir(), "jadpo-golden-protected-"));
const project = join(root, "application");
const previousSqlitePath = Bun.env.SQLITE_PATH;
const previousDatabaseUrl = Bun.env.DATABASE_URL;
let database: Database | undefined;
let server: import("node:http").Server | undefined;
let baseUrl: URL;
afterAll(async () => {
  if (server !== undefined) await new Promise<void>(resolveClose => server!.close(() => resolveClose()));
  database?.close();
  if (previousSqlitePath === undefined) delete Bun.env.SQLITE_PATH;
  else Bun.env.SQLITE_PATH = previousSqlitePath;
  if (previousDatabaseUrl === undefined) delete Bun.env.DATABASE_URL;
  else Bun.env.DATABASE_URL = previousDatabaseUrl;
  rmSync(root, { recursive: true, force: true });
});
copyGoldenFixture(project);
writeFileSync(join(project, "routes/adapter-probe.jadpo"), `
route GET /adapter-probe {
    auth: none
    headers: { trace_id: Text from "X-Trace" optional }
    output: Text
    action: { return "ready" }
}
`);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Golden protected fixture failed to build:\n${build.stdout}\n${build.stderr}`);
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
if (!existsSync(join(dependency, "node_modules/jose/package.json"))) throw new Error("Install the pinned JWT dependency before running this suite");
symlinkSync(join(dependency, "node_modules"), join(project, "build/target/node_modules"), "dir");
Bun.env.SQLITE_PATH = join(root, "golden.sqlite");
delete Bun.env.DATABASE_URL;
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const authentication = await import(pathToFileURL(join(project, "build/target/authentication.ts")).href);
beforeAll(async () => {
  server = app.createApplicationServer();
  await new Promise<void>((resolveListen, reject) => {
    server!.once("error", reject);
    server!.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("protected-route server has no port");
  baseUrl = new URL(`http://127.0.0.1:${address.port}`);
});
database = new Database(Bun.env.SQLITE_PATH, { strict: true });
const db = database;
const owner = "00000000-0000-4000-8000-000000000001";
const otherOwner = "00000000-0000-4000-8000-000000000002";
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
beforeEach(async () => {
  await app.initializeApplication(environment);
  db.exec('DELETE FROM todo; DELETE FROM "__jadpo_auth_sessions"; DELETE FROM "user";');
  now = Date.now();
  for (const [id, subject] of [[owner, "owner"], [otherOwner, "other-owner"]]) {
    db.prepare('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)')
      .run(id, subject, `${subject}@example.test`, "active", now, null);
  }
});
const host = () => app.authenticationHost();
const issue = (strategy = "api_bearer", subject = "owner") => host().issue(strategy, subject, now + 3_600_000, now);
const bearer = (credential: string) => ({ authorization: `Bearer ${credential}` });
const countTodos = () => (db.prepare("SELECT count(*) AS count FROM todo").get() as { count: number }).count;
function rawRequest(headers: Record<string, string>, body: unknown = { title: "Protected todo" }) {
  return new Request("https://todo.test/todos", {
    method: "POST", headers: { "content-type": "application/json", ...headers }, body: JSON.stringify(body),
  });
}
async function rawListenerRequest(headers: Array<[string, string]>, body: unknown) {
  const encoded = JSON.stringify(body);
  const port = Number(baseUrl.port);
  const response = await new Promise<string>((resolveResponse, reject) => {
    let received = "";
    const socket = connect(port, "127.0.0.1", () => {
      socket.write([
        "POST /todos HTTP/1.1",
        `Host: 127.0.0.1:${port}`,
        "Content-Type: application/json",
        `Content-Length: ${Buffer.byteLength(encoded)}`,
        ...headers.map(([name, value]) => `${name}: ${value}`),
        "Connection: close",
        "",
        encoded,
      ].join("\r\n"));
    });
    socket.on("data", chunk => { received += chunk.toString(); });
    socket.on("end", () => resolveResponse(received));
    socket.on("error", reject);
  });
  const [head, responseBody] = response.split("\r\n\r\n", 2);
  return { status: Number(head.split(" ")[1]), body: JSON.parse(responseBody) };
}
async function rejected(headers: Record<string, string>, status: number, code: string) {
  const before = countTodos();
  const response = await app.handleRequest(rawRequest(headers));
  expect(response.status).toBe(status);
  const text = await response.text();
  expect(JSON.parse(text).error.code).toBe(code);
  expect(countTodos()).toBe(before);
  expect(text).not.toContain(environment.SESSION_SIGNING_KEY);
  for (const value of Object.values(headers)) {
    if (value.startsWith("Bearer ")) expect(text).not.toContain(value.slice(7));
  }
}

test("signed user and browser credentials preserve principal identity and the declared strength enum", async () => {
  for (const strategy of ["api_bearer", "browser_session"]) {
    const credential = await issue(strategy);
    const headers = strategy === "api_bearer" ? bearer(credential.credential) : { cookie: credential.setCookie.split(";", 1)[0] };
    for (const fresh of [false, true]) {
      const principal = await host().authenticate(new Request("https://todo.test/todos", { headers }), fresh, now);
      expect(principal).toEqual({ kind: "user", subject: "owner", authenticationStrength: "primary", values: { user_id: owner } });
      expect(JSON.stringify(principal)).not.toContain(credential.credential);
    }
    const refreshed = await host().refresh(strategy, credential.credential, now + 1);
    const refreshedHeaders = strategy === "api_bearer" ? bearer(refreshed.credential) : { cookie: refreshed.setCookie.split(";", 1)[0] };
    expect(await host().authenticate(new Request("https://todo.test/todos", { headers: refreshedHeaders }), true, now + 1))
      .toEqual({ kind: "user", subject: "owner", authenticationStrength: "primary", values: { user_id: owner } });
  }
});

test("disabled golden users fail fresh authority and refresh immediately while signed access stays within five minutes", async () => {
  const credential = await issue("api_bearer");
  expect(credential.expires).toBeGreaterThan(now);
  expect(credential.expires).toBeLessThanOrEqual(now + 300_000);

  db.prepare('UPDATE "user" SET status = ?, disabled_at = ? WHERE id = ?')
    .run("disabled", now, owner);
  const request = new Request("https://todo.test/todos", { headers: bearer(credential.credential) });

  // Signed credentials retain the declared bounded-revocation window.
  expect(await host().authenticate(request, false, now)).toMatchObject({ kind: "user", values: { user_id: owner } });
  await expect(host().authenticate(request, true, now)).rejects.toMatchObject({
    code: "principal_inactive",
    declaredFailure: "UserDisabled",
  });
  await expect(host().refresh("api_bearer", credential.credential, now + 1)).rejects.toMatchObject({
    code: "principal_inactive",
    declaredFailure: "UserDisabled",
  });
  await expect(host().authenticate(request, false, credential.expires)).rejects.toMatchObject({ code: "invalid_credentials" });
});

test("the generated adapter boundary validates the authored strength enum without accepting widened principals", async () => {
  const principal = { kind: "user", subject: "owner", authenticationStrength: "primary", values: { user_id: owner } };
  const normalize = (candidate: unknown) => authentication.authenticateRequest(
    new Request("https://todo.test/todos", { headers: bearer("adapter-test") }),
    { freshAuthority: false },
    {
      async validate() { return { kind: "valid", identity: { principal: candidate, authorityRequired: false } }; },
      async resolve() { throw new Error("Lookup-free test identity must not resolve"); },
    },
  );
  for (const strength of ["primary", "multi_factor"]) {
    await expect(normalize({ ...principal, authenticationStrength: strength }))
      .resolves.toEqual({ ...principal, authenticationStrength: strength });
  }
  for (const strength of ["signed", "api_key", "jwt", "unknown", "", null, 1, undefined]) {
    await expect(normalize({ ...principal, authenticationStrength: strength }))
      .rejects.toMatchObject({ code: "authority_invariant" });
  }
  const { authenticationStrength: _omitted, ...missingStrength } = principal;
  await expect(normalize(missingStrength)).rejects.toMatchObject({ code: "authority_invariant" });
  await expect(normalize({ ...principal, proofMode: "multi_factor" })).rejects.toMatchObject({ code: "authority_invariant" });
});

test("protected create persists the authenticated owner and returns the matching generated row", async () => {
  for (const [subject, ownerId] of [["owner", owner], ["other-owner", otherOwner]]) {
    const credential = await issue("api_bearer", subject);
    const response = await app.handleRequest(rawRequest(bearer(credential.credential), { title: `Todo for ${subject}` }));
    expect(response.status).toBe(201);
    const body = await response.json();
    expect(body.id).toMatch(/^[a-f0-9-]{36}$/u);
    expect(body).toMatchObject({ title: `Todo for ${subject}`, status: "open", due_at: null });
    expect(body).not.toHaveProperty("owner_id");
    const stored = db.prepare("SELECT id, owner_id, title, status, due_at FROM todo WHERE id = ?").get(body.id);
    expect(stored).toEqual({ id: body.id, owner_id: ownerId, title: body.title, status: "open", due_at: null });
  }
  expect(countTodos()).toBe(2);
});

test("missing, malformed, tampered and competing credentials fail before todo creation", async () => {
  const credential = await issue();
  const browser = await issue("browser_session");
  await rejected({}, 401, "authentication_required");
  await rejected({ authorization: "Basic unsupported" }, 401, "invalid_credentials");
  await rejected(bearer(`${credential.credential}x`), 401, "invalid_credentials");
  await rejected({ ...bearer(credential.credential), cookie: browser.setCookie.split(";", 1)[0] }, 401, "ambiguous_credentials");
  await rejected({ ...bearer(credential.credential), cookie: "todo_session=invalid" }, 401, "ambiguous_credentials");
  await rejected(bearer(`${credential.credential}, Bearer ${credential.credential}`), 401, "ambiguous_credentials");
});

test("browser mutations require both the configured origin and session-bound CSRF proof", async () => {
  const credential = await issue("browser_session");
  const cookie = credential.setCookie.split(";", 1)[0];
  const valid = { cookie, origin: environment.BROWSER_ORIGIN, "x-jadpo-csrf": credential.csrfToken };
  await rejected({ cookie, "x-jadpo-csrf": credential.csrfToken }, 403, "csrf_rejected");
  await rejected({ cookie, origin: environment.BROWSER_ORIGIN }, 403, "csrf_rejected");
  await rejected({ ...valid, origin: "https://evil.test" }, 403, "csrf_rejected");
  await rejected({ ...valid, "x-jadpo-csrf": "invalid" }, 403, "csrf_rejected");
  const anotherSession = await issue("browser_session");
  await rejected({ ...valid, "x-jadpo-csrf": anotherSession.csrfToken }, 403, "csrf_rejected");
  const response = await app.handleRequest(rawRequest(valid));
  expect(response.status).toBe(201);
  const body = await response.json();
  expect(db.prepare("SELECT owner_id FROM todo WHERE id = ?").get(body.id)).toEqual({ owner_id: owner });
});

test("node:http adapter preserves protected POST authentication and CSRF boundaries", async () => {
  const missing = await fetch(new URL("/todos", baseUrl), {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ title: "Missing auth" }),
  });
  expect(missing.status).toBe(401);
  expect((await missing.json()).error.code).toBe("authentication_required");

  const api = await issue();
  const accepted = await fetch(new URL("/todos", baseUrl), {
    method: "POST",
    headers: { "content-type": "application/json", ...bearer(api.credential) },
    body: JSON.stringify({ title: "Adapter bearer" }),
  });
  expect(accepted.status).toBe(201);
  expect(await accepted.json()).toMatchObject({ title: "Adapter bearer", status: "open" });

  const browser = await issue("browser_session");
  const cookie = browser.setCookie.split(";", 1)[0];
  const wrongOrigin = await fetch(new URL("/todos", baseUrl), {
    method: "POST",
    headers: { "content-type": "application/json", cookie, origin: "https://evil.test", "x-jadpo-csrf": browser.csrfToken },
    body: JSON.stringify({ title: "Wrong origin" }),
  });
  expect(wrongOrigin.status).toBe(403);
  expect((await wrongOrigin.json()).error.code).toBe("csrf_rejected");

  const validBrowser = await fetch(new URL("/todos", baseUrl), {
    method: "POST",
    headers: { "content-type": "application/json", cookie, origin: environment.BROWSER_ORIGIN, "x-jadpo-csrf": browser.csrfToken },
    body: JSON.stringify({ title: "Adapter browser" }),
  });
  expect(validBrowser.status).toBe(201);
  expect(await validBrowser.json()).toMatchObject({ title: "Adapter browser", status: "open" });

  const duplicate = await rawListenerRequest([
    ["Authorization", `Bearer ${api.credential}`],
    ["authorization", `Bearer ${api.credential}`],
  ], { title: "Duplicate credential" });
  expect(duplicate.status).toBe(401);
  expect(duplicate.body.error.code).toBe("ambiguous_credentials");
  expect(countTodos()).toBe(2);
});

test("missing or unsafe origin configuration leaves protected routes unavailable without writes", async () => {
  const credential = await issue();
  for (const origin of [undefined, "http://todo.test", "https://todo.test/path"]) {
    await expect(app.initializeApplication({ ...environment, BROWSER_ORIGIN: origin })).rejects.toThrow();
    expect(() => app.authenticationHost()).toThrow();
    await rejected(bearer(credential.credential), 503, "authentication_misconfigured");
  }
});
