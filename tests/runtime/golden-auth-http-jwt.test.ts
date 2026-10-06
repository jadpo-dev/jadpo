import { copyGoldenFixture } from "./golden-fixture";
import { afterAll, beforeEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Unchanged migration, real localhost HTTP and real pinned JOSE signatures.
// Only the external issuer discovery/JWKS documents are supplied by the test.
const root = mkdtempSync(join(tmpdir(), "jadpo-golden-http-jwt-"));
const project = join(root, "application");
const previousSqlitePath = Bun.env.SQLITE_PATH;
const previousDatabaseUrl = Bun.env.DATABASE_URL;
const originalFetch = globalThis.fetch;
let database: Database | undefined;
let server: { stop(close: boolean): void; url: URL } | undefined;
afterAll(() => {
  server?.stop(true);
  database?.close();
  globalThis.fetch = originalFetch;
  if (previousSqlitePath === undefined) delete Bun.env.SQLITE_PATH;
  else Bun.env.SQLITE_PATH = previousSqlitePath;
  if (previousDatabaseUrl === undefined) delete Bun.env.DATABASE_URL;
  else Bun.env.DATABASE_URL = previousDatabaseUrl;
  rmSync(root, { recursive: true, force: true });
});
copyGoldenFixture(project);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Golden HTTP/JWT fixture failed to build:\n${build.stdout}\n${build.stderr}`);
const target = join(project, "build/target");
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
const installedPath = join(dependency, "node_modules/jose/package.json");
if (!existsSync(installedPath)) throw new Error("Install the pinned JWT dependency before running this suite");
const canonical = resolve("jadpo/crates/core/src/runtime/jwt");
const pin = JSON.parse(readFileSync(join(canonical, "dependency.json"), "utf8"));
const installed = JSON.parse(readFileSync(installedPath, "utf8"));
if (installed.version !== pin.version) throw new Error("Golden HTTP/JWT integration requires the compiler-pinned jose version");
for (const name of ["package.json", "bun.lock"]) {
  const expected = readFileSync(join(canonical, name), "utf8");
  if (readFileSync(join(target, name), "utf8") !== expected || readFileSync(join(dependency, name), "utf8") !== expected) {
    throw new Error(`Generated/cached JWT ${name} differs from compiler-owned declaration`);
  }
}
symlinkSync(join(dependency, "node_modules"), join(target, "node_modules"), "dir");
const { SignJWT, generateKeyPair, exportJWK } = await import(pathToFileURL(join(dependency, "node_modules/jose/dist/webapi/index.js")).href);
const keys = await generateKeyPair("RS256", { modulusLength: 2048, extractable: true });
const publicKey = { ...await exportJWK(keys.publicKey), kid: "golden-rsa", alg: "RS256", use: "sig" };
const issuer = "https://golden-issuer.example.test";
const discovery = `${issuer}/.well-known/openid-configuration`;
const jwksUri = `${issuer}/jwks`;
const providerCalls: string[] = [];
globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
  const url = input instanceof Request ? input.url : String(input);
  if (url !== discovery && url !== jwksUri) return originalFetch(input, init);
  providerCalls.push(url);
  if (init?.redirect !== "error" || init?.credentials !== "omit") throw new Error("Provider fetch lost its closed request policy");
  return Response.json(url === discovery ? { issuer, jwks_uri: jwksUri } : { keys: [publicKey] });
}) as typeof fetch;
Bun.env.SQLITE_PATH = join(root, "golden.sqlite");
delete Bun.env.DATABASE_URL;
const app = await import(pathToFileURL(join(target, "app.ts")).href);
database = new Database(Bun.env.SQLITE_PATH, { strict: true });
const db = database;
server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
const owner = "00000000-0000-4000-8000-000000000001";
const environment = {
  DATABASE_URL: "https://db.test/golden",
  SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test",
  OIDC_ISSUER: issuer,
  OIDC_AUDIENCE: "golden-todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"),
  MAIL_SENDER: "todo@example.test",
};
let now: number;
beforeEach(async () => {
  await app.initializeApplication(environment);
  providerCalls.length = 0;
  db.exec('DELETE FROM todo; DELETE FROM "__jadpo_auth_sessions"; DELETE FROM "user";');
  now = Date.now();
  db.prepare('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)')
    .run(owner, "owner", "owner@example.test", "active", now, null);
});
const countTodos = () => (db.prepare("SELECT count(*) AS count FROM todo").get() as { count: number }).count;
async function request(credential?: string) {
  return fetch(new URL("/todos", server!.url), {
    method: "POST",
    headers: { "content-type": "application/json", ...(credential ? { authorization: `Bearer ${credential}` } : {}) },
    body: JSON.stringify({ title: "HTTP-authenticated todo" }),
  });
}
async function requestUserTodos(credential: string) {
  return fetch(new URL(`/users/${owner}/todos`, server!.url), {
    method: "GET",
    headers: { authorization: `Bearer ${credential}` },
  });
}
async function accepted(credential: string) {
  const response = await request(credential);
  expect(response.status).toBe(201);
  const body = await response.json();
  expect(body).toMatchObject({ title: "HTTP-authenticated todo", status: "open", due_at: null });
  expect(db.prepare("SELECT owner_id, title FROM todo WHERE id = ?").get(body.id)).toEqual({ owner_id: owner, title: body.title });
}
async function rejected(credential: string | undefined, code: string) {
  const before = countTodos();
  const response = await request(credential);
  expect(response.status).toBe(401);
  const text = await response.text();
  expect(JSON.parse(text).error.code).toBe(code);
  expect(countTodos()).toBe(before);
  if (credential) expect(text).not.toContain(credential);
  expect(text).not.toContain(environment.SESSION_SIGNING_KEY);
}
async function token() {
  const seconds = Math.floor(now / 1000);
  return new SignJWT({ iss: issuer, aud: environment.OIDC_AUDIENCE, sub: "owner", iat: seconds, exp: seconds + 120 })
    .setProtectedHeader({ alg: "RS256", kid: "golden-rsa", typ: "JWT" }).sign(keys.privateKey);
}
async function tokenWithUntrustedEmail() {
  const seconds = Math.floor(now / 1000);
  return new SignJWT({ iss: issuer, aud: environment.OIDC_AUDIENCE, sub: "owner", email: "attacker@example.test", iat: seconds, exp: seconds + 120 })
    .setProtectedHeader({ alg: "RS256", kid: "golden-rsa", typ: "JWT" }).sign(keys.privateKey);
}

test("actual HTTP protected todo creation binds the signed user's identity and rejects absent or invalid credentials", async () => {
  const credential = await app.authenticationHost().issue("api_bearer", "owner", now + 3_600_000, now);
  await rejected(undefined, "authentication_required");
  await rejected("INVALID_GOLDEN_CREDENTIAL_SENTINEL", "invalid_credentials");
  await accepted(credential.credential);
  expect(countTodos()).toBe(1);
  expect(providerCalls).toHaveLength(0);
});

test("the selected golden JWT validator resolves a typed primary user and authorizes real HTTP creation", async () => {
  const credential = await token();
  const principal = await app.authenticationHost().authenticate(new Request("https://todo.test/todos", { headers: { authorization: `Bearer ${credential}` } }), false, now);
  expect(principal).toEqual({ kind: "user", subject: "owner", authenticationStrength: "primary", values: { user_id: owner } });
  await accepted(credential);
  expect(countTodos()).toBe(1);
  expect(providerCalls).toEqual([discovery, jwksUri]);
});

test("self todo output omits the provisioning-only email even when a JWT supplies a different email", async () => {
  const credential = await tokenWithUntrustedEmail();
  const response = await requestUserTodos(credential);
  expect(response.status).toBe(200);
  expect(await response.json()).toEqual({ user_id: owner, todos: [] });
  expect(providerCalls).toEqual([discovery, jwksUri]);
});

test("a corrupted signature cannot pass the selected golden JWT validator or mutate over HTTP", async () => {
  const credential = await token();
  const parts = credential.split(".");
  const signature = Buffer.from(parts[2], "base64url");
  signature[0] ^= 1;
  parts[2] = signature.toString("base64url");
  await rejected(parts.join("."), "invalid_credentials");
  expect(countTodos()).toBe(0);
  expect(providerCalls).toEqual([discovery, jwksUri]);
});
