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

const root = mkdtempSync(join(tmpdir(), "jadpo-readiness-"));
const databaseDirectory = join(root, "recovered");


function reserveDeploymentPort(applicationPort: number): number {
  for (let attempt = 0; attempt < 3; attempt++) { const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() }); const port = server.port; server.stop(true); if (port !== applicationPort) return port; }
  throw Error("Could not reserve a separate deployment port");
}

function controlledReadiness(initialReady = true) {
  const state = { ready: initialReady, fatal: false, needed: false, generation: 0,
    checks: 0, prepares: 0, publications: 0, failures: 0, cancels: 0, timers: 0, now: 0 };
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
    check: (_recovering, setCancel) => {
      state.checks++; setCancel(() => { state.cancels++; cancel(); }); return work();
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
    expect(generated).toContain("recovering ? initializePersistenceOnce(false, setCancel)");
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
      expect(h.state.checks).toBe(1); expect(observers).toBe(1); expect(h.state.timers).toBe(1);
      h.advance(1000);
      expect(await Promise.all(callers)).toEqual(Array(32).fill(false));
      expect(h.state.ready).toBe(false); expect(h.state.failures).toBe(1); expect(h.state.cancels).toBe(1);
      h.advance(60000);
      expect(await Promise.all(Array.from({ length: 128 }, () => h.gate.refresh()))).toEqual(Array(128).fill(false));
      expect(h.state.checks).toBe(1); expect(observers).toBe(1); expect(h.state.timers).toBe(1);
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
        const emitted = await new AsyncFunction("SQL", "Database", "Bun", "performance", "setTimeout", "clearTimeout", "console", javascript)(
          SQL, Database, { env: { DATABASE_URL: "postgres://fixture.invalid/never_opened" } }, { now: () => now },
          (callback: () => void) => { const id = ++sequence; timers.set(id, callback); return id; },
          (id: number) => timers.delete(id), { error() {} },
        );
        const originalClient = emitted.state().client;
        expect(emitted.state().ready).toBe(true);
        recovering = true; emitted.unavailable();
        const responses = Array.from({ length: 32 }, () => emitted.refresh());
        expect(responses.every(value => value === responses[0])).toBe(true);
        for (let i = 0; i < 100 && !settle; i++) await Promise.resolve();
        expect(typeof settle).toBe("function");
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

if (Bun.env.JADPO_READINESS_TEST_DATABASE_URL) {
  test("actual generated PostgreSQL recovery retains a held native DDL lifetime after advisory cancellation", async () => {
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
      emitted = await new AsyncFunction("SQL", "Database", "Bun", javascript)(SQL, Database, {env:{DATABASE_URL:databaseUrl}});
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
      // Bun's advisory cancel does not establish interruption of this held DDL.
      // Keep the actual native slot, refuse replacement work, and fence late success.
      expect(await Promise.all(Array.from({length:128},()=>emitted.refresh()))).toEqual(Array(128).fill(false));
      const stillBlocked = await control.unsafe("SELECT query FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'");
      expect(stillBlocked.filter(row=>String(row.query).includes('CREATE INDEX IF NOT EXISTS "account_owner_email_idx"'))).toHaveLength(1);
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
}
