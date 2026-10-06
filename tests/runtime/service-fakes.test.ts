import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = mkdtempSync(join(tmpdir(), "jadpo-service-fakes-"));
const project = join(root, "application");
const source = resolve("tests/compile/pass/170_checked_service_operation.jadpo");
const sourceText = readFileSync(source, "utf8");
const route = `\nroute POST /deliver {\n    auth: none\n    input: ReminderMessage\n    output: ReminderReceipt\n    run: deliver(input)\n}\n`;
mkdirSync(project, { recursive: true });
writeFileSync(join(project, "app.jadpo"), `${sourceText}${route}`);
const assurance = join(project, "tests/assurance");
mkdirSync(assurance, { recursive: true });
cpSync(resolve("tests/assurance/service-reference-mail-v0.1.json"), join(assurance, "service-reference-mail-v0.1.json"));
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Service fake fixture failed to build:\n${build.stdout}\n${build.stderr}`);

const previousNodeEnv = process.env.NODE_ENV;
const previousMailKey = Bun.env.MAIL_API_KEY;
const previousError = console.error;
const canary = "service-adapter-secret-canary";
let app: any;
let testAdapter: any;

beforeAll(async () => {
  process.env.NODE_ENV = "test";
  Bun.env.MAIL_API_KEY = canary;
  app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
  testAdapter = await import(pathToFileURL(join(project, "build/target/service-adapter.ts")).href);
});

afterAll(() => {
  console.error = previousError;
  if (previousNodeEnv === undefined) delete process.env.NODE_ENV;
  else process.env.NODE_ENV = previousNodeEnv;
  if (previousMailKey === undefined) delete Bun.env.MAIL_API_KEY;
  else Bun.env.MAIL_API_KEY = previousMailKey;
  rmSync(root, { recursive: true, force: true });
});

test("authored service fakes preserve adapter semantics and never open a provider connection", async () => {
  let connects = 0;
  const originalConnect = Bun.connect;
  const lines: string[] = [];
  Bun.connect = ((..._arguments: Parameters<typeof Bun.connect>) => {
    connects += 1;
    throw new Error("authored service fake attempted network egress");
  }) as typeof Bun.connect;
  console.error = value => { lines.push(String(value)); };
  try {
    for (let run = 0; run < 2; run += 1) {
      const report = await app.runTests();
      expect(report.status).toBe("passed");
      expect(report.summary).toEqual({ total: 6, passed: 6, failed: 0 });
      expect(report.evidence.surfaces).toMatchObject({ callable: 6 });
    }
  } finally {
    Bun.connect = originalConnect;
    console.error = previousError;
  }

  expect(connects).toBe(0);
  const attempts = lines
    .map(line => { try { return JSON.parse(line); } catch { return null; } })
    .filter(event => event?.eventName === "service.attempt");
  const expectedOutcomes = [
    "accepted",
    "rate_limited", "accepted",
    "rate_limited", "accepted",
    "pre_dispatch_failure", "pre_dispatch_failure", "pre_dispatch_failure",
    "outcome_unknown",
    "outcome_unknown",
  ];
  expect(attempts.map(event => event.outcome)).toEqual([...expectedOutcomes, ...expectedOutcomes]);
  expect(attempts.slice(0, 10).map(event => event.elapsedMs)).toEqual([0, 0, 50, 0, 50, 5_000, 10_050, 15_150, 0, 5_000]);
  expect(JSON.stringify(attempts)).not.toContain("fixture-only-provider-key");
  expect(JSON.stringify(attempts)).not.toContain(canary);
});

test("reports the declared mail advisory as unprobed without provider egress", async () => {
  let providerCalls = 0;
  const originalConnect = Bun.connect;
  const originalFetch = globalThis.fetch;
  Bun.connect = ((..._arguments: Parameters<typeof Bun.connect>) => {
    providerCalls += 1;
    throw new Error("readiness attempted mail egress");
  }) as typeof Bun.connect;
  globalThis.fetch = (async () => {
    providerCalls += 1;
    throw new Error("readiness attempted mail fetch");
  }) as typeof fetch;
  try {
    const response = await app.handleDeploymentRequest(new Request("https://jadpo.test/health/ready"));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      status: "ready",
      checks: { database: "available" },
      advisories: { reminder_mail: "unprobed" },
    });
    expect(providerCalls).toBe(0);
  } finally {
    Bun.connect = originalConnect;
    globalThis.fetch = originalFetch;
  }
});

test("adapter validation precedes fake consumption and malformed or exhausted fakes become unknown", async () => {
  let connects = 0;
  const originalConnect = Bun.connect;
  Bun.connect = ((..._arguments: Parameters<typeof Bun.connect>) => {
    connects += 1;
    throw new Error("service fake attempted network egress");
  }) as typeof Bun.connect;
  const context = () => ({
    operationId: "service-fake-validation-test",
    monotonicStartedAt: performance.now(),
    serviceAttemptBudget: { attemptsUsed: 0 },
    signal: null,
  });
  try {
    const invalidInputFake = { outcomes: [{ kind: "accepted", value: { accepted_at: "2026-01-15T12:00:00Z" } }], cursor: { index: 0 }, elapsedMs: 0 };
    await expect(testAdapter.invokeReferenceMail({ idempotency_key: "bad-key" }, canary, context(), invalidInputFake))
      .rejects.toMatchObject({ kind: "misconfigured" });
    expect(invalidInputFake.cursor.index).toBe(0);

    const malformedFake = { outcomes: [{ kind: "accepted", value: { accepted_at: 42 } }], cursor: { index: 0 }, elapsedMs: 0 };
    await expect(testAdapter.invokeReferenceMail({ idempotency_key: "00000000-0000-4000-8000-000000000001" }, canary, context(), malformedFake))
      .rejects.toMatchObject({ kind: "outcome_unknown" });
    expect(malformedFake.cursor.index).toBe(1);

    const exhaustedFake = { outcomes: [], cursor: { index: 0 }, elapsedMs: 0 };
    await expect(testAdapter.invokeReferenceMail({ idempotency_key: "00000000-0000-4000-8000-000000000002" }, canary, context(), exhaustedFake))
      .rejects.toMatchObject({ kind: "outcome_unknown" });
    expect(exhaustedFake.cursor.index).toBe(1);
  } finally {
    Bun.connect = originalConnect;
  }
  expect(connects).toBe(0);
});

test("route deadlines cap no-effect service retries and preserve uncertain dispatch", async () => {
  const deadlineContext = (): any => {
    const monotonicStartedAt = performance.now();
    return {
      operationId: "service-fake-route-deadline",
      monotonicStartedAt,
      deadlineAt: monotonicStartedAt + 5,
      serviceAttemptBudget: { attemptsUsed: 0 },
      signal: null,
    };
  };

  const noEffect = { outcomes: [{ kind: "declared", name: "transport.pre_dispatch_timeout" }], cursor: { index: 0 }, elapsedMs: 0 };
  await expect(testAdapter.invokeReferenceMail(
    { idempotency_key: "00000000-0000-4000-8000-000000000003" },
    canary,
    deadlineContext(),
    noEffect,
  )).rejects.toMatchObject({ kind: "deadline_exceeded" });
  expect(noEffect.cursor.index).toBe(1);

  const uncertain = { outcomes: [{ kind: "declared", name: "transport.possible_dispatch_timeout" }], cursor: { index: 0 }, elapsedMs: 0 };
  await expect(testAdapter.invokeReferenceMail(
    { idempotency_key: "00000000-0000-4000-8000-000000000004" },
    canary,
    deadlineContext(),
    uncertain,
  )).rejects.toMatchObject({ kind: "outcome_unknown" });
  expect(uncertain.cursor.index).toBe(1);
});

async function buildVariant(name: string, contents: string) {
  const directory = join(root, name);
  mkdirSync(join(directory, "tests/assurance"), { recursive: true });
  cpSync(resolve("tests/assurance/service-reference-mail-v0.1.json"), join(directory, "tests/assurance/service-reference-mail-v0.1.json"));
  writeFileSync(join(directory, "app.jadpo"), contents);
  const result = Bun.spawnSync([compiler, "build", directory], { stdout: "pipe", stderr: "pipe" });
  expect(result.exitCode, `${result.stdout}\n${result.stderr}`).toBe(0);
  return import(pathToFileURL(join(directory, "build/target/app.ts")).href);
}

test("empty checked authored service fake fails closed without connections or fetch", async () => {
  const empty = sourceText.replace(/(fixture service_accepted \{[\s\S]*?service ReminderMail: fake \{)[\s\S]*?(\n    \}\n\})/, "$1$2");
  expect(empty).not.toBe(sourceText);
  const variant = await buildVariant("empty-fake", empty);
  let egress = 0;
  const originalConnect = Bun.connect;
  const originalFetch = globalThis.fetch;
  Bun.connect = (() => { egress++; throw new Error("blocked empty-fake connection"); }) as typeof Bun.connect;
  globalThis.fetch = (async () => { egress++; throw new Error("blocked empty-fake fetch"); }) as typeof fetch;
  try {
    const report = await variant.runTests();
    expect(report.summary).toEqual({ total: 6, passed: 5, failed: 1 });
    expect(egress).toBe(0);
    expect(JSON.stringify(report)).not.toContain("fixture-only-provider-key");
  } finally { Bun.connect = originalConnect; globalThis.fetch = originalFetch; }
});

test("fresh callable operations reset fake elapsed budgets but preserve the sequence", async () => {
  const sequential = sourceText
    .replace(/(fixture service_timeout \{[\s\S]*?service ReminderMail: fake \{)([\s\S]*?)(\n    \}\n\})/, (_all, prefix, body, suffix) => prefix + body + body + '\n        send_overdue_reminder => accept ReminderReceipt { accepted_at: Instant("2026-01-15T12:00:00Z") }' + suffix)
    .replace('var receipt = call classify_delivery(mail)\n    assert receipt.accepted_at == Instant("2000-01-04T00:00:00Z")', 'var first = call classify_delivery(mail)\n    var second = call classify_delivery(mail)\n    var receipt = call classify_delivery(mail)\n    assert receipt.accepted_at == Instant("2026-01-15T12:00:00Z")');
  expect(sequential).not.toBe(sourceText);
  const variant = await buildVariant("fresh-operations", sequential);
  let egress = 0;
  const originalConnect = Bun.connect;
  const originalFetch = globalThis.fetch;
  Bun.connect = (() => { egress++; throw new Error("blocked sequential-fake connection"); }) as typeof Bun.connect;
  globalThis.fetch = (async () => { egress++; throw new Error("blocked sequential-fake fetch"); }) as typeof fetch;
  try {
    expect((await variant.runTests()).summary).toEqual({ total: 6, passed: 6, failed: 0 });
    expect(egress).toBe(0);
  } finally { Bun.connect = originalConnect; globalThis.fetch = originalFetch; }
});

test("fake elapsed time and attempt allowance remain shared by nested calls", async () => {
  const context = { operationId: "nested-fake-budget", monotonicStartedAt: performance.now(), serviceAttemptBudget: { attemptsUsed: 0 }, signal: null };
  const fake = { outcomes: Array.from({ length: 4 }, () => ({ kind: "accepted", value: { accepted_at: "2026-01-15T12:00:00Z" } })), cursor: { index: 0 }, elapsedMs: 10000 };
  const input = { idempotency_key: "00000000-0000-4000-8000-000000000005" };
  for (let call = 0; call < 3; call++) await testAdapter.invokeReferenceMail(input, canary, context, fake);
  await expect(testAdapter.invokeReferenceMail(input, canary, context, fake)).rejects.toMatchObject({ kind: "unavailable" });
  expect(fake.cursor.index).toBe(3);
  expect(context.serviceAttemptBudget.attemptsUsed).toBe(3);

  const deadlineContext = { ...context, serviceAttemptBudget: { attemptsUsed: 0 } };
  const deadlineFake = { ...fake, cursor: { index: 0 } };
  await testAdapter.invokeReferenceMail(input, canary, deadlineContext, deadlineFake);
  deadlineFake.elapsedMs += 30000;
  await expect(testAdapter.invokeReferenceMail(input, canary, deadlineContext, deadlineFake)).rejects.toMatchObject({ kind: "unavailable" });
  expect(deadlineFake.cursor.index).toBe(1);
  expect(deadlineContext.serviceAttemptBudget.attemptsUsed).toBe(1);
});
