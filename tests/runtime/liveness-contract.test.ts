import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Exercise reserved liveness before readiness with an actually unavailable DB.
const root = mkdtempSync(join(tmpdir(), "jadpo-authored-liveness-"));
const project = join(root, "application");
mkdirSync(project);
const health = `
enum HealthStatus { healthy }
output PublicHealth { status: HealthStatus }
route GET /health/live {
    auth: none
    output: PublicHealth
    action: { return PublicHealth { status: HealthStatus.healthy } }
}
`;
writeFileSync(join(project, "app.jadpo"), readFileSync(resolve("examples/persistence-seed/app.jadpo"), "utf8") + health);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Local health fixture build failed: ${build.stdout} ${build.stderr}`);
const priorSqlite = Bun.env.SQLITE_PATH, priorPostgres = Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = join(root, "missing-directory", "application.sqlite");
delete Bun.env.DATABASE_URL;
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
afterAll(() => {
  if (priorSqlite === undefined) delete Bun.env.SQLITE_PATH; else Bun.env.SQLITE_PATH = priorSqlite;
  if (priorPostgres === undefined) delete Bun.env.DATABASE_URL; else Bun.env.DATABASE_URL = priorPostgres;
  rmSync(root, { recursive: true, force: true });
});

test("authored constant liveness survives unavailable persistence and ambient invalid credentials", async () => {
  const openapi = JSON.parse(readFileSync(join(project, "build/openapi/openapi.json"), "utf8"));
  expect(openapi.paths["/health/live"].get.responses["200"].content["application/json"].schema.$ref)
    .toBe("#/components/schemas/PublicHealth");
  expect(openapi.components.schemas.HealthStatus.enum).toEqual(["healthy"]);
  const readiness = await app.handleRequest(new Request("http://localhost/health/ready"));
  expect(readiness.status).toBe(503);
  expect((await readiness.json()).checks.database).toBe("unavailable");
  for (const headers of [{}, { authorization: "Bearer definitely-invalid", cookie: "session=also-invalid" }]) {
    const response = await app.handleRequest(new Request("http://localhost/health/live", { headers }));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ status: "healthy" });
  }
  const ordinary = await app.handleRequest(new Request("http://localhost/customers", { method: "POST" }));
  expect(ordinary.status).toBe(503);
  expect((await ordinary.json()).error.code).toBe("dependency_unavailable");
});
