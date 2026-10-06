import { afterAll, beforeEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const postgresUrl = Bun.env.JADPO_LIFECYCLE_DATABASE_URL;
delete Bun.env.DATABASE_URL;
delete Bun.env.SQLITE_PATH;
const root = mkdtempSync(join(tmpdir(), "jadpo-hourly-maintenance-"));
const databasePath = join(root, "maintenance.sqlite");
if (postgresUrl) Bun.env.DATABASE_URL = postgresUrl;
else Bun.env.SQLITE_PATH = databasePath;
const modulePath = new URL("../../tests/compile/pass/lifecycle/build/target/persistence.ts", import.meta.url).pathname;
const { persistence, configureLifecycleRetention } = await import(modulePath);
configureLifecycleRetention({ soft_delete_retention: "30d" });
Bun.env.JADPO_LIFECYCLE_RETENTION = "30d";
const { startLifecycleMaintenance } = await import(new URL("../../tests/compile/pass/lifecycle/build/target/app.ts", import.meta.url).pathname);
const postgres = postgresUrl ? new SQL({ url: postgresUrl, prepare: false }) : null;
const sqlite = postgres ? null : new Database(databasePath);
sqlite?.exec("PRAGMA foreign_keys=ON");
const owner = crypto.randomUUID();
const base = Math.floor(Date.now() / 3_600_000) * 3_600_000 - 3 * 3_600_000;
const at = (milliseconds: number) => new Date(milliseconds).toISOString();
const value = (milliseconds: number) => postgres ? at(milliseconds) : milliseconds;
async function sql(query: string, values: any[] = []): Promise<any[]> {
  if (postgres) return await postgres.unsafe(query, values);
  const statement = sqlite!.prepare(query);
  try { return statement.all(...values); } finally { statement.finalize(); }
}
async function count(): Promise<number> { return Number((await sql('SELECT COUNT(*) AS n FROM "todo"'))[0].n); }
async function pulse(milliseconds: number) {
  return persistence.withOperationTime(at(milliseconds)).scheduled_retention_purge_Todo(() => {});
}
async function seed(amount: number, deleted: number | null = Date.now() - 90 * 86_400_000): Promise<string[]> {
  const ids: string[] = [];
  for (let i = 0; i < amount; i++) {
    const id = crypto.randomUUID(); ids.push(id);
    await sql('INSERT INTO "todo" (id,user_id,title,status,deleted_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6)', [id, owner, "retained", "open", deleted === null ? null : value(deleted), value(Date.now())]);
  }
  return ids;
}
beforeEach(async () => {
  await pulse(base - 3_600_000); // Native schema admission, then isolated fixture reset.
  await sql('DELETE FROM "todo_reference"');
  await sql('DELETE FROM "todo"');
  await sql('DELETE FROM "user"');
  await sql('DELETE FROM "__jadpo_maintenance_schedule_v1"');
  await sql('INSERT INTO "user" (id,name,batch_group,status,disabled_at,updated_at) VALUES ($1,$2,$3,$4,NULL,$5)', [owner, "owner", "maintenance", "active", value(Date.now())]);
});
afterAll(async () => { sqlite?.close(); await postgres?.close(); rmSync(root, { recursive: true, force: true }); delete Bun.env.DATABASE_URL; delete Bun.env.SQLITE_PATH; delete Bun.env.JADPO_LIFECYCLE_RETENTION; });

test("UTC hourly singleton coalesces missed ticks, retains500 bound and excludes live/young rows", async () => {
  await seed(501); await seed(1, null); await seed(1, Date.now() - 86_400_000);
  const first = await pulse(base + 1);
  expect(first.outcome).toBe("committed"); expect(first.result.committedCount).toBe(500);
  expect(first.scheduledFor).toBe(at(base)); expect(await count()).toBe(3);
  expect(await pulse(base + 3_599_999)).toBeNull(); expect(await count()).toBe(3);
  const next = await pulse(base + 2 * 3_600_000 + 1); // One occurrence, not two backlog batches.
  expect(next.scheduledFor).toBe(at(base + 2 * 3_600_000)); expect(next.result.committedCount).toBe(1);
  expect(await count()).toBe(2); expect(await pulse(base + 3_600_000)).toBeNull();
});

test("known failed purge rolls back its entire batch and retries only next hourly occurrence", async () => {
  const ids = await seed(2);
  await sql('INSERT INTO "todo_reference" (id,todo_id) VALUES ($1,$2)', [crypto.randomUUID(), ids[1]]);
  const failed = await pulse(base);
  expect(failed.outcome).toBe("rolled_back"); expect(await count()).toBe(2);
  await sql('DELETE FROM "todo_reference"');
  expect(await pulse(base + 60_000)).toBeNull(); expect(await count()).toBe(2);
  expect((await pulse(base + 3_600_000)).result.committedCount).toBe(2); expect(await count()).toBe(0);
});

test("actual native maintenance host starts a clause-bound hourly pulse and can stop future wakes", async () => {
  await seed(1); await seed(1, null);
  const stop = startLifecycleMaintenance();
  try {
    const deadline = performance.now() + 3000;
    while (await count() !== 1 && performance.now() < deadline) await Bun.sleep(10);
    expect(await count()).toBe(1);
    const occurrence = (await sql('SELECT completed_for FROM "__jadpo_maintenance_schedule_v1"'))[0].completed_for;
    expect(occurrence.endsWith(":00:00.000Z")).toBe(true);
  } finally { stop(); }
});

function worker(hold = false) {
  return Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", new URL("./fixtures/lifecycle-maintenance-worker.ts", import.meta.url).pathname], {
    env: { ...Bun.env, JADPO_MAINTENANCE_MODULE: modulePath, JADPO_MAINTENANCE_AT: at(base), JADPO_MAINTENANCE_HOLD: hold ? "1" : "0" }, stdout: "pipe", stderr: "pipe",
  });
}
test("separate host crash before commit reclaims safely; committed occurrence survives process restart", async () => {
  await seed(501);
  const held = worker(true);
  try {
    const reader = held.stdout.getReader();
    const observed = await Promise.race([reader.read(), new Promise<never>((_, reject) => setTimeout(() => reject(new Error("native held worker handshake timed out")), 5000))]);
    expect(new TextDecoder().decode(observed.value)).toContain("HELD"); reader.releaseLock();
    // A second process must not commit a competing batch while ownership is held.
    const contender = worker();
    const contenderResult = new Response(contender.stdout).text();
    await Bun.sleep(100);
    expect(await count()).toBe(501);
    held.kill(9); await held.exited;
    const output = JSON.parse((await contenderResult).trim()); await contender.exited;
    expect(["committed", "refused"]).toContain(output.outcome);
    if (output.outcome === "refused") {
      const recovery = worker(); const result = JSON.parse((await new Response(recovery.stdout).text()).trim());
      expect(await recovery.exited).toBe(0); expect(result.outcome).toBe("committed"); expect(result.count).toBe(500);
    } else expect(output.count).toBe(500);
    expect(await count()).toBe(1);
    const restarted = worker(); const result = JSON.parse((await new Response(restarted.stdout).text()).trim());
    expect(await restarted.exited).toBe(0); expect(result.outcome).toBe("skipped"); expect(await count()).toBe(1);
  } finally { held.kill(9); await held.exited; }
}, 15_000);
