import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const base = readFileSync(resolve("examples/first-party-authentication/app.jadpo"), "utf8");
const environment = { AUTH_SIGNING_KEY: Buffer.alloc(32, 7).toString("base64url"), AUTH_PREVIOUS_SIGNING_KEY: Buffer.alloc(32, 8).toString("base64url"), BROWSER_ORIGIN: "https://example.test" };

for (const branch of [false, true]) {
  // Each case builds and inspects two fresh projects; native request deadlines
  // stay bounded separately while the test allows concurrent compiler load.
  test(`actual literal admission ${branch ? "branch remains unsupported" : "matches a source-only candidate"}`, async () => {
    const root = mkdtempSync(join(tmpdir(), "jadpo-admission-candidate-"));
    const oldDatabase = Bun.env.DATABASE_URL, oldSqlite = Bun.env.SQLITE_PATH;
    const body = branch ? 'if true { return "literal witness" } else { return "literal witness" }' : 'return "literal witness"';
    const generatedPins: Record<string, string> = {};
    const observations: unknown[] = [];
    try {
      delete Bun.env.DATABASE_URL;
      for (const version of ["before", "after"]) {
        const project = join(root, version); mkdirSync(project);
        writeFileSync(join(project, "app.jadpo"), `${base}\nroute GET /witness { ${version === "after" ? "auth: none " : ""}output: Text action: { ${body} } }\n`);
        const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
        expect(build.exitCode).toBe(0);
        Bun.env.SQLITE_PATH = join(project, "fixture.sqlite");
        const file = join(project, "build/target/app.ts");
        const expected = JSON.parse(readFileSync(join(project, "build/approval/subject.json"), "utf8"));
        const pins = expected.canonical.after.provenance.generated_artifacts.target.files;
        for (const [name, pin] of Object.entries(pins) as [string, any][]) expect(`sha256:${createHash("sha256").update(readFileSync(join(project, "build", name))).digest("hex")}`).toBe(pin.byte_digest);
        generatedPins[version] = createHash("sha256").update(readFileSync(file)).digest("hex");
        const app = await import(pathToFileURL(file).href);
        await app.initializeApplication(environment);
        const application = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest });
        const deployment = app.createDeploymentServer(0);
        try {
          expect(application.port).not.toBe(deployment.port);
          const ready = await fetch(`http://127.0.0.1:${deployment.port}/health/ready`, { signal: AbortSignal.timeout(1000) });
          const readyBody = await ready.json();
          expect(ready.status).toBe(200); expect(readyBody.checks).toEqual({ database: "available" });
          const request = new Request(`http://127.0.0.1:${application.port}/witness`, { signal: AbortSignal.timeout(1000) });
          const response = await fetch(request);
          expect(response.status).toBe(version === "before" ? 401 : 200);
          const body = await response.json();
          if (version === "before") expect(body.error.code).toBe("authentication_required"); else expect(body).toBe("literal witness");
          observations.push({ version, request: { method: request.method, path: new URL(request.url).pathname, authorization: request.headers.get("authorization"), cookie: request.headers.get("cookie") }, readiness: { status: ready.status, body: readyBody }, response: { status: response.status, body } });
        } finally { application.stop(true); deployment.stop(true); }
      }
      const before = JSON.parse(readFileSync(join(root, "before/build/approval/subject.json"), "utf8"));
      const result = Bun.spawnSync([compiler, "approval", join(root, "after"), "--before", join(root, "before"), "--expected-before-state", before.canonical.after.state_digest], { stdout: "pipe", stderr: "pipe" });
      expect(result.exitCode).toBe(0);
      const s = JSON.parse(result.stdout.toString());
      const row = s.canonical.scenarios.find((r: any) => r.route === "GET /witness");
      expect(row.counterexample.status).toBe(branch ? "not_generated" : "source_derived_anonymous_admission_candidate");
      expect(row.execution).toBe("not_proved"); expect(row.feasibility).toBe("not_analyzed");
      expect(s.canonical.evidence.runtime_validated).toEqual([]);
      expect(s.canonical.after.impact.anonymous_admission_candidates.runtime_observations).toBeNull();
      expect(s.canonical.release_authority).toBe(false);
      if (!branch) expect(row.counterexample.facts.assumptions.database).toBe("native_readiness_available_if_persistent_entities");
      for (const version of ["before", "after"]) expect(createHash("sha256").update(readFileSync(join(root, version, "build/target/app.ts"))).digest("hex")).toBe(generatedPins[version]);
      console.log(JSON.stringify({ kind: "native_anonymous_admission_observation", branch, observations, generatedPins, canonical: { counterexampleStatus: row.counterexample.status, execution: row.execution, feasibility: row.feasibility, releaseAuthority: s.canonical.release_authority } }));
    } finally {
      if (oldDatabase === undefined) delete Bun.env.DATABASE_URL; else Bun.env.DATABASE_URL = oldDatabase;
      if (oldSqlite === undefined) delete Bun.env.SQLITE_PATH; else Bun.env.SQLITE_PATH = oldSqlite;
      rmSync(root, { recursive: true, force: true });
    }
  }, 20_000);
}
