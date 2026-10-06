import { afterAll, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { createReadinessGate } from "../../jadpo/crates/core/src/runtime/readiness_gate.ts";

const generatedApplication = join(
  import.meta.dir,
  "../../examples/persistence-seed/build/target/app.ts",
);
const generatedAuthApplication = join(
  import.meta.dir,
  "../../examples/first-party-authentication/build/target/app.ts",
);
const authReadinessHarness = join(import.meta.dir, "fixtures/readiness-auth-server.ts");
// The controlled evaluators strip module imports. Supply the real emitted
// helper in this imported/library context; native supervision is tested by the
// separate actual startup-ownership suite.
const { initialNativeStartupDeadline } = await import(pathToFileURL(join(dirname(generatedApplication), "startup-owner.ts")).href);

const root = mkdtempSync(join(tmpdir(), "jadpo-readiness-"));
const databaseDirectory = join(root, "recovered");


function reserveDeploymentPort(applicationPort: number): number {
  for (let attempt = 0; attempt < 3; attempt++) { const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() }); const port = server.port; server.stop(true); if (port !== applicationPort) return port; }
  throw Error("Could not reserve a separate deployment port");
}

function controlledReadiness(initialReady = true) {
  const state = { ready: initialReady, fatal: false, needed: false, generation: 0,
    checks: 0, prepares: 0, publications: 0, failures: 0, cancels: 0, timers: 0, now: 0, deadlines: [] as number[] };
  const scheduled = new Map<number, { at: number; callback: () => void }>();
  let work: () => Promise<boolean> = () => Promise.resolve(true);
  let prepare = () => {};
  let cancel = () => {};
  const gate = createReadinessGate({
    ready: () => state.ready, fatal: () => state.fatal, needsProbe: () => state.needed,
    invalidation: () => state.generation, now: () => state.now,
    schedule: (callback, milliseconds) => {
      const id = ++state.timers; scheduled.set(id, { at: state.now + milliseconds, callback }); return id;
    },
    clear: timer => { scheduled.delete(timer as number); },
    check: (_recovering, setCancel, deadlineAt) => {
      state.deadlines.push(deadlineAt); state.checks++; setCancel(() => { state.cancels++; cancel(); }); return work();
    },
    prepare: recovering => { state.prepares++; prepare(); return recovering; },
    publish: () => { state.publications++; state.ready = true; state.needed = false; },
    unavailable: () => { state.failures++; state.ready = false; },
  });
  return { gate, state, scheduled,
    work: (callback: () => Promise<boolean>) => { work = callback; },
    prepare: (callback: () => void) => { prepare = callback; },
    cancel: (callback: () => void) => { cancel = callback; },
    advance: (milliseconds: number, fire = true) => {
      state.now += milliseconds;
      if (fire) for (const [id, timer] of [...scheduled]) if (timer.at <= state.now) {
        scheduled.delete(id); timer.callback();
      }
    },
  };
}

function deferredReadiness() {
  let resolve!: (value: boolean) => void, reject!: (cause: unknown) => void;
  const promise = new Promise<boolean>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe("compiler-private bounded readiness response", () => {
  test("the generated persistence includes the exact controller and stages recovery publication", () => {
    const generated = readFileSync(join(dirname(generatedApplication), "persistence.ts"), "utf8");
    const source = readFileSync(join(import.meta.dir, "../../jadpo/crates/core/src/runtime/readiness_gate.ts"), "utf8");
    expect(generated).toContain(source);
    expect(generated).toContain("async function initializePersistenceOnce(publish = true, setCancel?");
    expect(generated).toContain("if (publish) { persistenceReady = true;");
    expect(generated).toContain("recovering ? initializePersistenceOnce(false, setCancel, deadlineAt)");
    expect(generated).toContain("persistenceInvalidationGeneration++");
    expect(generated).toContain("export function refreshPersistenceReadiness(): Promise<boolean> { return readinessGate.refresh(); }");
    expect(generated).not.toContain("const timer = setTimeout(() => query.cancel()");
  });

  test("stalled probe and initialization share one finite response and retain one native observer", async () => {
    for (const initialReady of [true, false]) {
      const h = controlledReadiness(initialReady), native = deferredReadiness();
      let observers = 0;
      const then = native.promise.then.bind(native.promise);
      native.promise.then = ((...args: any[]) => { observers++; return (then as any)(...args); }) as typeof native.promise.then;
      h.work(() => native.promise);
      const callers = Array.from({ length: 32 }, () => h.gate.refresh());
      expect(callers.every(response => response === callers[0])).toBe(true);
      expect(h.state.checks).toBe(1); expect(observers).toBe(1); expect(h.state.timers).toBe(1); expect(h.state.deadlines).toEqual([1000]);
      h.advance(1000);
      expect(await Promise.all(callers)).toEqual(Array(32).fill(false));
      expect(h.state.ready).toBe(false); expect(h.state.failures).toBe(1); expect(h.state.cancels).toBe(1);
      h.advance(60000);
      expect(await Promise.all(Array.from({ length: 128 }, () => h.gate.refresh()))).toEqual(Array(128).fill(false));
      expect(h.state.checks).toBe(1); expect(observers).toBe(1); expect(h.state.timers).toBe(1); expect(h.state.deadlines).toEqual([1000]);
      native.resolve(true); await Promise.resolve();
      expect(h.state.publications).toBe(0); expect(h.state.prepares).toBe(0); expect(h.state.failures).toBe(1);
      h.work(() => Promise.resolve(true));
      expect(await h.gate.refresh()).toBe(true);
      expect(h.state.checks).toBe(2); expect(h.state.publications).toBe(1);
    }
  });

  test("publication rejects exact or late deadlines even when the timer has not fired", async () => {
    for (const elapsed of [999, 1000, 1001]) {
      const h = controlledReadiness(false), native = deferredReadiness();
      h.work(() => native.promise);
      const response = h.gate.refresh();
      h.advance(elapsed, false); native.resolve(true);
      expect(await response).toBe(elapsed < 1000);
      expect(h.state.publications).toBe(elapsed < 1000 ? 1 : 0);
      expect(h.scheduled.size).toBe(0);
    }
    const h = controlledReadiness(false);
    h.prepare(() => { h.advance(1000, false); });
    expect(await h.gate.refresh()).toBe(false);
    expect(h.state.prepares).toBe(1); expect(h.state.publications).toBe(0);
  });

  test("a newer availability failure cannot be erased by an older successful probe", async () => {
    const h = controlledReadiness(), native = deferredReadiness();
    h.work(() => native.promise);
    const response = h.gate.refresh();
    h.state.generation++; h.state.needed = true;
    native.resolve(true);
    expect(await response).toBe(false); expect(h.state.needed).toBe(true); expect(h.state.publications).toBe(0);
    h.advance(250); h.work(() => Promise.resolve(true));
    expect(await h.gate.refresh()).toBe(true); expect(h.state.needed).toBe(false);
    // A failure during candidate preparation is fenced too.
    h.advance(1000); h.prepare(() => { h.state.generation++; h.state.needed = true; });
    expect(await h.gate.refresh()).toBe(false); expect(h.state.publications).toBe(1);
  });

  test("backoff is once per failure, capped, and only a fresh fenced success resets it", async () => {
    const h = controlledReadiness(false);
    h.work(() => Promise.resolve(false));
    for (const delay of [250, 500, 1000, 2000, 4000, 5000, 5000]) {
      const previous = h.state.checks;
      expect(await h.gate.refresh()).toBe(false); expect(h.state.checks).toBe(previous + 1);
      h.advance(delay - 1);
      expect(await h.gate.refresh()).toBe(false); expect(h.state.checks).toBe(previous + 1);
      h.advance(1);
    }
    h.work(() => Promise.resolve(true)); expect(await h.gate.refresh()).toBe(true);
    const checks = h.state.checks;
    h.advance(999); expect(await h.gate.refresh()).toBe(true); expect(h.state.checks).toBe(checks);
    h.advance(1); h.work(() => Promise.resolve(false)); expect(await h.gate.refresh()).toBe(false);
    h.advance(249); expect(await h.gate.refresh()).toBe(false); expect(h.state.checks).toBe(checks + 1);
    h.advance(1); expect(await h.gate.refresh()).toBe(false); expect(h.state.checks).toBe(checks + 2);
  });

  test("throwing cancellation, late rejection and genuine fatal initialization stay contained", async () => {
    for (const fatal of [false, true]) {
      const h = controlledReadiness(false), native = deferredReadiness();
      h.cancel(() => { throw new Error("cancel unavailable"); });
      // Models the aggregate initializer's preserved genuine failure classifier.
      h.work(() => native.promise.catch(() => { h.state.fatal = fatal; return false; }));
      const response = h.gate.refresh(); h.advance(1000);
      expect(await response).toBe(false); expect(h.state.fatal).toBe(false);
      native.reject(new Error("native failure")); await Promise.resolve(); await Promise.resolve();
      expect(h.state.failures).toBe(1); expect(h.state.publications).toBe(0); expect(h.state.fatal).toBe(fatal);
      h.advance(6000); h.work(() => Promise.resolve(true));
      expect(await h.gate.refresh()).toBe(!fatal); expect(h.state.checks).toBe(fatal ? 1 : 2);
    }
    const h = controlledReadiness(false);
    h.work(() => { throw new Error("synchronous native setup failure"); });
    expect(await h.gate.refresh()).toBe(false); expect(h.scheduled.size).toBe(0);
    h.advance(250); h.work(() => Promise.resolve(true)); expect(await h.gate.refresh()).toBe(true);
  });
});

afterAll(() => {
  rmSync(root, { recursive: true, force: true });
});

describe("generated runtime readiness", () => {
  test("retry-cutoff requests can observe mixed readiness while eligible requests share one initialization", async () => {
    const directory = join(root, "cutoff-output");
    cpSync(dirname(generatedApplication), directory, { recursive: true });
    const databasePath = join(root, "cutoff-database", "app.sqlite");
    const previousUrl = Bun.env.DATABASE_URL, previousPath = Bun.env.SQLITE_PATH;
    const originalNow = performance.now, originalExec = Database.prototype.exec;
    let now = 0, initializations = 0;
    delete Bun.env.DATABASE_URL; Bun.env.SQLITE_PATH = databasePath;
    performance.now = () => now;
    Database.prototype.exec = function (statement: string) {
      if (statement.includes('CREATE TABLE IF NOT EXISTS "customer"')) initializations++;
      return originalExec.call(this, statement);
    };
    try {
      const persistence = await import(pathToFileURL(join(directory, "persistence.ts")).href);
      expect(await persistence.refreshPersistenceReadiness()).toBe(false);
      mkdirSync(dirname(databasePath), { recursive: true });
      now = 249;
      const before = Array.from({ length: 16 }, () => persistence.refreshPersistenceReadiness());
      now = 250;
      const eligible = Array.from({ length: 16 }, () => persistence.refreshPersistenceReadiness());
      expect(await Promise.all(before)).toEqual(Array(16).fill(false));
      expect(await Promise.all(eligible)).toEqual(Array(16).fill(true));
      expect(initializations).toBe(1);
      expect(persistence.isPersistenceReady()).toBe(true);
    } finally {
      performance.now = originalNow; Database.prototype.exec = originalExec;
      if (previousUrl === undefined) delete Bun.env.DATABASE_URL; else Bun.env.DATABASE_URL = previousUrl;
      if (previousPath === undefined) delete Bun.env.SQLITE_PATH; else Bun.env.SQLITE_PATH = previousPath;
    }
  });

  test("initial and recovery-time SQLite schema locks stay recoverable after contention clears", async () => {
    const previousUrl = Bun.env.DATABASE_URL, previousPath = Bun.env.SQLITE_PATH;
    const originalNow = performance.now;
    delete Bun.env.DATABASE_URL;
    try {
      for (const initialLock of [true, false]) {
        let now = 0;
        performance.now = () => now;
        const name = initialLock ? "initial-lock" : "recovery-lock";
        const directory = join(root, name + "-output");
        cpSync(dirname(generatedApplication), directory, { recursive: true });
        const databasePath = join(root, name + "-database", "app.sqlite");
        Bun.env.SQLITE_PATH = databasePath;
        let blocker: Database | null = null;
        if (initialLock) {
          mkdirSync(dirname(databasePath), { recursive: true });
          blocker = new Database(databasePath, { create: true, strict: true });
          blocker.exec("BEGIN EXCLUSIVE");
        }
        const persistence = await import(pathToFileURL(join(directory, "persistence.ts")).href);
        try {
          expect(persistence.isPersistenceInitializationFatal()).toBe(false);
          expect(await persistence.refreshPersistenceReadiness()).toBe(false);
          if (!initialLock) {
            mkdirSync(dirname(databasePath), { recursive: true });
            blocker = new Database(databasePath, { create: true, strict: true });
            blocker.exec("BEGIN EXCLUSIVE");
            now = 250;
            expect(await persistence.refreshPersistenceReadiness()).toBe(false);
          }
          expect(persistence.isPersistenceInitializationFatal()).toBe(false);
          blocker!.exec("ROLLBACK");
          now = 1000;
          expect(await persistence.refreshPersistenceReadiness()).toBe(true);
          expect(persistence.isPersistenceInitializationFatal()).toBe(false);
        } finally { if (blocker?.inTransaction) blocker.exec("ROLLBACK"); blocker?.close(); }
      }
    } finally {
      performance.now = originalNow;
      if (previousUrl === undefined) delete Bun.env.DATABASE_URL; else Bun.env.DATABASE_URL = previousUrl;
      if (previousPath === undefined) delete Bun.env.SQLITE_PATH; else Bun.env.SQLITE_PATH = previousPath;
    }
  });

  test("recovers required storage and authenticated reads through generated handlers without binding a listener", async () => {
    const databasePath = join(databaseDirectory, "direct-auth", "app.sqlite");
    const environmentKeys = [
      "SQLITE_PATH",
      "DATABASE_URL",
      "AUTH_SIGNING_KEY",
      "AUTH_PREVIOUS_SIGNING_KEY",
      "BROWSER_ORIGIN",
      "JADPO_DEBUG_TARGET_STACKS",
    ] as const;
    const previousEnvironment = Object.fromEntries(environmentKeys.map(key => [key, Bun.env[key]]));
    Bun.env.SQLITE_PATH = databasePath;
    delete Bun.env.DATABASE_URL;
    Bun.env.AUTH_SIGNING_KEY = Buffer.alloc(32, 7).toString("base64url");
    Bun.env.AUTH_PREVIOUS_SIGNING_KEY = Buffer.alloc(32, 8).toString("base64url");
    Bun.env.BROWSER_ORIGIN = "https://example.test";
    Bun.env.JADPO_DEBUG_TARGET_STACKS = "0";
    const databasePrototype = Database.prototype as any;
    const originalExec = databasePrototype.exec;
    let userSchemaInitializations = 0;
    databasePrototype.exec = function (statement: string, ...values: unknown[]) {
      if (statement.includes('CREATE TABLE IF NOT EXISTS "user"')) userSchemaInitializations += 1;
      return originalExec.call(this, statement, ...values);
    };

    try {
      const application = await import(pathToFileURL(generatedAuthApplication).href);
      await application.initializeApplication(Bun.env);
      const readyRequest = () => application.handleDeploymentRequest(new Request("http://jadpo.test/health/ready"));
      const unavailable = await readyRequest();
      expect(unavailable.status).toBe(503);
      expect(await unavailable.json()).toMatchObject({ status: "not_ready", checks: { database: "unavailable" } });

      const gatedIdentity = await application.handleRequest(new Request("http://jadpo.test/identity"));
      expect(gatedIdentity.status).toBe(503);
      expect((await gatedIdentity.json()).error.code).toBe("dependency_unavailable");

      mkdirSync(dirname(databasePath), { recursive: true });
      await Bun.sleep(275);
      let recoveredBatch: Response[] = [];
      const recoveryDeadline = performance.now() + 8000;
      while (performance.now() < recoveryDeadline) {
        const batch = await Promise.all(Array.from({ length: 32 }, () => readyRequest()));
        if (batch.every(response => response.status === 200)) {
          recoveredBatch = batch;
          break;
        }
        for (const response of batch) {
          expect([200, 503]).toContain(response.status);
          expect(await response.json()).toMatchObject({ status: response.status === 200 ? "ready" : "not_ready", checks: { database: response.status === 200 ? "available" : "unavailable" } });
        }
        await Bun.sleep(25);
      }
      expect(recoveredBatch).toHaveLength(32);
      const recoveredBodies = await Promise.all(recoveredBatch.map(response => response.json()));
      expect(recoveredBodies.every(body => body.status === "ready" && body.checks.database === "available")).toBe(true);
      expect(userSchemaInitializations).toBe(1);
      databasePrototype.exec = originalExec;

      const database = new Database(databasePath, { strict: true });
      try {
        database.prepare('INSERT INTO "user" ("id", "authentication_subject", "enabled") VALUES (?, ?, 1)')
          .run("00000000-0000-4000-8000-000000000007", "readiness-user");
      } finally {
        database.close();
      }
      const issued = await application.authenticationHost().issue(
        "api_bearer",
        "readiness-user",
        Date.now() + 60 * 60 * 1000,
        Date.now(),
      );
      const identity = await application.handleRequest(new Request("http://jadpo.test/identity", {
        headers: { authorization: `Bearer ${issued.credential}` },
      }));
      expect(identity.status).toBe(200);
      expect(await identity.json()).toEqual({ id: "00000000-0000-4000-8000-000000000007" });

      const outageDatabase = new Database(databasePath, { strict: true });
      outageDatabase.exec('ALTER TABLE "__jadpo_auth_sessions" RENAME TO "unavailable_sessions"');
      try {
        const duringOutage = await application.handleRequest(new Request("http://jadpo.test/identity", {
          headers: { authorization: `Bearer ${issued.credential}` },
        }));
        expect(duringOutage.status).toBe(503);
        expect((await duringOutage.json()).error.code).toBe("authentication_unavailable");
      } finally {
        outageDatabase.exec('ALTER TABLE "unavailable_sessions" RENAME TO "__jadpo_auth_sessions"');
        outageDatabase.close();
      }
      const afterRecovery = await application.handleRequest(new Request("http://jadpo.test/identity", {
        headers: { authorization: `Bearer ${issued.credential}` },
      }));
      expect(afterRecovery.status).toBe(200);
      expect(await afterRecovery.json()).toEqual({ id: "00000000-0000-4000-8000-000000000007" });
    } finally {
      databasePrototype.exec = originalExec;
      for (const key of environmentKeys) {
        const value = previousEnvironment[key];
        if (value === undefined) delete Bun.env[key];
        else Bun.env[key] = value;
      }
    }
  });

  test("keeps the process live, gates traffic, and recovers when the required database returns", async () => {
    for (const initialLock of [false, true]) {
    const portReservation = Bun.serve({ port: 0, fetch: () => new Response() });
    const port = portReservation.port;
    portReservation.stop(true);
    const deploymentPort = reserveDeploymentPort(port);

    const childDatabaseDirectory = join(databaseDirectory, initialLock ? "locked-process" : "process");
    let blocker: Database | null = null;
    if (initialLock) {
      mkdirSync(childDatabaseDirectory, { recursive: true });
      blocker = new Database(join(childDatabaseDirectory, "app.sqlite"), { create: true, strict: true });
      blocker.exec("BEGIN EXCLUSIVE");
    }
    const childEnvironment = { ...Bun.env, PORT: String(port), JADPO_DEPLOYMENT_PORT: String(deploymentPort), SQLITE_PATH: join(childDatabaseDirectory, "app.sqlite"), JADPO_DEBUG_TARGET_STACKS: "0" };
    delete childEnvironment.DATABASE_URL;
    const child = Bun.spawn([process.execPath, "--no-install", generatedApplication], {
      env: childEnvironment,
      stdout: "pipe",
      stderr: "pipe",
    });

    try {
      const deadline = performance.now() + 8000;
      let live: Response | undefined;
      let unavailable: Response | undefined;
      while (performance.now() < deadline) {
        try {
          live = await fetch(`http://127.0.0.1:${port}/health/live`);
          unavailable = await fetch(`http://127.0.0.1:${deploymentPort}/health/ready`);
          if (live.status === 200 && unavailable.status === 503) break;
        } catch {}
        await Bun.sleep(25);
      }

      expect(live?.status).toBe(200);
      expect(await live?.json()).toEqual({ status: "live" });
      expect(unavailable?.status).toBe(503);
      expect(await unavailable?.json()).toMatchObject({ status: "not_ready", checks: { database: "unavailable" }, advisories: {} });
      expect((await fetch(`http://127.0.0.1:${port}/not-a-route`)).status).toBe(503);
      expect(child.exitCode).toBeNull();

      mkdirSync(childDatabaseDirectory, { recursive: true });
      if (blocker?.inTransaction) blocker.exec("ROLLBACK");
      let recovered: Response | undefined;
      const recoveryDeadline = performance.now() + 8000;
      while (performance.now() < recoveryDeadline) {
        recovered = await fetch(`http://127.0.0.1:${deploymentPort}/health/ready`);
        if (recovered.status === 200) break;
        await Bun.sleep(25);
      }
      expect(recovered?.status).toBe(200);
      expect(await recovered?.json()).toMatchObject({ status: "ready", checks: { database: "available" }, advisories: {} });
      expect((await fetch(`http://127.0.0.1:${port}/not-a-route`)).status).toBe(404);
    } finally {
      if (blocker?.inTransaction) blocker.exec("ROLLBACK");
      blocker?.close();
      child.kill();
      await child.exited;
      const [stdout, stderr] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(stdout).toContain("runtime.live");
      expect(stderr).toContain("RUNTIME_DEPENDENCY_UNAVAILABLE");
      expect(stderr).not.toContain("PersistenceFault");
    }
    }
  });

  test("recovers first-party authentication after its required database returns", async () => {
    const portReservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
    const port = portReservation.port;
    portReservation.stop(true);
    const deploymentPort = reserveDeploymentPort(port);

    const authDirectory = join(root, "first-party-auth-process");
    const databasePath = join(authDirectory, "app.sqlite");
    const signingKey = Buffer.alloc(32, 7).toString("base64url");
    const childEnvironment = {
      ...Bun.env,
      AUTH_SIGNING_KEY: signingKey,
      AUTH_PREVIOUS_SIGNING_KEY: Buffer.alloc(32, 8).toString("base64url"),
      BROWSER_ORIGIN: "https://example.test",
      JADPO_DEBUG_TARGET_STACKS: "0",
      PORT: String(port), JADPO_DEPLOYMENT_PORT: String(deploymentPort),
      SQLITE_PATH: databasePath,
    };
    delete childEnvironment.DATABASE_URL;
    const child = Bun.spawn([process.execPath, "--no-install", authReadinessHarness, generatedAuthApplication], {
      env: childEnvironment,
      stdout: "pipe",
      stderr: "pipe",
    });
    let credential = "";
    try {
      const startupDeadline = performance.now() + 8000;
      let live: Response | undefined;
      let unavailable: Response | undefined;
      while (performance.now() < startupDeadline) {
        try {
          live = await fetch(`http://127.0.0.1:${port}/health/live`);
          unavailable = await fetch(`http://127.0.0.1:${deploymentPort}/health/ready`);
          if (live.status === 200 && unavailable.status === 503) break;
        } catch {}
        await Bun.sleep(25);
      }
      expect(live?.status).toBe(200);
      expect(await live?.json()).toEqual({ status: "live" });
      expect(unavailable?.status).toBe(503);
      expect(await unavailable?.json()).toMatchObject({ status: "not_ready", checks: { database: "unavailable" }, advisories: {} });
      const gated = await fetch(`http://127.0.0.1:${port}/identity`);
      expect(gated.status).toBe(503);
      expect((await gated.json()).error.code).toBe("dependency_unavailable");
      expect(child.exitCode).toBeNull();

      mkdirSync(authDirectory, { recursive: true });
      const recoveryDeadline = performance.now() + 8000;
      let recoveredBatch: Response[] = [];
      while (performance.now() < recoveryDeadline) {
        const batch = await Promise.all(Array.from({ length: 32 }, () => fetch(
          `http://127.0.0.1:${deploymentPort}/health/ready`,
          { signal: AbortSignal.timeout(8000) },
        )));
        if (batch.every(response => response.status === 200)) {
          recoveredBatch = batch;
          break;
        }
        for (const response of batch) {
          expect([200, 503]).toContain(response.status);
          expect(await response.json()).toEqual({ status: response.status === 200 ? "ready" : "not_ready", checks: { database: response.status === 200 ? "available" : "unavailable" }, advisories: {} });
        }
        await Bun.sleep(25);
      }
      expect(recoveredBatch).toHaveLength(32);
      const readinessBodies = await Promise.all(recoveredBatch.map(response => response.json()));
      expect(readinessBodies.every(body => body.status === "ready" && body.checks.database === "available" && Object.keys(body.advisories).length === 0)).toBe(true);
      const readinessStats = await fetch(`http://127.0.0.1:${port}/__test/readiness-stats`);
      expect(await readinessStats.json()).toEqual({ userSchemaInitializations: 1 });

      const issued = await fetch(`http://127.0.0.1:${port}/__test/issue`, { method: "POST" });
      expect(issued.status).toBe(200);
      credential = (await issued.json() as { credential: string }).credential;
      expect(typeof credential).toBe("string");
      const authenticated = await fetch(`http://127.0.0.1:${port}/identity`, {
        headers: { authorization: `Bearer ${credential}` },
      });
      expect(authenticated.status).toBe(200);
      expect(await authenticated.json()).toEqual({ id: "00000000-0000-4000-8000-000000000007" });
      expect(child.exitCode).toBeNull();
    } finally {
      child.kill();
      await child.exited;
      const [stdout, stderr] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(stderr).toContain("RUNTIME_DEPENDENCY_UNAVAILABLE");
      expect(stdout).not.toContain(signingKey);
      expect(stderr).not.toContain(signingKey);
      if (credential !== "") {
        expect(stdout).not.toContain(credential);
        expect(stderr).not.toContain(credential);
      }
    }
  });

  if (Bun.env.JADPO_READINESS_AUTH_DATABASE_URL) {
    test("recovers PostgreSQL authentication sessions after a post-startup authority-store outage without restart", async () => {
      const databaseUrl = Bun.env.JADPO_READINESS_AUTH_DATABASE_URL!;
      const database = new SQL(databaseUrl, { prepare: false });
      const portReservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
      const port = portReservation.port;
      portReservation.stop(true);
      const deploymentPort = reserveDeploymentPort(port);
      const signingKey = Buffer.alloc(32, 7).toString("base64url");
      const previousSigningKey = Buffer.alloc(32, 8).toString("base64url");
      const child = Bun.spawn([process.execPath, "--no-install", authReadinessHarness, generatedAuthApplication], {
        env: { ...Bun.env, DATABASE_URL: databaseUrl, AUTH_SIGNING_KEY: signingKey,
          AUTH_PREVIOUS_SIGNING_KEY: previousSigningKey, BROWSER_ORIGIN: "https://example.test",
          PORT: String(port), JADPO_DEPLOYMENT_PORT: String(deploymentPort), SQLITE_PATH: join(root, "postgres-auth-unused.sqlite"), JADPO_DEBUG_TARGET_STACKS: "0" },
        stdout: "pipe", stderr: "pipe",
      });
      const request = (path: string, init: RequestInit = {}) => fetch(`http://127.0.0.1:${path === "/health/ready" ? deploymentPort : port}${path}`,
        { ...init, signal: AbortSignal.timeout(8000) });
      let renamed = false, credential = "";
      try {
        const deadline = performance.now() + 8000;
        let ready: Response | undefined;
        while (performance.now() < deadline) {
          try { ready = await request("/health/ready"); if (ready.status === 200) break; } catch {}
          await Bun.sleep(25);
        }
        expect(ready?.status).toBe(200);
        expect(await ready?.json()).toEqual({ status: "ready", checks: { database: "available" }, advisories: {} });
        const issued = await request("/__test/issue", { method: "POST" });
        expect(issued.status).toBe(200);
        credential = (await issued.json() as { credential: string }).credential;
        expect(typeof credential).toBe("string");
        expect(credential.length).toBeGreaterThan(0);
        const headers = { authorization: `Bearer ${credential}` };
        const authenticated = await request("/identity", { headers });
        expect(authenticated.status).toBe(200);
        expect(await authenticated.json()).toEqual({ id: "00000000-0000-4000-8000-000000000007" });

        await database.unsafe('ALTER TABLE "__jadpo_auth_sessions" RENAME TO "unavailable_auth_sessions"');
        renamed = true;
        const unavailable = await request("/identity", { headers });
        expect(unavailable.status).toBe(503);
        const unavailableBody = await unavailable.text();
        expect(JSON.parse(unavailableBody).error.code).toBe("authentication_unavailable");
        for (const secret of [credential, signingKey, previousSigningKey, databaseUrl, "__jadpo_auth_sessions", "SELECT", "42P01"])
          expect(unavailableBody).not.toContain(secret);
        const live = await request("/health/live");
        expect(live.status).toBe(200);
        expect(await live.json()).toEqual({ status: "live" });
        // A healthy connection ping is not proof that private auth tables exist.
        const ping = await request("/health/ready");
        expect(ping.status).toBe(200);
        expect(await ping.json()).toEqual({ status: "ready", checks: { database: "available" }, advisories: {} });
        expect(child.exitCode).toBeNull();

        await database.unsafe('ALTER TABLE "unavailable_auth_sessions" RENAME TO "__jadpo_auth_sessions"');
        renamed = false;
        const recovered = await request("/identity", { headers });
        expect(recovered.status).toBe(200);
        expect(await recovered.json()).toEqual({ id: "00000000-0000-4000-8000-000000000007" });
        expect(child.exitCode).toBeNull();
      } finally {
        try {
          if (renamed) await database.unsafe('ALTER TABLE "unavailable_auth_sessions" RENAME TO "__jadpo_auth_sessions"');
        } finally {
          child.kill();
          await child.exited;
          await database.close();
        }
        const [stdout, stderr] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
        for (const secret of [signingKey, previousSigningKey, databaseUrl, ...(credential ? [credential] : [])]) {
          expect(stdout).not.toContain(secret);
          expect(stderr).not.toContain(secret);
        }
        expect(stderr).not.toContain("42P01");
        expect(stderr).not.toContain("SELECT");
      }
    });
  }
  if (Bun.env.JADPO_READINESS_RECOVERY_DATABASE_URL && Bun.env.JADPO_READINESS_CONTROL_DATABASE_URL) {
    test("recovers concurrent PostgreSQL readiness and the same credential after connection loss without restarting", async () => {
      // Only the disposable-cluster wrapper supplies these isolated databases.
      // Refuse database-level fault injection unless the exact local pair matches.
      const databaseUrl = Bun.env.JADPO_READINESS_RECOVERY_DATABASE_URL!;
      const controlUrl = Bun.env.JADPO_READINESS_CONTROL_DATABASE_URL!;
      const applicationDatabase = new URL(databaseUrl), controlDatabase = new URL(controlUrl);
      expect(applicationDatabase.hostname).toBe("127.0.0.1");
      expect(applicationDatabase.pathname).toBe("/jadpo_readiness_recovery_runtime");
      expect(applicationDatabase.username).toBe("jadpo_auth_test");
      expect(applicationDatabase.protocol).toBe("postgres:");
      expect(applicationDatabase.port.length).toBeGreaterThan(0);
      expect(controlDatabase.protocol).toBe(applicationDatabase.protocol);
      expect(controlDatabase.hostname).toBe(applicationDatabase.hostname);
      expect(controlDatabase.port).toBe(applicationDatabase.port);
      expect(controlDatabase.username).toBe(applicationDatabase.username);
      expect(controlDatabase.pathname).toBe("/postgres");
      const control = new SQL(controlUrl, { prepare: false });
      const reservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
      const port = reservation.port;
      reservation.stop(true);
      const deploymentPort = reserveDeploymentPort(port);
      const signingKey = Buffer.alloc(32, 17).toString("base64url");
      const previousSigningKey = Buffer.alloc(32, 18).toString("base64url");
      const child = Bun.spawn([process.execPath, "--no-install", authReadinessHarness, generatedAuthApplication], {
        env: { ...Bun.env, DATABASE_URL: databaseUrl, AUTH_SIGNING_KEY: signingKey,
          AUTH_PREVIOUS_SIGNING_KEY: previousSigningKey, BROWSER_ORIGIN: "https://example.test",
          PORT: String(port), JADPO_DEPLOYMENT_PORT: String(deploymentPort), SQLITE_PATH: join(root, "postgres-recovery-unused.sqlite"), JADPO_DEBUG_TARGET_STACKS: "0" },
        stdout: "pipe", stderr: "pipe",
      });
      const request = (path: string, init: RequestInit = {}) => fetch(`http://127.0.0.1:${path === "/health/ready" ? deploymentPort : port}${path}`,
        { ...init, signal: AbortSignal.timeout(8000) });
      let connectionsDisabled = false, credential = "";
      const assertSafe = (body: string) => {
        for (const secret of [signingKey, previousSigningKey, databaseUrl, controlUrl,
          "jadpo_readiness_recovery_runtime", "57P01", "55000", "SELECT", ...(credential ? [credential] : [])])
          expect(body).not.toContain(secret);
      };
      try {
        let ready: Response | undefined;
        const startupDeadline = performance.now() + 8000;
        while (performance.now() < startupDeadline) {
          try { ready = await request("/health/ready"); if (ready.status === 200) break; } catch {}
          await Bun.sleep(25);
        }
        expect(ready?.status).toBe(200);
        const issued = await request("/__test/issue", { method: "POST" });
        expect(issued.status).toBe(200);
        credential = (await issued.json() as { credential: string }).credential;
        expect(credential.length).toBeGreaterThan(0);
        const headers = { authorization: `Bearer ${credential}` };
        expect((await request("/identity", { headers })).status).toBe(200);

        await control.unsafe('ALTER DATABASE "jadpo_readiness_recovery_runtime" ALLOW_CONNECTIONS false');
        connectionsDisabled = true;
        const terminated = await control.unsafe("SELECT pg_terminate_backend(pid) AS terminated FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()",
          ["jadpo_readiness_recovery_runtime"]);
        expect(terminated.length).toBeGreaterThan(0);
        expect(terminated.every(row => row.terminated === true)).toBe(true);
        // Observe cache expiry and actual failure; this is not a hard deadline claim.
        let unavailable: Response | undefined;
        const outageDeadline = performance.now() + 8000;
        while (performance.now() < outageDeadline) {
          unavailable = await request("/health/ready");
          if (unavailable.status === 503) break;
          expect(unavailable.status).toBe(200);
          await Bun.sleep(25);
        }
        expect(unavailable?.status).toBe(503);
        const unavailableBody = await unavailable!.text();
        expect(JSON.parse(unavailableBody)).toEqual({ status: "not_ready", checks: { database: "unavailable" }, advisories: {} });
        assertSafe(unavailableBody);
        const outageBatch = await Promise.all(Array.from({ length: 32 }, () => request("/health/ready")));
        for (const response of outageBatch) {
          expect(response.status).toBe(503);
          const body = await response.text();
          expect(JSON.parse(body)).toEqual({ status: "not_ready", checks: { database: "unavailable" }, advisories: {} });
          assertSafe(body);
        }
        const gated = await request("/identity", { headers });
        expect(gated.status).toBe(503);
        const gatedBody = await gated.text();
        expect(JSON.parse(gatedBody).error.code).toBe("dependency_unavailable");
        assertSafe(gatedBody);
        const live = await request("/health/live");
        expect(live.status).toBe(200);
        expect(await live.json()).toEqual({ status: "live" });
        expect(child.exitCode).toBeNull();

        await control.unsafe('ALTER DATABASE "jadpo_readiness_recovery_runtime" ALLOW_CONNECTIONS true');
        connectionsDisabled = false;
        let recoveredBatch: Response[] = [];
        const recoveryDeadline = performance.now() + 8000;
        while (performance.now() < recoveryDeadline) {
          const batch = await Promise.all(Array.from({ length: 32 }, () => request("/health/ready")));
          const allReady = batch.every(response => response.status === 200);
          for (const response of batch) {
            expect([200, 503]).toContain(response.status);
            const body = await response.text();
            expect(JSON.parse(body)).toEqual({ status: response.status === 200 ? "ready" : "not_ready",
              checks: { database: response.status === 200 ? "available" : "unavailable" }, advisories: {} });
            assertSafe(body);
          }
          if (allReady) { recoveredBatch = batch; break; }
          await Bun.sleep(25);
        }
        expect(recoveredBatch).toHaveLength(32);
        const recovered = await request("/identity", { headers });
        expect(recovered.status).toBe(200);
        expect(await recovered.json()).toEqual({ id: "00000000-0000-4000-8000-000000000007" });
        expect(child.exitCode).toBeNull();
      } finally {
        try {
          if (connectionsDisabled) await control.unsafe('ALTER DATABASE "jadpo_readiness_recovery_runtime" ALLOW_CONNECTIONS true');
        } finally {
          child.kill();
          await child.exited;
          await control.close();
        }
        const [stdout, stderr] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
        assertSafe(stdout); assertSafe(stderr);
      }
    }, 30000);
  }
  if (Bun.env.JADPO_READINESS_TEST_DATABASE_URL) {
    test("runs the readiness ping against the disposable PostgreSQL adapter", async () => {
      const portReservation = Bun.serve({ port: 0, fetch: () => new Response() });
      const port = portReservation.port;
      portReservation.stop(true);
      const deploymentPort = reserveDeploymentPort(port);
      const childEnvironment = {
        ...Bun.env,
        DATABASE_URL: Bun.env.JADPO_READINESS_TEST_DATABASE_URL!,
        PORT: String(port), JADPO_DEPLOYMENT_PORT: String(deploymentPort),
        SQLITE_PATH: join(root, "postgres-unused.sqlite"),
        JADPO_DEBUG_TARGET_STACKS: "0",
      };
      const child = Bun.spawn([process.execPath, "--no-install", generatedApplication], {
        env: childEnvironment,
        stdout: "pipe",
        stderr: "pipe",
      });
      try {
        const deadline = performance.now() + 8000;
        let ready: Response | undefined;
        while (performance.now() < deadline) {
          try {
            ready = await fetch(`http://127.0.0.1:${deploymentPort}/health/ready`);
            if (ready.status === 200) break;
          } catch {}
          await Bun.sleep(25);
        }
        expect(ready?.status).toBe(200);
        expect(await ready?.json()).toMatchObject({ status: "ready", checks: { database: "available" }, advisories: {} });
        expect(child.exitCode).toBeNull();
      } finally {
        child.kill();
        await child.exited;
        const [stdout, stderr] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
        expect(stdout).not.toContain(Bun.env.JADPO_READINESS_TEST_DATABASE_URL!);
        expect(stderr).not.toContain(Bun.env.JADPO_READINESS_TEST_DATABASE_URL!);
      }
    });
  }
});

describe("actual emitted aggregate initialization cancellation", () => {
  for (const fixture of [generatedApplication, generatedAuthApplication]) {
    for (const disposition of ["late_success", "cancelled_57014", "late_schema_failure", "unknown_57014", "throwing_cancel"] as const) {
      test(`${fixture.includes("first-party") ? "authentication" : "persistence"}: ${disposition}`, async () => {
        let now = 0, recovering = false, starts = 0, cancels = 0;
        let settle!: (rows: unknown[]) => void, reject!: (error: unknown) => void;
        const timers = new Map<number, () => void>(); let sequence = 0;
        const sql = ((parts: TemplateStringsArray) => query(parts.join("?"))) as any;
        sql.unsafe = (statement: string) => query(statement);
        sql.begin = async (work: (tx: unknown) => Promise<unknown>) => work(sql);
        const chosen = fixture.includes("first-party") ? 'CREATE TABLE IF NOT EXISTS "__jadpo_auth_sessions"' : 'CREATE INDEX IF NOT EXISTS "account_owner_email_idx"';
        const executed: string[] = [];
        function query(statement: string) {
          const blocked = recovering && statement.includes(chosen);
          const promise = blocked ? new Promise<unknown[]>((yes, no) => { settle = yes; reject = no; }) : Promise.resolve([]);
          return Object.assign(promise, {
            execute() { starts++; executed.push(statement); return this; },
            cancel() { cancels++; if (disposition === "throwing_cancel") throw new Error("native cancel unavailable"); return this; },
          });
        }
        function SQL() { return sql; }
        let source = readFileSync(join(dirname(fixture), "persistence.ts"), "utf8")
          .replace(/^import .+;\n/gm, "")
          .replace(/\bexport /gu, "")
          .replaceAll("import.meta.url", JSON.stringify(pathToFileURL(join(dirname(fixture), "persistence.ts")).href));
        const javascript = new Bun.Transpiler({ loader: "ts" }).transformSync(source) + '\nreturn {refresh: refreshPersistenceReadiness, state: () => ({ ready: persistenceReady, fatal: persistenceInitializationFatal, client: activePersistenceClient }), unavailable: () => { persistenceReady = false; persistenceProbeNeeded = true; }, unknownError: () => Object.assign(new OutcomeUnknownFault("schema.fixture"), {code: "57014"})};';
        const AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
        const emitted = await new AsyncFunction("initialNativeStartupDeadline", "SQL", "Database", "Bun", "performance", "setTimeout", "clearTimeout", "console", javascript)(
          initialNativeStartupDeadline, SQL, Database, { env: { DATABASE_URL: "postgres://fixture.invalid/never_opened" } }, { now: () => now },
          (callback: () => void) => { const id = ++sequence; timers.set(id, callback); return id; },
          (id: number) => timers.delete(id), { error() {} },
        );
        const originalClient = emitted.state().client;
        expect(emitted.state().ready).toBe(true);
        const recoveryStartsAt = executed.length; recovering = true; emitted.unavailable();
        const responses = Array.from({ length: 32 }, () => emitted.refresh());
        expect(responses.every(value => value === responses[0])).toBe(true);
        for (let i = 0; i < 200 && !executed.slice(recoveryStartsAt).some(statement => statement.includes(chosen)); i++) await Promise.resolve();
        expect(typeof settle).toBe("function");
        expect(executed.slice(recoveryStartsAt).some(statement => statement.includes(chosen))).toBe(true); // Actual native dispatch, not lazy query construction.
        const startsAtExpiry = starts;
        now = 1000; for (const callback of [...timers.values()]) callback();
        expect(await Promise.all(responses)).toEqual(Array(32).fill(false));
        expect(cancels).toBe(1); expect(emitted.state().ready).toBe(false);
        expect(emitted.state().fatal).toBe(false); expect(emitted.state().client).toBe(originalClient);
        now = 60000;
        expect(await Promise.all(Array.from({ length: 128 }, () => emitted.refresh()))).toEqual(Array(128).fill(false));
        expect(starts).toBe(startsAtExpiry); expect(cancels).toBe(1);
        if (disposition === "cancelled_57014") reject(Object.assign(new Error("cancelled"), { code: "57014" }));
        else if (disposition === "late_schema_failure") reject(Object.assign(new Error("invalid schema"), { code: "42P01" }));
        else if (disposition === "unknown_57014") reject(emitted.unknownError());
        else settle([]);
        for (let i = 0; i < 100; i++) await Promise.resolve();
        expect(starts).toBe(startsAtExpiry); // No subsequent schema/auth statement runs.
        expect(emitted.state().ready).toBe(false); expect(emitted.state().client).toBe(originalClient);
        expect(emitted.state().fatal).toBe((disposition === "late_schema_failure" || disposition === "unknown_57014"));
        recovering = false;
        expect(await emitted.refresh()).toBe((disposition !== "late_schema_failure" && disposition !== "unknown_57014"));
        expect(emitted.state().ready).toBe((disposition !== "late_schema_failure" && disposition !== "unknown_57014"));
        expect(executed.some(statement => statement.includes(chosen))).toBe(true);
      });
    }
  }
});

for(const adapter of ["sqlite",...(Bun.env.JADPO_READINESS_TEST_DATABASE_URL ? ["postgres"] : [])]) {
  test(`native ${adapter} malformed authentication schema remains fatal before public bind`, async () => {
    const reserve = () => {const server=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});const port=server.port;server.stop(true);return port;};
    const port=reserve(),deploymentPort=reserveDeploymentPort(port),database="jadpo_startup_fatal_"+crypto.randomUUID().replaceAll("-","");
    const path=join(root,database+".sqlite");
    const url=adapter === "postgres" ? new URL(Bun.env.JADPO_READINESS_TEST_DATABASE_URL!) : null;
    const control=url ? new SQL({url:url.href,prepare:false}) : null;
    let probe:SQL|null=null,child:ReturnType<typeof Bun.spawn>|null=null;
    try {
      if(control&&url){await control.unsafe(`CREATE DATABASE "${database}"`);url.pathname="/"+database;probe=new SQL({url:url.href,prepare:false});await probe.unsafe('CREATE TABLE "__jadpo_auth_sessions" ("id" TEXT PRIMARY KEY)');}
      else {const sqlite=new Database(path,{create:true});sqlite.exec('CREATE TABLE "__jadpo_auth_sessions" ("id" TEXT PRIMARY KEY)');sqlite.close();}
      const secret=Buffer.alloc(32,7).toString("base64url");
      child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",generatedAuthApplication],{env:{PATH:Bun.env.PATH!,AUTH_SIGNING_KEY:secret,AUTH_PREVIOUS_SIGNING_KEY:Buffer.alloc(32,8).toString("base64url"),BROWSER_ORIGIN:"https://example.test",PORT:String(port),JADPO_DEPLOYMENT_PORT:String(deploymentPort),...(url?{DATABASE_URL:url.href}:{SQLITE_PATH:path})},stdout:"pipe",stderr:"pipe"});
      const exit=await Promise.race([child.exited,Bun.sleep(3_000).then(()=>null)]);
      expect(exit).toBe(1);const stderr=await new Response(child.stderr).text(),stdout=await new Response(child.stdout).text();child=null;
      expect(stderr).toContain("RUNTIME_STARTUP_FAILED");expect(stderr).not.toContain(secret);expect(stdout).not.toContain("runtime.ready");expect(stdout).not.toContain("runtime.live");
      for(const [listener,pathname] of [[port,"/health/live"],[deploymentPort,"/health/ready"]] as const) await expect(fetch(`http://127.0.0.1:${listener}${pathname}`,{signal:AbortSignal.timeout(100)})).rejects.toBeDefined();
      if(probe)expect((await probe.unsafe("SELECT to_regclass('public.user') AS entity"))[0].entity).toBeNull();
    } finally {if(child){child.kill("SIGKILL");await child.exited;}await probe?.close();if(control){await control.unsafe(`DROP DATABASE IF EXISTS "${database}" WITH (FORCE)`).catch(()=>{});await control.close();}}
  });
}

if (Bun.env.JADPO_READINESS_TEST_DATABASE_URL) {
  test("two native PostgreSQL startups share a fresh schema without a catalogue race", async () => {
    const control=new SQL({url:Bun.env.JADPO_READINESS_TEST_DATABASE_URL!,prepare:false});
    const database="jadpo_startup_"+crypto.randomUUID().replaceAll("-","");
    const url=new URL(Bun.env.JADPO_READINESS_TEST_DATABASE_URL!);url.pathname="/"+database;
    const children:ReturnType<typeof Bun.spawn>[]=[],ports:number[]=[];
    const reserve = () => {const server=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});const port=server.port;server.stop(true);return port;};
    try {
      await control.unsafe(`CREATE DATABASE "${database}"`);
      for(let i=0;i<2;i++) {
        const port=reserve(),deploymentPort=reserveDeploymentPort(port);ports.push(deploymentPort);
        children.push(Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",generatedApplication],{env:{PATH:Bun.env.PATH!,DATABASE_URL:url.href,PORT:String(port),JADPO_DEPLOYMENT_PORT:String(deploymentPort),JADPO_DEBUG_TARGET_STACKS:Bun.env.JADPO_STARTUP_RACE_DEBUG === "1" ? "1" : "0"},stdout:"pipe",stderr:"pipe"}));
      }
      const started=performance.now();let ready:number[]=Array(2).fill(0);
      while(performance.now()-started<5_000 && ready.some(status=>status!==200) && children.every(child=>child.exitCode===null)) {
        ready=await Promise.all(ports.map(async port=>{try{return(await fetch(`http://127.0.0.1:${port}/health/ready`,{signal:AbortSignal.timeout(100)})).status;}catch{return 0;}}));await Bun.sleep(10);
      }
      for(const child of children) {if(child.exitCode===null)child.kill("SIGTERM");await child.exited;}
      const logs=await Promise.all(children.map(child=>new Response(child.stderr).text()));
      console.log(JSON.stringify({case:"native_concurrent_initial_schema",ready,exitCodes:children.map(child=>child.exitCode),startupFailures:logs.map(log=>log.includes("RUNTIME_STARTUP_FAILED")),nativeCodes:logs.map(log=>["23505","42P07","42710","42P01"].filter(code=>log.includes(code))),pgTypeUnique:logs.some(log=>log.includes("pg_type_typname_nsp_index"))}));
      expect(ready).toEqual([200,200]);expect(logs.every(log=>!log.includes("RUNTIME_STARTUP_FAILED"))).toBe(true);
    } finally {for(const child of children)if(child.exitCode===null){child.kill("SIGKILL");await child.exited;}await control.unsafe(`DROP DATABASE IF EXISTS "${database}" WITH (FORCE)`).catch(()=>{});await control.close();}
  },10_000);

  test("initial PostgreSQL budget settings stay within the actual schema transaction", async () => {
    const source = readFileSync(join(dirname(generatedApplication), "persistence.ts"), "utf8")
      .replace(/^import .+;\n/gm, "").replace(/\bexport /gu, "")
      .replaceAll("import.meta.url", JSON.stringify(pathToFileURL(join(dirname(generatedApplication), "persistence.ts")).href));
    const javascript = new Bun.Transpiler({loader:"ts"}).transformSync(source) + '\nreturn {connection: postgres, ready: persistenceReady, fatal: persistenceInitializationFatal};';
    const AsyncFunction = Object.getPrototypeOf(async function(){}).constructor;
    const emitted = await new AsyncFunction("initialNativeStartupDeadline", "SQL", "Database", "Bun", javascript)(initialNativeStartupDeadline,SQL,Database,{env:{DATABASE_URL:Bun.env.JADPO_READINESS_TEST_DATABASE_URL!}});
    try {
      expect(emitted.ready).toBe(true);expect(emitted.fatal).toBe(false);
      const connection=await emitted.connection.reserve();
      try {
        const settings=await connection.unsafe("SELECT current_setting('statement_timeout') AS statement_timeout,current_setting('lock_timeout') AS lock_timeout");
        expect(settings[0]).toMatchObject({statement_timeout:"0",lock_timeout:"0"});
      } finally {connection.release();}
    } finally {await emitted.connection.close();}
  });

  for(const variant of ["held","cumulative","admission"] as const) test(`initial PostgreSQL ${variant} schema budget ends actual DDL before public listeners bind`, async () => {
    const cumulative=variant === "cumulative",admission=variant === "admission";
    const databaseUrl = Bun.env.JADPO_READINESS_TEST_DATABASE_URL!;
    const control = new SQL({url:databaseUrl,prepare:false,max:3});
    let locker:any=null,secondLocker:any=null,child:ReturnType<typeof Bun.spawn>|null=null;
    const reserve = () => {const server=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});const port=server.port;server.stop(true);return port;};
    const port=reserve(),deploymentPort=reserveDeploymentPort(port);
    const reachable = async (listener:number,path:string) => {try{return(await fetch(`http://127.0.0.1:${listener}${path}`,{signal:AbortSignal.timeout(100)})).status;}catch{return null;}};
    try {
      await control.unsafe('CREATE TABLE IF NOT EXISTS "account" ("id" TEXT NOT NULL PRIMARY KEY, "handle" TEXT NOT NULL UNIQUE, "tenant" TEXT NOT NULL, "owner_email" TEXT NOT NULL, UNIQUE("tenant","owner_email"))');
      await control.unsafe('DROP INDEX IF EXISTS "account_owner_email_idx"');
      locker=await control.reserve();await locker.unsafe("BEGIN");
      if(admission)await locker.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('jadpo:native-schema-initialization:v1',0))");
      else await locker.unsafe('LOCK TABLE "account" IN ACCESS EXCLUSIVE MODE');
      if(cumulative) {
        await control.unsafe('DROP INDEX IF EXISTS "user_group_idx"');
        secondLocker=await control.reserve();await secondLocker.unsafe("BEGIN");await secondLocker.unsafe('LOCK TABLE "user" IN ACCESS EXCLUSIVE MODE');
      }
      const started=performance.now();
      child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",generatedApplication],{env:{PATH:Bun.env.PATH!,DATABASE_URL:databaseUrl,PORT:String(port),JADPO_DEPLOYMENT_PORT:String(deploymentPort)},stdout:"pipe",stderr:"pipe"});
      let backend:number|null=null,everBound=false,secondWait=false;
      while(performance.now()-started<14_000 && child.exitCode===null) {
        if(cumulative && locker && performance.now()-started>=5_000) {await locker.unsafe("ROLLBACK");locker.release();locker=null;}
        const waiting=await control.unsafe("SELECT pid, query FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND (query LIKE '%account_owner_email_idx%' OR query LIKE '%user_group_idx%' OR query LIKE '%jadpo:native-schema-initialization:v1%')");
        if(waiting.length===1) {backend=Number(waiting[0].pid);secondWait ||= String(waiting[0].query).includes('user_group_idx');}
        everBound ||= (await reachable(port,"/health/live"))!==null || (await reachable(deploymentPort,"/health/ready"))!==null;
        await Bun.sleep(50);
      }
      expect(backend).not.toBeNull();expect(child.exitCode).not.toBeNull();if(cumulative)expect(secondWait).toBe(true);
      const elapsed=performance.now()-started;
      expect(await child.exited).toBe(1);expect(elapsed).toBeGreaterThan(9_500);expect(elapsed).toBeLessThan(13_000);expect(everBound).toBe(false);
      const stderr=await new Response(child.stderr).text();child=null;
      expect(stderr).toContain("RUNTIME_STARTUP_FAILED");expect(stderr).not.toContain(databaseUrl);
      expect(await control.unsafe("SELECT pid FROM pg_stat_activity WHERE pid=$1",[backend])).toHaveLength(0);
      // The blocker remains held: the initializer ended without fixture release.
      expect(await control.unsafe("SELECT indexname FROM pg_indexes WHERE schemaname='public' AND indexname='account_owner_email_idx'")).toHaveLength(0);
      if(cumulative)expect(await control.unsafe("SELECT indexname FROM pg_indexes WHERE schemaname='public' AND indexname='user_group_idx'")).toHaveLength(0);
      if(locker){await locker.unsafe("ROLLBACK");locker.release();locker=null;}
      if(secondLocker){await secondLocker.unsafe("ROLLBACK");secondLocker.release();secondLocker=null;}
      console.log(JSON.stringify({case:"actual_postgres_startup_schema_budget",variant,cumulative,secondWait,elapsedMs:Math.round(elapsed),applicationBound:everBound,nativeInitializerRemaining:0,indexCommitted:false}));
    } finally {if(child){child.kill("SIGKILL");await child.exited;}if(locker){await locker.unsafe("ROLLBACK").catch(()=>{});locker.release();}if(secondLocker){await secondLocker.unsafe("ROLLBACK").catch(()=>{});secondLocker.release();}await control.close();}
  },20_000);

  async function nativeRecovery(driver: any = SQL) {
    const source=readFileSync(join(dirname(generatedApplication),"persistence.ts"),"utf8")
      .replace(/^import .+;\n/gm,"").replace(/\bexport /gu,"")
      .replaceAll("import.meta.url",JSON.stringify(pathToFileURL(join(dirname(generatedApplication),"persistence.ts")).href));
    const javascript=new Bun.Transpiler({loader:"ts"}).transformSync(source)+'\nreturn {connection:postgres,refresh:refreshPersistenceReadiness,unavailable:()=>{persistenceReady=false;persistenceProbeNeeded=true},state:()=>({ready:persistenceReady,fatal:persistenceInitializationFatal}),close:()=>postgres?.close()};';
    const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
    return new AsyncFunction("initialNativeStartupDeadline","SQL","Database","Bun",javascript)(initialNativeStartupDeadline,driver,Database,{env:{DATABASE_URL:Bun.env.JADPO_READINESS_TEST_DATABASE_URL!}});
  }
  test("recovery server limits restore existing settings on the same actual PostgreSQL backend",async()=>{
    // Pin one actual native backend for the setting-isolation control only.
    function OneConnection(options:any){return new SQL({...options,max:1})}
    const emitted=await nativeRecovery(OneConnection);let reserved:any=null;
    try{
      reserved=await emitted.connection.reserve();await reserved.unsafe("SET statement_timeout='25s'");await reserved.unsafe("SET lock_timeout='21s'");
      const before=(await reserved.unsafe("SELECT pg_backend_pid() AS pid,current_setting('statement_timeout') AS statement_timeout,current_setting('lock_timeout') AS lock_timeout"))[0];reserved.release();reserved=null;
      emitted.unavailable();expect(await emitted.refresh()).toBe(true);
      reserved=await emitted.connection.reserve();const after=(await reserved.unsafe("SELECT pg_backend_pid() AS pid,current_setting('statement_timeout') AS statement_timeout,current_setting('lock_timeout') AS lock_timeout"))[0];
      expect(after).toEqual(before);expect(after).toMatchObject({statement_timeout:"25s",lock_timeout:"21s"});
      console.log(JSON.stringify({case:"actual_recovery_same_backend_settings",before,after}));
    }finally{reserved?.release();await emitted.close()}
  });
  test("two native PostgreSQL recovery lock waits share the original readiness attempt deadline",async()=>{
    const control=new SQL({url:Bun.env.JADPO_READINESS_TEST_DATABASE_URL!,prepare:false,max:3});
    const emitted=await nativeRecovery();let first:any=null,second:any=null;
    try{
      await control.unsafe('DROP INDEX IF EXISTS "account_owner_email_idx"');await control.unsafe('DROP INDEX IF EXISTS "user_group_idx"');
      first=await control.reserve();await first.unsafe("BEGIN");await first.unsafe('LOCK TABLE "account" IN ACCESS EXCLUSIVE MODE');
      second=await control.reserve();await second.unsafe("BEGIN");await second.unsafe('LOCK TABLE "user" IN ACCESS EXCLUSIVE MODE');
      const started=performance.now();emitted.unavailable();const response=emitted.refresh();let firstWait=false,secondWait=false,remaining=0;
      do{
        if(first&&performance.now()-started>=650){await first.unsafe("ROLLBACK");first.release();first=null}
        const rows=await control.unsafe("SELECT query FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'");
        firstWait ||= rows.some(row=>String(row.query).includes("account_owner_email_idx"));secondWait ||= rows.some(row=>String(row.query).includes("user_group_idx"));
        remaining=rows.filter(row=>String(row.query).includes("account_owner_email_idx")||String(row.query).includes("user_group_idx")).length;
        await Bun.sleep(5);
      }while((performance.now()-started<1000||remaining)&&performance.now()-started<1400);
      const elapsedMs=performance.now()-started;
      expect(await response).toBe(false);expect(firstWait).toBe(true);expect(secondWait).toBe(true);expect(remaining).toBe(0);expect(elapsedMs).toBeLessThan(1400);
      expect(await control.unsafe("SELECT oid FROM pg_class WHERE relname IN ('account_owner_email_idx','user_group_idx')")).toHaveLength(0);
      expect(emitted.state()).toEqual({ready:false,fatal:false});
      console.log(JSON.stringify({case:"actual_recovery_cumulative_budget",elapsedMs,firstReleasedAtMs:650,firstWait,secondWait,remaining,partialIndexCommitted:false,externalSecondLockStillHeld:true}));
    }finally{for(const lock of [first,second])if(lock){await lock.unsafe("ROLLBACK").catch(()=>{});lock.release()}await emitted.close();await control.close()}
  },5000);
  test("actual generated PostgreSQL recovery server budget ends held DDL without releasing the external lock", async () => {
    const databaseUrl = Bun.env.JADPO_READINESS_TEST_DATABASE_URL!;
    const control = new SQL({ url: databaseUrl, prepare: false, max: 2 });
    let locker: any = null, emitted: any = null;
    try {
      const source = readFileSync(join(dirname(generatedApplication), "persistence.ts"), "utf8")
        .replace(/^import .+;\n/gm, "").replace(/\bexport /gu, "")
        .replaceAll("import.meta.url", JSON.stringify(pathToFileURL(join(dirname(generatedApplication), "persistence.ts")).href));
      const javascript = new Bun.Transpiler({ loader: "ts" }).transformSync(source)
        + '\nreturn {refresh: refreshPersistenceReadiness, state: () => ({ready: persistenceReady, fatal: persistenceInitializationFatal, client: activePersistenceClient}), unavailable: () => {persistenceReady=false; persistenceProbeNeeded=true;}, close: () => postgres?.close()};';
      const AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
      emitted = await new AsyncFunction("initialNativeStartupDeadline", "SQL", "Database", "Bun", javascript)(initialNativeStartupDeadline, SQL, Database, {env:{DATABASE_URL:databaseUrl}});
      expect(emitted.state().ready).toBe(true);
      const originalClient = emitted.state().client;
      await control.unsafe('DROP INDEX IF EXISTS "account_owner_email_idx"');
      locker = await control.reserve(); await locker.unsafe('BEGIN');
      await locker.unsafe('LOCK TABLE "account" IN ACCESS EXCLUSIVE MODE');
      emitted.unavailable(); const started = performance.now();
      const responses = Array.from({length:32},()=>emitted.refresh());
      expect(responses.every(response=>response===responses[0])).toBe(true);
      let blocked = false;
      for(let i=0;i<40;i++) {
        const rows = await control.unsafe("SELECT query FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'");
        if(rows.some(row=>String(row.query).includes('CREATE INDEX IF NOT EXISTS "account_owner_email_idx"'))) {blocked=true;break;}
        await Bun.sleep(10);
      }
      expect(blocked).toBe(true);
      expect(await Promise.all(responses)).toEqual(Array(32).fill(false));
      expect(performance.now()-started).toBeLessThan(1600);
      expect(emitted.state().ready).toBe(false); expect(emitted.state().client).toBe(originalClient);
      // The caller deadline remains separate from actual native settlement.
      // Server remaining-budget limits now stop the held DDL; the locker is
      // deliberately retained until native disappearance and rollback prove it.
      expect(await Promise.all(Array.from({length:128},()=>emitted.refresh()))).toEqual(Array(128).fill(false));
      let remaining=1;
      for(let i=0;i<100&&remaining;i++){
        const rows=await control.unsafe("SELECT query FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'");
        remaining=rows.filter(row=>String(row.query).includes('CREATE INDEX IF NOT EXISTS "account_owner_email_idx"')).length;
        if(remaining)await Bun.sleep(10);
      }
      expect(remaining).toBe(0);
      expect((await control.unsafe("SELECT oid FROM pg_class WHERE relname='account_owner_email_idx'"))).toHaveLength(0);
      expect(emitted.state().fatal).toBe(false);
      await locker.unsafe('ROLLBACK'); locker.release(); locker=null;
      await Bun.sleep(300);
      expect(emitted.state().ready).toBe(false); expect(emitted.state().client).toBe(originalClient);
      expect(emitted.state().fatal).toBe(false);
      expect(await emitted.refresh()).toBe(true); expect(emitted.state().ready).toBe(true);
      expect(emitted.state().client).not.toBe(originalClient);
    } finally {
      if(locker) {await locker.unsafe('ROLLBACK').catch(()=>{});locker.release();}
      await emitted?.close(); await control.close();
    }
  }, 10000);
  test("real recovery COMMIT acknowledgement hold preserves committed schema, native slot and late-publication refusal",async()=>{
    const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",join(import.meta.dir,"fixtures/recovery-owned-commit.ts")],{env:{PATH:Bun.env.PATH!,JADPO_READINESS_TEST_DATABASE_URL:Bun.env.JADPO_READINESS_TEST_DATABASE_URL!,JADPO_READINESS_APPLICATION:generatedApplication},stdout:"pipe",stderr:"pipe"});
    const watchdog=setTimeout(()=>child.kill("SIGKILL"),8000);
    try{
    const [stdout,stderr,exit]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
    if(exit!==0)throw Error(`Native recovery COMMIT fixture failed: ${stdout}\n${stderr}`);
    expect(exit).toBe(0);const proof=JSON.parse(stdout);expect(proof.before.committed).toBe(true);expect(proof.before.retainedSlot).toBe(true);expect(proof.before.clientUnchanged).toBe(true);expect(proof.after.freshRecovery).toBe(true);expect(proof.after.backendRemaining).toBe(0);console.log(stdout.trim());
    }finally{clearTimeout(watchdog);if(child.exitCode===null)child.kill("SIGKILL");await child.exited}
  },10000);

}


// Actual native main and PostgreSQL result transport; no replacement evaluator.
describe("ordinary native PostgreSQL probe ownership", () => {
  for (const mode of ["healthy", "hold", "keep-hold"]) {
    const run = Bun.env.JADPO_READINESS_TEST_DATABASE_URL ? test : test.skip;
    run(`actual native ordinary probe ${mode} preserves response and owner boundaries`, async () => {
      const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null",
        join(import.meta.dir, "fixtures/ordinary-owned-probe.ts"), mode], {
        env: { PATH: Bun.env.PATH!, JADPO_READINESS_TEST_DATABASE_URL: Bun.env.JADPO_READINESS_TEST_DATABASE_URL!,
          JADPO_READINESS_APPLICATION: generatedApplication }, stdout: "pipe", stderr: "pipe",
      });
      const watchdog = setTimeout(() => child.kill("SIGKILL"), 18000);
      try {
        const [stdout, stderr, exit] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
        if (exit !== 0) throw Error(`Native ordinary probe fixture failed: ${stdout}\n${stderr}`);
        const proof = JSON.parse(stdout);
        expect(proof.failure).toBeNull(); expect(proof.qualifiedNativeExecution).toBe(true);
        expect(proof.watchdog).toBe(false); expect(proof.nativeExit).toBe(143);
        expect(proof.backendRemaining).toBe(0); expect(proof.cleanupBeforeFixtureTeardown).toBe(true);
        expect(proof.safeNativeOutput).toBe(true); expect(proof.protocolFailure).toBeNull();
        if (mode !== "healthy") {
          const responses = proof.observations.find((p: any) => p.phase === "first_http_joiners").responses;
          expect(responses).toHaveLength(32);
          expect(responses.every((r: any) => r.status === 503 && r.elapsedMs >= 900 && r.elapsedMs < 1600)).toBe(true);
        }
        expect(proof.downstreamClosed).toBe(proof.connections); expect(proof.upstreamClosed).toBe(proof.connections);
        expect(proof.nativeProcesses).toHaveLength(2); expect(proof.formerPorts.every((p: any) => p.refused)).toBe(true);
        console.log(stdout.trim());
      } finally { clearTimeout(watchdog); if (child.exitCode === null) child.kill("SIGKILL"); await child.exited; }
    }, 20000);
  }
});
