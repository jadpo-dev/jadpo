import { afterAll, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { copyGoldenFixture } from "./golden-fixture";

const root = mkdtempSync(join(tmpdir(), "jadpo-config-preflight-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const project = join(root, "golden"); copyGoldenFixture(project);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const result = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (result.exitCode) throw Error(`Golden preflight build failed: ${result.stdout}\n${result.stderr}`);
const target = join(project, "build/target");
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
if (!existsSync(join(dependency, "node_modules/jose/package.json"))) throw Error("Requires pinned JOSE cache");
symlinkSync(join(dependency, "node_modules"), join(target, "node_modules"), "dir");
const marker = join(root, "listener-started");
const preload = join(root, "listener-probe.ts");
writeFileSync(preload, `import {writeFileSync} from "node:fs"; import {Server} from "node:net"; const marker=${JSON.stringify(marker)}; const serve=Bun.serve; Bun.serve=((...args:any[])=>{writeFileSync(marker,"Bun.serve"); return serve(...args);}) as typeof Bun.serve; const listen=Server.prototype.listen; Server.prototype.listen=function(...args:any[]){writeFileSync(marker,"Node.listen");return listen.apply(this,args as any);};`);
const environment = {
  PATH: Bun.env.PATH ?? "", PORT: "0", SQLITE_PATH: join(root, "must-not-open.sqlite"),
  DATABASE_URL: "https://db.test/golden", SESSION_SIGNING_KEY: "preflight-secret-session-canary-32-characters",
  BROWSER_ORIGIN: "https://todo.test", OIDC_ISSUER: "https://issuer.test", OIDC_AUDIENCE: "golden-todo",
  MAIL_API_KEY: "preflight-secret-mail-canary", MAIL_SENDER: "todo@example.test",
  JADPO_DEBUG_TARGET_STACKS: "1",
};
function run(override: Record<string, string>) {
  rmSync(marker, { force: true });
  const child = Bun.spawnSync([process.execPath, "--no-install", "--env-file=/dev/null", "--preload", preload, join(target, "app.ts")], { env: { ...environment, ...override }, stdout: "pipe", stderr: "pipe", timeout: 5000 });
  const output = child.stdout.toString() + child.stderr.toString();
  expect(child.exitCode).toBe(1); expect(existsSync(marker)).toBe(false); expect(existsSync(environment.SQLITE_PATH)).toBe(false);
  expect(output).not.toContain("Unsupported adapter"); expect(output).not.toContain("required persistence schema"); expect(output).not.toContain("runtime.ready");
  for (const value of [environment.SESSION_SIGNING_KEY, environment.MAIL_API_KEY, "user:password", "canary-invalid-issuer"]) expect(output).not.toContain(value);
  const records = output.split("\n").filter(line => line.startsWith("{")).map(line => JSON.parse(line));
  const startup = records.filter(value => value.classification === "RUNTIME_STARTUP_FAILED");
  expect(startup.length).toBe(1); expect(startup[0].sourceRevision).toMatch(/^src_[0-9a-f]{16}$/); expect(startup[0].attributes).toEqual({});
  return records.filter(value => value.kind === "configuration_diagnostic").map(({ code, declaration }) => ({ code, declaration }));
}
test("CONFIG-001 actual native main diagnoses missing mail key before adapter initialization or either native listener", () => {
  expect(run({ MAIL_API_KEY: "" })).toEqual([{ code: "config_required", declaration: "TodoConfiguration.mail_api_key" }]);
});
test("CONFIG-002 actual native main aggregates both qualified malformed declarations without credential URL disclosure", () => {
  expect(run({ DATABASE_URL: "postgres://user:password@", OIDC_ISSUER: "canary-invalid-issuer" })).toEqual([
    { code: "config_invalid", declaration: "TodoConfiguration.database_url" },
    { code: "config_invalid", declaration: "TodoConfiguration.oidc_issuer" },
  ]);
});
test("preflight artifact has no persistence, authentication or provider import and reuses emitted declaration validators", () => {
  const source = readFileSync(join(target, "configuration-preflight.ts"), "utf8");
  expect(source).not.toMatch(/\bimport\s/); expect(source).toContain("validate_DatabaseUrl"); expect(source).toContain("validate_MailApiKey");
  expect(readFileSync(join(target, "persistence.ts"), "utf8")).toContain('import "./configuration-preflight.ts"');
});

test("healthy native main completes declared preflight before opening the selected adapter and listening", async () => {
  const healthy = join(root, "healthy"); mkdirSync(healthy);
  writeFileSync(join(healthy, "app.jadpo"), readFileSync(resolve("tests/compile/pass/lifecycle/170_lifecycle_initial_values.jadpo"), "utf8"));
  const built = Bun.spawnSync([compiler, "build", healthy], { stdout: "pipe", stderr: "pipe" });
  expect(built.exitCode).toBe(0);
  const reservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
  const port = reservation.port; reservation.stop(true);
  const postgres = Bun.env.JADPO_CONFIG_PREFLIGHT_DATABASE_URL;
  if (postgres) { const url = new URL(postgres); if (!["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Requires disposable local PostgreSQL"); }
  const env: Record<string, string> = { PATH: environment.PATH, PORT: String(port), JADPO_LIFECYCLE_RETENTION: "30d", SQLITE_PATH: join(root, "healthy.sqlite") };
  if (postgres) env.DATABASE_URL = postgres;
  const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", join(healthy, "build/target/app.ts")], { env, stdout: "pipe", stderr: "pipe" });
  const stderr = new Response(child.stderr).text(); let output = ""; let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const ready = (async () => { const reader = child.stdout.getReader(); try { while (!output.includes("runtime.ready")) { const next = await reader.read(); if (next.done) throw Error(`No native readiness: ${output}; ${await stderr}`); output += new TextDecoder().decode(next.value); } } finally { reader.releaseLock(); } })();
    await Promise.race([ready, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(Error("Native startup deadline exceeded")), 5000); })]);
    const response = await fetch(`http://127.0.0.1:${port}/lifecycle/users`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name: "preflight owner", batch_group: "default" }) });
    expect(response.status).toBe(200); expect((await response.json()).name).toBe("preflight owner");
  } finally { clearTimeout(timer); child.kill(); await child.exited; }
  expect(await stderr).not.toContain("configuration_diagnostic"); expect(await stderr).not.toContain("RUNTIME_STARTUP_FAILED");
});
