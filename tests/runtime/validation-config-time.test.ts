import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const root = mkdtempSync(join(tmpdir(), "jadpo-config-time-validation-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const compiler = Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
const emptyEnvironmentFile = `--env-file=${process.platform === "win32" ? "NUL" : "/dev/null"}`;
const canary = "fixture-secret-canary-489211";
const source = `
type ApiKey = Text { min_length: 8 }
type Label = Text { min_length: 2 }
config Settings {
  api_key: ApiKey { binding: "VALIDATION_KEY" secret: true }
  label: Label { binding: "VALIDATION_LABEL" }
  enabled: Bool { binding: "VALIDATION_ENABLED" default: true }
  count: Int { binding: "VALIDATION_COUNT" default: 1 }
}
type Reading = Object { now: Instant label: Label }
type Readings = Object { first: Reading second: Reading }
action read_inner() -> Reading { return Reading { now: clock.now label: config.label } }
action read_outer() -> Readings {
  var first = read_inner()
  var second = read_inner()
  return Readings { first: first second: second }
}
type TimeInput = Object { instant: Instant duration: Duration }
type TimeResult = Object { normalized: Instant shifted: Instant }
action shift_time(input: TimeInput) -> TimeResult {
  return TimeResult { normalized: input.instant shifted: temporal.add_elapsed(input.instant, input.duration) }
}
route POST /time { auth: none input: TimeInput output: TimeResult run: shift_time(input) }
route GET /reading { auth: none output: Readings run: read_outer() }
fixture first_fixture {
  clock: fixed Instant("2024-02-29T23:59:59.999Z")
  config { api_key: secret("${canary}") label: Label("first") }
}
fixture second_fixture {
  clock: fixed Instant("2026-01-01T00:00:00Z")
  config { api_key: secret("${canary}") label: Label("second") }
}
`;
const authoredTests = [
  `test "advance then fail" using first_fixture {
    advance clock by Duration("PT0.001S")
    var reading = call read_outer()
    assert reading.first.now == Instant("2024-03-01T00:00:00Z")
    assert reading.first.now == reading.second.now
    assert false
  }`,
  `test "different fixture survives previous failure" using second_fixture {
    var reading = call read_outer()
    assert reading.first.label == Label("second")
    assert reading.first.now == Instant("2026-01-01T00:00:00Z")
    assert reading.first.now == reading.second.now
  }`,
  `test "original fixture starts fresh after failure" using first_fixture {
    var reading = call read_outer()
    assert reading.first.label == Label("first")
    assert reading.first.now == Instant("2024-02-29T23:59:59.999Z")
  }`,
];
const intentionalFailures = new Map<string, string>();
function build(name: string, tests: string[]) {
  const directory = join(root, name);
  mkdirSync(directory);
  const text = source + tests.join("\n");
  writeFileSync(join(directory, "app.jadpo"), text);
  const built = Bun.spawnSync([compiler, "build", directory], { stdout: "pipe", stderr: "pipe" });
  if (built.exitCode !== 0) throw new Error(`config/time source failed to build:\n${built.stdout}\n${built.stderr}`);
  const entry = join(directory, "build/target/app.ts");
  const failureStart = text.indexOf("assert false") + "assert ".length;
  intentionalFailures.set(entry, `assertion failed at source bytes ${failureStart}..${failureStart + "false".length}`);
  return entry;
}
const entry = build("normal", authoredTests);
const reorderedEntry = build("reordered", [authoredTests[2], authoredTests[0], authoredTests[1]]);
const app = await import(pathToFileURL(entry).href);
const reordered = await import(pathToFileURL(reorderedEntry).href);
const environment = { PATH: Bun.env.PATH ?? "", VALIDATION_KEY: canary, VALIDATION_LABEL: "production", PORT: "0" };

test("failed authored tests cannot leak clock or configuration state into later tests or later runs", async () => {
  for (const [instance, entrypoint] of [[app, entry], [reordered, reorderedEntry], [app, entry]]) {
    const report = await instance.runTests();
    expect(report.summary).toEqual({ total: 3, passed: 2, failed: 1 });
    expect(Object.fromEntries(report.results.map((item: any) => [item.name, item.status]))).toEqual({
      "advance then fail": "failed",
      "different fixture survives previous failure": "passed",
      "original fixture starts fresh after failure": "passed",
    });
    expect(report.evidence.surfaces).toEqual({ pure: 0, callable: 3, route: 0, job: 0 });
    expect(report.results.find((item: any) => item.name === "advance then fail").diagnostic.message).toBe(intentionalFailures.get(entrypoint));
    expect(JSON.stringify(report)).not.toContain(canary);
  }
});

test("fixture isolation also holds across a fresh process", async () => {
  const runner = join(root, "run-fixtures.ts");
  writeFileSync(runner, `import { runTests } from ${JSON.stringify(entry)}; console.log(JSON.stringify(await runTests()));`);
  for (let count = 0; count < 2; count++) {
    const child = Bun.spawnSync([process.execPath, "--no-install", emptyEnvironmentFile, runner], { env: environment, stdout: "pipe", stderr: "pipe", timeout: 5000 });
    expect(child.exitCode).toBe(0);
    const report = JSON.parse(child.stdout.toString());
    expect(report.summary).toEqual({ total: 3, passed: 2, failed: 1 });
    expect(child.stdout.toString() + child.stderr.toString()).not.toContain(canary);
  }
});

test("missing and malformed startup configuration exits safely without reporting a listening server", () => {
  for (const override of [
    { VALIDATION_KEY: "" }, { VALIDATION_KEY: "short" }, { VALIDATION_LABEL: "x" },
    { VALIDATION_ENABLED: "TRUE" }, { VALIDATION_ENABLED: "1" },
    { VALIDATION_COUNT: "1.5" }, { VALIDATION_COUNT: "01" },
  ]) {
    const result = Bun.spawnSync([process.execPath, "--no-install", emptyEnvironmentFile, entry], { env: { ...environment, ...override }, stdout: "pipe", stderr: "pipe", timeout: 5000 });
    expect(result.exitCode).toBe(1);
    const output = result.stdout.toString() + result.stderr.toString();
    expect(output).toContain("RUNTIME_STARTUP_FAILED");
    expect(output).not.toContain("runtime.ready");
    expect(output).not.toContain(canary);
  }
});

test("production launch cannot satisfy missing configuration from ambient env files", () => {
  const directory = join(root, "normal");
  for (const filename of [".env", ".env.local", ".env.test"]) {
    writeFileSync(join(directory, filename), `VALIDATION_KEY=${canary}\nVALIDATION_LABEL=from_file\n`);
  }
  const result = Bun.spawnSync([process.execPath, "--no-install", emptyEnvironmentFile, entry], { cwd: directory, env: { PATH: environment.PATH, PORT: "0" }, stdout: "pipe", stderr: "pipe", timeout: 5000 });
  expect(result.exitCode).toBe(1);
  const output = result.stdout.toString() + result.stderr.toString();
  expect(output).toContain("RUNTIME_STARTUP_FAILED");
  expect(output).not.toContain("runtime.ready");
  expect(output).not.toContain(canary);
});

test("a successful process restart loads its own configuration and serves stable nested operation time", async () => {
  for (const label of ["before_restart", "after_restart"]) {
    const reservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response("reserved") });
    const port = reservation.port;
    reservation.stop(true);
    const child = Bun.spawn([process.execPath, "--no-install", emptyEnvironmentFile, entry], { env: { ...environment, VALIDATION_LABEL: label, PORT: String(port) }, stdout: "pipe", stderr: "pipe" });
    let timer: ReturnType<typeof setTimeout> | undefined;
    let output = "";
    try {
      const ready = (async () => {
        const reader = child.stdout.getReader();
        try {
          while (!output.includes("runtime.ready")) {
            const chunk = await reader.read();
            if (chunk.done) throw new Error(`process ended before readiness: ${output}`);
            output += new TextDecoder().decode(chunk.value);
          }
        } finally { reader.releaseLock(); }
      })();
      await Promise.race([ready, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("startup readiness deadline exceeded")), 5000); })]);
      const response = await fetch(`http://127.0.0.1:${port}/reading`);
      expect(response.status).toBe(200);
      const body: any = await response.json();
      expect(body.first.label).toBe(label);
      expect(body.second.label).toBe(label);
      expect(body.first.now).toBe(body.second.now);
      expect(body.first.now).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
      expect(JSON.stringify(body) + output).not.toContain(canary);
    } finally {
      clearTimeout(timer);
      child.kill();
      await child.exited;
    }
  }
});

async function boundary(instant: string, duration = "PT0S") {
  const response = await app.handleRequest(new Request("https://time.test/time", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ instant, duration }) }));
  return { status: response.status, body: await response.json() };
}
test("the generated request boundary normalizes offsets and millisecond arithmetic across leap midnight", async () => {
  const result = await boundary("2024-03-01T05:44:59.999+05:45", "PT0.001S");
  expect(result).toEqual({ status: 200, body: { normalized: "2024-02-29T23:59:59.999Z", shifted: "2024-03-01T00:00:00.000Z" } });
  expect(await boundary(result.body.shifted, "-PT0.001S")).toEqual({ status: 200, body: { normalized: "2024-03-01T00:00:00.000Z", shifted: "2024-02-29T23:59:59.999Z" } });
});
test("invalid date precision and offsets are rejected before authored arithmetic", async () => {
  for (const instant of ["2025-02-29T12:00:00Z", "2026-04-31T12:00:00Z", "2026-01-01T24:00:00Z", "2026-01-01T12:00:00", "2026-01-01T12:00:00.0001Z", "2026-01-01T12:00:00-00:00"]) {
    const result = await boundary(instant);
    expect(result.status).toBe(422);
    expect(result.body.error.code).toBe("invalid_input");
    expect(result.body.error).not.toHaveProperty("stack");
  }
});
test("every accepted offset instant remains valid when its canonical output is decoded again", async () => {
  for (const [instant, status] of [["0001-01-01T00:00:00+01:00", 422], ["0001-01-01T00:00:00Z", 200], ["9999-12-31T23:59:59-01:00", 422], ["9999-12-31T23:59:59Z", 200]] as const) {
    const first = await boundary(instant);
    expect(first.status).toBe(status);
    if (status === 422) {
      expect(first.body.error.code).toBe("invalid_input");
    } else {
      expect(first.status).toBe(200);
      const second = await boundary(first.body.normalized);
      expect(second.status).toBe(200);
      expect(second.body).toEqual(first.body);
    }
  }
});
test("elapsed arithmetic overflow is contained as a safe runtime fault", async () => {
  const result = await boundary("9999-12-31T23:59:59.999Z", "PT0.001S");
  expect(result.status).toBe(500);
  expect(result.body.error.code).toBe("internal_fault");
  expect(result.body.error).not.toHaveProperty("stack");
  expect(result.body.error).not.toHaveProperty("details");
});
