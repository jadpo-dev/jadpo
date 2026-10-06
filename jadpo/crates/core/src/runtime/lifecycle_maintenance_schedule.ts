// Included privately inside the native persistence client. The renderer supplies
// only a checked purge-clause identity and its generated purge callback.
const runScheduledMaintenance = async <T>(binding: string, attemptNow: string | null, purge: () => Promise<T>): Promise<null | Readonly<{ scheduledFor: string; outcome: "committed"; result: T } | { scheduledFor: string; outcome: "rolled_back"; error: unknown }>> => {
  assertTransactionActive();
  if (!transactional || attemptNow === null) throw new PersistenceFault("maintenance.schedule", "data", new Error("maintenance requires an active operation-time transaction"));
  const execute = async (operation: string, sql: string, values: unknown[] = []): Promise<any[]> => postgres !== null
    ? persistenceAsync(operation, () => postgres!.unsafe(sql, values))
    : persistenceSync(operation, () => { const statement = sqlite!.prepare(sql); try { return statement.all(...values); } finally { statement.finalize(); } });
  if (postgres !== null) {
    const present = await execute("maintenance.schema.present", "SELECT to_regclass('\"__jadpo_maintenance_schedule_v1\"') IS NOT NULL AS present");
    if (present.length !== 1) throw new PersistenceFault("maintenance.schema", "driver", new Error("invalid maintenance schema result"));
    if (present[0].present !== true) {
      const admission = await execute("maintenance.schema.admission", "SELECT pg_try_advisory_xact_lock(1245791312, 1296124238) AS acquired");
      if (admission.length !== 1 || admission[0].acquired !== true) throw new PersistenceFault("maintenance.schema", "driver", new Error("maintenance schema busy"));
      await execute("maintenance.schema", 'CREATE TABLE IF NOT EXISTS "__jadpo_maintenance_schedule_v1" ("binding" TEXT PRIMARY KEY, "completed_for" TEXT)');
    }
  } else {
    await execute("maintenance.schema", 'CREATE TABLE IF NOT EXISTS "__jadpo_maintenance_schedule_v1" ("binding" TEXT PRIMARY KEY, "completed_for" TEXT)');
  }
  await execute("maintenance.enroll", 'INSERT INTO "__jadpo_maintenance_schedule_v1" (binding) VALUES ($1) ON CONFLICT (binding) DO NOTHING', [binding]);
  // Held through purge/savepoint recovery AND the outer commit. An elapsed
  // process timer never grants another host permission to delete concurrently.
  const rows = await execute("maintenance.lock", 'UPDATE "__jadpo_maintenance_schedule_v1" SET completed_for=completed_for WHERE binding=$1 RETURNING completed_for', [binding]);
  if (rows.length !== 1) throw new PersistenceFault("maintenance.schedule", "driver", new Error("invalid maintenance schedule result"));
  const clock = postgres !== null
    ? "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS now"
    : "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now') AS now";
  const times = await execute("maintenance.clock", clock);
  if (times.length !== 1) throw new PersistenceFault("maintenance.clock", "driver", new Error("invalid maintenance clock"));
  const canonical = (value: unknown): value is string => typeof value === "string" && /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(value) && !value.startsWith("0000-") && Number.isFinite(Date.parse(value)) && new Date(value).toISOString() === value;
  if (!canonical(attemptNow) || !canonical(times[0].now) || (rows[0].completed_for !== null && !canonical(rows[0].completed_for))) throw new PersistenceFault("maintenance.schedule", "data", new Error("invalid maintenance occurrence"));
  // Database time caps a future host clock. UTC arithmetic has no DST or local
  // timezone dependency; missed hours coalesce to this one bounded occurrence.
  const scheduledFor = new Date(Math.floor(Math.min(Date.parse(attemptNow), Date.parse(times[0].now)) / 3_600_000) * 3_600_000).toISOString();
  if (rows[0].completed_for !== null && rows[0].completed_for >= scheduledFor) return null;
  let outcome: Readonly<{ scheduledFor: string; outcome: "committed"; result: T } | { scheduledFor: string; outcome: "rolled_back"; error: unknown }>;
  try {
    outcome = { scheduledFor, outcome: "committed", result: await purge() };
  } catch (error) {
    // The supplied generated callback uses a native savepoint. Failed cleanup
    // poisons/expires the transaction, so it cannot acknowledge a failed hour.
    assertTransactionActive();
    outcome = { scheduledFor, outcome: "rolled_back", error };
  }
  await execute("maintenance.acknowledge", 'UPDATE "__jadpo_maintenance_schedule_v1" SET completed_for=$1 WHERE binding=$2', [scheduledFor, binding]);
  return Object.freeze(outcome);
};
