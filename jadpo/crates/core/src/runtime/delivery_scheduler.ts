// Compiler-owned singleton schedule storage. These native primitives do not
// activate authored jobs or choose a public execution profile. Generated callers
// must supply the exact checked binding/interval and validate typed page cursors.
type DeliveryActivation = Readonly<{
  binding: string; activationId: string; generation: string;
  scheduledFor: string; operationTime: string; leaseUntil: string;
  executionDeadline: string; page: Readonly<{ intentIds: readonly string[]; after: unknown }> | null;
  cursor: unknown;
}>;
function createDeliverySchedulePrimitives(
  execute: (operation: string, sql: string, values?: unknown[]) => Promise<any[]>,
  requireTransaction: () => void,
  isPostgres: boolean,
  atomic: <T>(work: () => Promise<T>) => Promise<T>,
) {
  const fault = (operation: string): never => { throw new PersistenceFault(operation, "driver", new Error("invalid durable activation contract")); };
  const instant = (value: unknown): string => {
    if (typeof value !== "string" || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(value) || value.startsWith("0000-") || !Number.isFinite(Date.parse(value)) || new Date(value).toISOString() !== value) return fault("delivery.activation_time");
    return value;
  };
  const positive = (value: number): void => { if (!Number.isSafeInteger(value) || value <= 0) fault("delivery.activation_profile"); };
  const durablePositive = (value: unknown, storageType: unknown): void => {
    // PostgreSQL BIGINT may be a canonical decimal string. SQLite affinity can
    // retain arbitrary TEXT/REAL despite a positive CHECK; require INTEGER
    // storage as well as the driver's safe integer value before native SQL can
    // interpret the duration. Number coercion is not a durable decoder.
    if (!isPostgres && storageType !== "integer") return fault("delivery.activation_state");
    if (typeof value === "number" && Number.isSafeInteger(value) && value > 0) return;
    if (isPostgres && typeof value === "string" && /^[1-9][0-9]{0,15}$/u.test(value) && BigInt(value) <= BigInt(Number.MAX_SAFE_INTEGER)) return;
    return fault("delivery.activation_state");
  };
  const clock = isPostgres ? "to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')" : "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";
  const add = (at: string, ms: number): string => {
    const value = Date.parse(at) + ms;
    if (!Number.isSafeInteger(value) || value > 253402300799999) return fault("delivery.activation_profile");
    return instant(new Date(value).toISOString());
  };
  const ensure = async () => {
    requireTransaction();
    await execute("delivery.activation.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_activations_v1" (
      "binding" TEXT PRIMARY KEY, "interval_ms" BIGINT NOT NULL CHECK ("interval_ms" > 0),
      "last_tick" TEXT, "pending_for" TEXT, "continuation_for" TEXT, "cursor" TEXT,
      "generation" BIGINT NOT NULL CHECK ("generation" >= 0), "activation_id" TEXT,
      "scheduled_for" TEXT, "operation_time" TEXT, "lease_until" TEXT, "execution_deadline" TEXT,
      "page" TEXT, "lease_ms" BIGINT CHECK ("lease_ms" > 0)
    )`);
    // Earlier private storage did not retain renewal duration. Add it without
    // deriving authority from timestamps or a caller's replacement profile.
    // A legacy active row remains usable under its old fence, but cannot renew
    // until expiry/reclaim installs an explicit checked duration.
    const columns = await execute("delivery.activation.schema", isPostgres
      ? 'SELECT attname AS name FROM pg_attribute WHERE attrelid=\'"__jadpo_delivery_activations_v1"\'::regclass AND attnum>0 AND NOT attisdropped'
      : 'PRAGMA table_info("__jadpo_delivery_activations_v1")');
    if (isPostgres) {
      if (!columns.some(column => column.name === "lease_ms")) await execute("delivery.activation.schema", 'ALTER TABLE "__jadpo_delivery_activations_v1" ADD COLUMN IF NOT EXISTS "lease_ms" BIGINT CHECK ("lease_ms" > 0)');
    } else {
      if (!columns.some(column => column.name === "lease_ms")) await execute("delivery.activation.schema", 'ALTER TABLE "__jadpo_delivery_activations_v1" ADD COLUMN "lease_ms" BIGINT CHECK ("lease_ms" > 0)');
    }
  };
  const lock = async (binding: string): Promise<any> => {
    const types = isPostgres ? "" : ', typeof("interval_ms") AS "__interval_storage_type", typeof("lease_ms") AS "__lease_storage_type"';
    const rows = await execute("delivery.activation.lock", 'UPDATE "__jadpo_delivery_activations_v1" SET "generation" = "generation" WHERE "binding" = $1 RETURNING *, CAST("generation" AS TEXT) AS "generation"' + types, [binding]);
    if (rows.length !== 1) return fault("delivery.activation_binding");
    const row = rows[0];
    // Durable TEXT is not Instant authority merely because it sorts after DB
    // now. Decode under the same row lock before any live/reclaim/write choice;
    // a caught decoder fault rolls the enclosing native savepoint back.
    if (typeof row.generation !== "string" || !/^(0|[1-9][0-9]{0,18})$/u.test(row.generation) || BigInt(row.generation) > 9223372036854775807n) return fault("delivery.activation_state");
    durablePositive(row.interval_ms, row.__interval_storage_type);
    if (row.lease_ms !== null) durablePositive(row.lease_ms, row.__lease_storage_type);
    for (const field of ["last_tick", "pending_for", "continuation_for", "scheduled_for", "operation_time", "lease_until", "execution_deadline"]) if (row[field] !== null) instant(row[field]);
    if ((row.cursor === null) !== (row.continuation_for === null)) return fault("delivery.activation_state");
    if (row.activation_id === null) {
      if ([row.scheduled_for, row.operation_time, row.lease_until, row.execution_deadline, row.page, row.lease_ms].some(value => value !== null)) return fault("delivery.activation_state");
    } else {
      if (typeof row.activation_id !== "string" || row.activation_id.length === 0 || row.generation === "0" || [row.scheduled_for, row.operation_time, row.lease_until, row.execution_deadline].some(value => value === null) || row.lease_until > row.execution_deadline) return fault("delivery.activation_state");
    }
    return row;
  };
  const now = async (): Promise<string> => {
    const rows = await execute("delivery.activation.time", `SELECT ${clock} AS now`);
    if (rows.length !== 1) return fault("delivery.activation_time");
    return instant(rows[0].now);
  };
  const current = async (handle: DeliveryActivation): Promise<any | null> => {
    const row = await lock(handle.binding), at = await now();
    return row.activation_id === handle.activationId && row.generation === handle.generation && row.lease_until > at && row.execution_deadline > at ? row : null;
  };
  const readPage = (encoded: string | null): DeliveryActivation["page"] => {
    if (encoded === null) return null;
    const page = JSON.parse(encoded);
    if (page === null || typeof page !== "object" || Reflect.ownKeys(page).sort().join(",") !== "after,intentIds" || !Array.isArray(page.intentIds) || page.intentIds.length > 500 || page.intentIds.some((id: unknown) => typeof id !== "string" || id.length === 0) || new Set(page.intentIds).size !== page.intentIds.length) return fault("delivery.activation_page");
    return Object.freeze({ intentIds: Object.freeze([...page.intentIds]), after: page.after });
  };
  return {
    async delivery_activation_clock(): Promise<string> {
      requireTransaction();
      return now();
    },
    async tick_delivery_schedule(binding: string, intervalMs: number, operationTime: string): Promise<void> {
      requireTransaction(); positive(intervalMs); instant(operationTime);
      if (typeof binding !== "string" || binding.length === 0) return fault("delivery.activation_binding");
      await atomic(async () => {
        await ensure();
        await execute("delivery.activation.create", 'INSERT INTO "__jadpo_delivery_activations_v1" (binding, interval_ms, generation) VALUES ($1,$2,0) ON CONFLICT (binding) DO NOTHING', [binding, intervalMs]);
        const row = await lock(binding);
        if (Number(row.interval_ms) !== intervalMs) return fault("delivery.activation_interval");
        const occurrence = new Date(Math.floor(Date.parse(operationTime) / intervalMs) * intervalMs).toISOString();
        // One latest occurrence, never a row per missed/overlapping tick. An old
        // or duplicate tick cannot manufacture another follow-up.
        if (row.last_tick === null || row.last_tick < occurrence) await execute("delivery.activation.tick", 'UPDATE "__jadpo_delivery_activations_v1" SET last_tick=$1, pending_for=$1 WHERE binding=$2', [occurrence, binding]);
      });
    },
    async claim_delivery_activation(binding: string, operationTime: string, profile: Readonly<{ executionMs: number; leaseMs: number }>): Promise<DeliveryActivation | null> {
      requireTransaction(); instant(operationTime);
      if (profile === null || typeof profile !== "object" || Reflect.ownKeys(profile).sort().join(",") !== "executionMs,leaseMs") return fault("delivery.activation_profile");
      positive(profile.executionMs); positive(profile.leaseMs);
      if (profile.leaseMs > profile.executionMs) return fault("delivery.activation_profile");
      return atomic(async () => {
        await ensure(); const row = await lock(binding), at = await now();
        if (row.activation_id !== null && row.lease_until > at && row.execution_deadline > at) return null;
        // Expired activation resumes its durable page/cursor. An already
        // committed dispatch remains governed by its separate intent fence and
        // unknown-outcome rules, never by this schedule lease.
        const resumed = row.activation_id !== null;
        const scheduledFor = resumed ? row.scheduled_for : row.continuation_for ?? row.pending_for;
        if (scheduledFor === null) return null;
        if (BigInt(row.generation) >= 9223372036854775807n) return fault("delivery.activation_generation");
        const activationId = crypto.randomUUID(), generation = String(BigInt(row.generation) + 1n);
        const executionDeadline = add(at, profile.executionMs), leaseUntil = add(at, profile.leaseMs);
        await execute("delivery.activation.claim", 'UPDATE "__jadpo_delivery_activations_v1" SET generation=$1, activation_id=$2, scheduled_for=$3, operation_time=$4, lease_until=$5, execution_deadline=$6, lease_ms=$7, pending_for=CASE WHEN $8=1 OR continuation_for IS NOT NULL THEN pending_for ELSE NULL END WHERE binding=$9', [generation, activationId, scheduledFor, operationTime, leaseUntil, executionDeadline, profile.leaseMs, resumed ? 1 : 0, binding]);
        return Object.freeze({ binding, activationId, generation, scheduledFor, operationTime, leaseUntil, executionDeadline, page: readPage(row.page), cursor: row.cursor === null ? null : JSON.parse(row.cursor) });
      });
    },
    async renew_delivery_activation(handle: DeliveryActivation): Promise<DeliveryActivation | null> {
      requireTransaction();
      return atomic(async () => {
        await ensure();
        const row = await current(handle); if (row === null || row.lease_ms === null) return null;
        add(await now(), Number(row.lease_ms));
        const renewalNow = '(SELECT now FROM activation_clock)';
        const proposed = isPostgres
          ? `to_char((CAST(${renewalNow} AS TIMESTAMPTZ) + lease_ms * INTERVAL '1 millisecond') AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')`
          : `strftime('%Y-%m-%dT%H:%M:%fZ', ${renewalNow}, '+' || (lease_ms / 1000.0) || ' seconds')`;
        const renewed = await execute("delivery.activation.renew", `WITH activation_clock AS MATERIALIZED (SELECT ${clock} AS now)
          UPDATE "__jadpo_delivery_activations_v1" SET lease_until=${isPostgres ? "GREATEST" : "MAX"}(lease_until, ${isPostgres ? "LEAST" : "MIN"}(${proposed}, execution_deadline))
          WHERE binding=$1 AND activation_id=$2 AND generation=$3 AND lease_ms IS NOT NULL
            AND lease_until > ${renewalNow} AND execution_deadline > ${renewalNow}
          RETURNING *, CAST(generation AS TEXT) AS generation`, [handle.binding, handle.activationId, handle.generation]);
        if (renewed.length === 0) return null;
        if (renewed.length !== 1) return fault("delivery.activation_state");
        const value = renewed[0], after = await now();
        instant(value.lease_until);
        // Driver/lock delay never revives expired authority. The durable lease
        // write may survive, but immutable page, generation and deadline do not
        // change and this caller gets no dispatch/finish authority.
        if (value.lease_until <= after || value.execution_deadline <= after) return null;
        return Object.freeze({ binding: handle.binding, activationId: value.activation_id, generation: value.generation, scheduledFor: value.scheduled_for, operationTime: value.operation_time, leaseUntil: value.lease_until, executionDeadline: value.execution_deadline, page: readPage(value.page), cursor: value.cursor === null ? null : JSON.parse(value.cursor) });
      });
    },
    async stage_delivery_activation_page(handle: DeliveryActivation, intentIds: readonly string[], after: unknown): Promise<boolean> {
      requireTransaction();
      const page = readPage(JSON.stringify({ intentIds, after }));
      return atomic(async () => {
        const row = await current(handle); if (row === null) return false;
        // A staged page is immutable until fenced completion. Re-enrollment
        // after crash cannot replace or silently advance its work list.
        if (row.page !== null) return fault("delivery.activation_page_staged");
        const changed = await execute("delivery.activation.page", `WITH activation_clock AS MATERIALIZED (SELECT ${clock} AS now)
          UPDATE "__jadpo_delivery_activations_v1" SET page=$1 WHERE binding=$2 AND activation_id=$3 AND generation=$4
            AND lease_until > (SELECT now FROM activation_clock) AND execution_deadline > (SELECT now FROM activation_clock)
          RETURNING binding`, [JSON.stringify(page), handle.binding, handle.activationId, handle.generation]);
        return changed.length === 1;
      });
    },
    async finish_delivery_activation(handle: DeliveryActivation): Promise<boolean> {
      requireTransaction();
      return atomic(async () => {
        const row = await current(handle); if (row === null) return false;
        const page = readPage(row.page); if (page === null) return fault("delivery.activation_page_missing");
        const continuation = page.after !== null;
        const changed = await execute("delivery.activation.finish", `WITH activation_clock AS MATERIALIZED (SELECT ${clock} AS now)
          UPDATE "__jadpo_delivery_activations_v1" SET activation_id=NULL, operation_time=NULL, lease_until=NULL, execution_deadline=NULL, lease_ms=NULL, page=NULL, cursor=$1, continuation_for=$2, scheduled_for=NULL
          WHERE binding=$3 AND activation_id=$4 AND generation=$5 AND lease_until > (SELECT now FROM activation_clock) AND execution_deadline > (SELECT now FROM activation_clock)
          RETURNING binding`, [continuation ? JSON.stringify(page.after) : null, continuation ? row.scheduled_for : null, handle.binding, handle.activationId, handle.generation]);
        return changed.length === 1;
      });
    },
  };
}
