import { afterAll, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = mkdtempSync(join(tmpdir(), "jadpo-deployment-plane-"));
const project = join(root, "application"); mkdirSync(project);
// Accepted authority/credential runtime, plus a pure header route to exercise
// the real Node application listener selected by checked header lowering.
writeFileSync(join(project, "app.jadpo"), readFileSync(resolve("examples/first-party-authentication/app.jadpo"), "utf8") + `
route GET /native-header { auth: none headers: { trace: Text from "X-Trace" } output: Text action: { return headers.trace } }
`);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const built = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (built.exitCode) throw Error(`Deployment fixture failed: ${built.stdout}\n${built.stderr}`);
const postgresUrl = Bun.env.JADPO_DEPLOYMENT_READINESS_DATABASE_URL;
if (postgresUrl) { const url = new URL(postgresUrl); if (!["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Requires disposable local PostgreSQL"); }
const old = Object.fromEntries(["DATABASE_URL", "SQLITE_PATH"].map(key => [key, Bun.env[key]]));
const path = join(root, "application.sqlite");
if (postgresUrl) Bun.env.DATABASE_URL = postgresUrl; else delete Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = path;
const deploymentAudit = JSON.parse(readFileSync(join(project, "build/audit/deployment.json"), "utf8"));
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const environment = { AUTH_SIGNING_KEY: Buffer.alloc(32, 7).toString("base64url"), AUTH_PREVIOUS_SIGNING_KEY: Buffer.alloc(32, 8).toString("base64url"), BROWSER_ORIGIN: "https://example.test" };
await app.initializeApplication(environment);
const postgres = postgresUrl ? new SQL({ url: postgresUrl, prepare: false }) : null;
const sqlite = postgres ? null : new Database(path);
const seed = 'INSERT INTO "user" (id, authentication_subject, enabled) VALUES ($1,$2,$3)';
const subject = "deployment-plane-user";
if (postgres) await postgres.unsafe(seed, ["00000000-0000-4000-8000-000000000007", subject, true]);
else sqlite!.prepare(seed).run("00000000-0000-4000-8000-000000000007", subject, 1);
const now = Date.now();
const issued = await app.authenticationHost().issue("api_bearer", subject, now + 3600000, now);
const browser = await app.authenticationHost().issue("browser_session", subject, now + 3600000, now);
const bunServer = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
const nodeServer = app.createApplicationServer();
await new Promise<void>((resolve, reject) => { nodeServer.once("error", reject); nodeServer.listen(0, "127.0.0.1", resolve); });
const nodeAddress = nodeServer.address(); if (!nodeAddress || typeof nodeAddress === "string") throw Error("Node application listener missing port");
const deploymentServer = app.createDeploymentServer(0);
afterAll(async () => {
  bunServer.stop(true); deploymentServer.stop(true); await new Promise<void>(resolve => nodeServer.close(() => resolve()));
  await postgres?.close(); sqlite?.close();
  for (const [key, value] of Object.entries(old)) { if (value === undefined) delete Bun.env[key]; else Bun.env[key] = value; }
  rmSync(root, { recursive: true, force: true });
});
const application = [
  (path: string, headers: HeadersInit) => app.handleRequest(new Request(`http://application.test${path}`, { headers })),
  (path: string, headers: HeadersInit) => fetch(`http://127.0.0.1:${bunServer.port}${path}`, { headers }),
  (path: string, headers: HeadersInit) => fetch(`http://127.0.0.1:${nodeAddress.port}${path}`, { headers }),
];
test("direct, real Bun and real Node application listeners refuse every compiler readiness alias with absent/invalid/valid credentials", async () => {
  for (const request of application) for (const headers of [{}, { authorization: "Bearer invalid-credential" }, { authorization: `Bearer ${issued.credential}` }, { cookie: `__Host-jadpo_session=${browser.credential}` }]) {
    for (const path of ["/health/ready", "/health", "/health/ready?checks=database"]) {
      const response = await request(path, headers); expect(response.status).toBe(404);
      const body = await response.json(); expect(body.error.code).toBe("not_found");
      const text = JSON.stringify(body); for (const field of ["database", "checks", "advisories", "not_ready", "available", issued.credential, environment.AUTH_SIGNING_KEY]) expect(text).not.toContain(field);
    }
    const live = await request("/health/live", headers); expect(live.status).toBe(200); expect(await live.json()).toEqual({ status: "live" });
  }
});
test("fresh deployment audit binds expected native planes without claiming live configuration", async () => {
  expect(deploymentAudit.status).toBe("compiler_supported_expected_native_contract");
  expect(deploymentAudit.facts.listener.hostname).toBe(deploymentServer.hostname);
  expect(deploymentAudit.facts.listener.factory).toBe("createDeploymentServer");
  expect(deploymentAudit.facts.listener.separate_from_application_listener).toBe(true);
  expect(deploymentAudit.facts.listener.port.required_api_argument).toBe(true);
  expect(deploymentAudit.facts.listener.port.native_main_environment).toBe("JADPO_DEPLOYMENT_PORT");
  expect(deploymentAudit.facts.listener.port.native_main_activation).toBe("only_when_explicitly_present");
  expect(deploymentAudit.facts.listener.port.resolved_value).toBeNull();
  expect(deploymentAudit.live_listener).toBeNull();
  expect(deploymentAudit.current_readiness).toBeNull();
  expect(deploymentAudit.worker_execution_profile).toBeNull();
  expect(deploymentAudit.runtime_conformance).toBe("not_established");
  expect(deploymentAudit.release_authority).toBe(false);
  expect(deploymentAudit.facts.application_plane.reserved_readiness_path).toBe("/health/ready");
  expect(deploymentAudit.facts.application_plane.deployment_aliases_automatically_mounted).toBe(false);
  expect(deploymentAudit.facts.readiness.database_check).toBe("native_database_readiness");
  for (const path of deploymentAudit.facts.readiness.paths) {
    const response = await fetch(`http://127.0.0.1:${deploymentServer.port}${path}`, { method: deploymentAudit.facts.readiness.method });
    expect(deploymentAudit.facts.readiness.response_statuses).toContain(response.status);
    const body = await response.json();
    expect(body.checks).toEqual({ database: "available" });
    expect(body.advisories).toEqual(deploymentAudit.facts.readiness.service_advisories);
  }
});
test("separate compiler-owned deployment listener reports bounded readiness but serves no business or authority routes", async () => {
  expect(deploymentServer.hostname).toBe("127.0.0.1"); expect(deploymentServer.port).not.toBe(bunServer.port); expect(deploymentServer.port).not.toBe(nodeAddress.port);
  for (const path of ["/health/ready", "/health"]) {
    const response = await fetch(`http://127.0.0.1:${deploymentServer.port}${path}`); expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ status: "ready", checks: { database: "available" }, advisories: {} });
  }
  for (const path of ["/identity", "/native-header", "/health/live"]) expect((await fetch(`http://127.0.0.1:${deploymentServer.port}${path}`, { headers: { authorization: `Bearer ${issued.credential}` } })).status).toBe(404);
  expect((await fetch(`http://127.0.0.1:${deploymentServer.port}/health/ready`, { method: "POST" })).status).toBe(404);
  const identity = await fetch(`http://127.0.0.1:${bunServer.port}/identity`, { headers: { authorization: `Bearer ${issued.credential}` } }); expect(identity.status).toBe(200);
});

test("an explicitly authored ordinary health route retains its checked output while compiler deployment aliases remain separate", async () => {
  const directory = join(root, "authored-health"); mkdirSync(directory);
  writeFileSync(join(directory, "app.jadpo"), `output Health { ok: Bool } route GET /health { auth: none output: Health action: { return Health { ok: true } } }`);
  const build = Bun.spawnSync([compiler, "build", directory], { stdout: "pipe", stderr: "pipe" }); expect(build.exitCode).toBe(0);
  const instance = await import(pathToFileURL(join(directory, "build/target/app.ts")).href);
  const noEntityAudit = JSON.parse(readFileSync(join(directory, "build/audit/deployment.json"), "utf8"));
  expect(noEntityAudit.facts.readiness.database_check).toBe("not_applicable_no_persistent_entities");
  const ready = await instance.handleDeploymentRequest(new Request("http://deployment.test/health/ready"));
  expect(noEntityAudit.facts.readiness.response_statuses).toContain(ready.status);
  expect((await ready.json()).checks).toEqual({});
  const response = await instance.handleRequest(new Request("http://authored.test/health", { headers: { authorization: "Bearer invalid" } })); expect(response.status).toBe(200); expect(await response.json()).toEqual({ ok: true });
  expect((await instance.handleRequest(new Request("http://authored.test/health/ready"))).status).toBe(404);
  expect(await (await instance.handleDeploymentRequest(new Request("http://deployment.test/health"))).json()).toEqual({ status: "ready", checks: {}, advisories: {} });
});

// An ordinary POST at /health never inherited the compiler's GET alias.
test("removing the default GET health alias preserves a supported authored POST at the same path", async () => {
  const directory = join(root, "authored-post-health"); mkdirSync(directory);
  writeFileSync(join(directory, "app.jadpo"), `output Health { ok: Bool } route POST /health { auth: none output: Health action: { return Health { ok: true } } }`);
  const build = Bun.spawnSync([compiler, "build", directory], { stdout: "pipe", stderr: "pipe" }); expect(build.exitCode).toBe(0);
  const instance = await import(pathToFileURL(join(directory, "build/target/app.ts")).href);
  const noEntityAudit = JSON.parse(readFileSync(join(directory, "build/audit/deployment.json"), "utf8"));
  expect(noEntityAudit.facts.readiness.database_check).toBe("not_applicable_no_persistent_entities");
  const ready = await instance.handleDeploymentRequest(new Request("http://deployment.test/health/ready"));
  expect(noEntityAudit.facts.readiness.response_statuses).toContain(ready.status);
  expect((await ready.json()).checks).toEqual({});
  const response = await instance.handleRequest(new Request("http://authored.test/health", { method: "POST" })); expect(response.status).toBe(200); expect(await response.json()).toEqual({ ok: true });
  expect((await instance.handleRequest(new Request("http://authored.test/health"))).status).toBe(404);
});
