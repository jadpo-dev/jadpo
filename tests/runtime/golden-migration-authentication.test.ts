import { copyGoldenFixture } from "./golden-fixture";
import { afterAll, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Database } from "bun:sqlite";

const root = mkdtempSync(join(tmpdir(), "jadpo-golden-migration-"));
const project = join(root, "golden-todo-migration");
copyGoldenFixture(project);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Golden migration failed to build:\n${build.stdout}\n${build.stderr}`);
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
if (!existsSync(join(dependency, "node_modules/jose/package.json"))) throw new Error("Install the pinned JWT dependency before running this suite");
symlinkSync(join(dependency, "node_modules"), join(project, "build/target/node_modules"), "dir");
const databasePath = join(project, "build", "golden.sqlite");
Bun.env.SQLITE_PATH = databasePath;
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const db = new Database(databasePath, { strict: true });
const owner = "00000000-0000-4000-8000-000000000001";
const service = "00000000-0000-4000-8000-000000000002";
const now = Date.now();
const environment = {
  DATABASE_URL: "https://db.test/golden",
  SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test",
  OIDC_ISSUER: "https://issuer.test",
  OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"),
  MAIL_SENDER: "todo@example.test",
};
await app.initializeApplication(environment);
db.prepare('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)').run(owner, "owner", "owner@example.test", "active", now, null);
db.prepare('INSERT INTO service (id, owner_id, name, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)').run(service, owner, "todo-service", "active", now, null);
const host = app.authenticationHost();
const serviceCredential = await host.issueServiceCredential("api_bearer", "todo-service", now + 3_600_000, now);
const userCredential = await host.issue("api_bearer", "owner", now + 3_600_000, now);
const browserCredential = await host.issue("browser_session", "owner", now + 3_600_000, now);
if (!serviceCredential.credential || !userCredential.credential || !browserCredential.credential || !browserCredential.csrfToken || !browserCredential.setCookie) throw new Error("Golden migration credential setup failed");

afterAll(() => {
  db.close();
  delete Bun.env.SQLITE_PATH;
  rmSync(root, { recursive: true, force: true });
});

async function request(path: string, init?: RequestInit): Promise<Response> {
  return app.handleRequest(new Request(`https://todo.test${path}`, init));
}

test("generated migration target enforces authentication and browser origin", async () => {
  expect((await request("/health/live")).status).toBe(200);
  expect((await request("/todos", { method: "POST", body: "{}" })).status).toBe(401);
  expect((await request("/todos", { method: "POST", headers: { authorization: "Bearer invalid" }, body: "{}" })).status).toBe(401);
  expect((await request("/todos", { method: "POST", headers: { authorization: `Bearer ${serviceCredential.credential}`, "content-type": "application/json" }, body: JSON.stringify({ title: "service" }) })).status).toBe(403);

  const userResponse = await request("/todos", {
    method: "POST",
    headers: { authorization: `Bearer ${userCredential.credential}`, "content-type": "application/json" },
    body: JSON.stringify({ title: "user" }),
  });
  expect(userResponse.status).toBe(201);

  const cookie = browserCredential.setCookie.split(";", 1)[0];
  const browserResponse = await request("/todos", {
    method: "POST",
    headers: { cookie, origin: "https://todo.test", "x-jadpo-csrf": browserCredential.csrfToken, "content-type": "application/json" },
    body: JSON.stringify({ title: "browser" }),
  });
  expect(browserResponse.status).toBe(201);
  const wrongOrigin = await request("/todos", {
    method: "POST",
    headers: { cookie, origin: "https://evil.test", "x-jadpo-csrf": browserCredential.csrfToken, "content-type": "application/json" },
    body: JSON.stringify({ title: "wrong-origin" }),
  });
  expect(wrongOrigin.status).toBe(403);
  expect((await wrongOrigin.json()).error.code).toBe("csrf_rejected");
});
