// Private storage primitives only: no authored handler, policy bypass, provider
// dispatch, scheduler or operator endpoint is supplied by this foundation.
type DeliveryClaimLimits = Readonly<{ maxInvocations: number; lifetimeMs: number; executionMs: number; leaseMs: number }>;
type DeliveryClaimHandle = Readonly<{ intentId: string; claimId: string; generation: string; invocation: number; firstClaimAt: string; lifetimeDeadline: string; executionDeadline: string; leaseUntil: string }>;
type DeliveryMailNoEffect = Readonly<{ failureClass: "service_unavailable" | "recipient_rejected"; providerAttempts: number; retryDelayMs: number }>;
function createDeliveryClaimPrimitives(
  execute: (operation: string, sql: string, values?: unknown[]) => Promise<any[]>,
  requireTransaction: () => void,
  isPostgres: boolean,
  atomic: <T>(work: () => Promise<T>) => Promise<T>,
) {
  const invalid = (operation: string): never => { throw new PersistenceFault(operation, "driver", new Error("invalid durable claim contract")); };
  const checkedLimits = (limits: DeliveryClaimLimits): void => {
    for (const value of [limits.maxInvocations, limits.lifetimeMs, limits.executionMs, limits.leaseMs]) {
      if (!Number.isSafeInteger(value) || value <= 0) invalid("delivery.claim_limits");
    }
    if (limits.maxInvocations > 2147483647 || limits.leaseMs > limits.executionMs) invalid("delivery.claim_limits");
  };
  const utcAfter = (instant: string, milliseconds: number): string => {
    const value = Date.parse(instant) + milliseconds;
    if (!Number.isSafeInteger(value) || value > 253402300799999) return invalid("delivery.claim_limits");
    return new Date(value).toISOString();
  };
  const databaseTimeExpression = isPostgres ? "to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')" : "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";
  const timeAfterSql = (instant: string, milliseconds: string): string => isPostgres
    ? `to_char((CAST(${instant} AS TIMESTAMPTZ) + ${milliseconds} * INTERVAL '1 millisecond') AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')`
    : `strftime('%Y-%m-%dT%H:%M:%fZ', ${instant}, '+' || (${milliseconds} / 1000.0) || ' seconds')`;
  const earliestSql = (left: string, right: string): string => `${isPostgres ? "LEAST" : "MIN"}(${left}, ${right})`;
  const databaseNow = async (): Promise<string> => {
    const rows = await execute("delivery.claim.time", `SELECT ${databaseTimeExpression} AS "now"`);
    if (rows.length !== 1 || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(rows[0].now)) return invalid("delivery.claim.time");
    return rows[0].now;
  };
  const ensureClaims = async (): Promise<void> => {
    await execute("delivery.claim.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_claims_v1" (
      "intent_id" TEXT PRIMARY KEY NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"),
      "generation" BIGINT NOT NULL CHECK ("generation" >= 0), "invocations" INTEGER NOT NULL CHECK ("invocations" >= 0),
      "max_invocations" INTEGER NOT NULL CHECK ("max_invocations" > 0),
      "lifetime_ms" BIGINT NOT NULL CHECK ("lifetime_ms" > 0), "execution_ms" BIGINT NOT NULL CHECK ("execution_ms" > 0),
      "lease_ms" BIGINT NOT NULL CHECK ("lease_ms" > 0 AND "lease_ms" <= "execution_ms"),
      "first_claim_at" TEXT, "lifetime_deadline" TEXT,
      "claim_id" TEXT, "execution_deadline" TEXT, "lease_until" TEXT,
      "possible_dispatch" INTEGER NOT NULL CHECK ("possible_dispatch" IN (0, 1))
    )`);
  };
  const lockIntent = async (intentId: string): Promise<any | null> => {
    requireTransaction();
    if (typeof intentId !== "string" || intentId.length === 0) return invalid("delivery.claim_identity");
    const initial = await execute("delivery.claim.intent", 'SELECT *, CAST("key_sequence" AS TEXT) AS "key_sequence" FROM "__jadpo_deliveries_v1" WHERE "intent_id" = $1', [intentId]);
    if (initial.length > 1) return invalid("delivery.claim_cardinality");
    if (initial.length === 0) return null;
    const locked = await execute("delivery.claim.key_lock", 'UPDATE "__jadpo_delivery_keys_v1" SET "next_sequence" = "next_sequence" WHERE "job" = $1 AND "ordering_key" = $2 RETURNING "ordering_key"', [initial[0].job, initial[0].ordering_key]);
    if (locked.length !== 1) return invalid("delivery.claim_key");
    const current = await execute("delivery.claim.intent", 'SELECT *, CAST("key_sequence" AS TEXT) AS "key_sequence" FROM "__jadpo_deliveries_v1" WHERE "intent_id" = $1', [intentId]);
    if (current.length !== 1) return invalid("delivery.claim_cardinality");
    return current[0];
  };
  const readLedger = async (intentId: string): Promise<any | null> => {
    const rows = await execute("delivery.claim.ledger", 'SELECT *, CAST("generation" AS TEXT) AS "generation" FROM "__jadpo_delivery_claims_v1" WHERE "intent_id" = $1', [intentId]);
    if (rows.length > 1) return invalid("delivery.claim_cardinality");
    return rows[0] ?? null;
  };
  const setState = async (intentId: string, state: string): Promise<void> => {
    await execute("delivery.claim.state", 'UPDATE "__jadpo_deliveries_v1" SET "state" = $1 WHERE "intent_id" = $2', [state, intentId]);
  };
  const ensureOutcomes = async (): Promise<void> => {
    await execute("delivery.attempt.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_attempts_v1" (
      "intent_id" TEXT NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"), "generation" BIGINT NOT NULL,
      "claim_id" TEXT NOT NULL, "invocation" INTEGER NOT NULL, "provider_attempts" INTEGER NOT NULL,
      "failure_class" TEXT NOT NULL, "observed_at" TEXT NOT NULL, "retry_not_before" TEXT,
      PRIMARY KEY ("intent_id", "generation")
    )`);
    await execute("delivery.dead_letter.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_dead_letters_v1" (
      "intent_id" TEXT PRIMARY KEY NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"),
      "payload_version" TEXT NOT NULL, "generation" BIGINT NOT NULL, "invocations" INTEGER NOT NULL,
      "provider_attempts" BIGINT NOT NULL, "failure_class" TEXT NOT NULL, "observed_at" TEXT NOT NULL
    )`);
  };
  const deadLetter = async (intent: any, ledger: any, failureClass: string, observedAt: string): Promise<void> => {
    // Safe classification/count metadata only: never payloads, credentials or
    // arbitrary host/provider diagnostic strings. No operator replay is added.
    await execute("delivery.dead_letter.record", `INSERT INTO "__jadpo_delivery_dead_letters_v1"
      ("intent_id", "payload_version", "generation", "invocations", "provider_attempts", "failure_class", "observed_at")
      VALUES ($1, $2, $3, $4, (SELECT COALESCE(SUM("provider_attempts"), 0) FROM "__jadpo_delivery_attempts_v1" WHERE "intent_id" = $1), $5, $6)`,
      [intent.intent_id, intent.payload_version, ledger.generation, ledger.invocations, failureClass, observedAt]);
  };
  const failExhausted = async (intent: any, ledger: any, observedAt: string): Promise<void> => atomic(async () => {
    await ensureOutcomes();
    await deadLetter(intent, ledger, "budget_exhausted", observedAt);
    await setState(intent.intent_id, "failed");
  });
  const transitionCurrentEffect = async (handle: DeliveryClaimHandle, state: "succeeded" | "outcome_unknown"): Promise<any | null> => {
    const intent = await lockIntent(handle.intentId);
    if (intent === null || intent.state !== "running") return null;
    await ensureClaims();
    const now = '(SELECT "now" FROM "outcome_clock")';
    const updated = await execute("delivery.outcome.state", `WITH "outcome_clock" AS MATERIALIZED (SELECT ${databaseTimeExpression} AS "now")
      UPDATE "__jadpo_deliveries_v1" SET "state" = $1 WHERE "intent_id" = $2 AND "state" = 'running'
        AND EXISTS (SELECT 1 FROM "__jadpo_delivery_claims_v1" WHERE "intent_id" = $2 AND "claim_id" = $3 AND "generation" = $4
          AND "possible_dispatch" = 1 AND "lease_until" > ${now} AND "execution_deadline" > ${now} AND "lifetime_deadline" > ${now})
      RETURNING "intent_id", "payload_version", ${now} AS "observed_at"`, [state, handle.intentId, handle.claimId, handle.generation]);
    if (updated.length > 1) return invalid("delivery.claim_cardinality");
    return updated[0] ?? null;
  };
  const primitives = {
    async record_delivery_mail_no_effect(handle: DeliveryClaimHandle, outcome: DeliveryMailNoEffect): Promise<"retry_wait" | "failed" | "cancelled" | null> {
      requireTransaction();
      // Reference adapter result only, not proof that an arbitrary host actually
      // received it. Public workers must bind this seam to checked provenance.
      if (outcome === null || typeof outcome !== "object" || Object.getPrototypeOf(outcome) !== Object.prototype || Reflect.ownKeys(outcome).length !== 3) return invalid("delivery.no_effect_outcome");
      const values = ["failureClass", "providerAttempts", "retryDelayMs"].map(key => Object.getOwnPropertyDescriptor(outcome, key));
      if (values.some(field => !field || !("value" in field))) return invalid("delivery.no_effect_outcome");
      const [failureClass, providerAttempts, retryDelayMs] = values.map(field => field!.value);
      if (!["service_unavailable", "recipient_rejected"].includes(failureClass) || !Number.isSafeInteger(providerAttempts) || providerAttempts < 0 || providerAttempts > 3 || !Number.isSafeInteger(retryDelayMs) || retryDelayMs < 0 || failureClass === "recipient_rejected" && retryDelayMs !== 0) return invalid("delivery.no_effect_outcome");
      return atomic(async () => {
        const intent = await lockIntent(handle.intentId);
        if (intent === null || intent.state !== "running") return null;
        await ensureClaims();
        await ensureOutcomes();
        await execute("delivery.cancel.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_cancellations_v1" (
          "intent_id" TEXT PRIMARY KEY NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"), "requested_at" TEXT NOT NULL
        )`);
        utcAfter(await databaseNow(), retryDelayMs);
        const now = '(SELECT "now" FROM "no_effect_clock")';
        // Reset possible dispatch only on a current, live claim and a trusted
        // known-no-effect classification, never merely on a timeout or expiry.
        const updated = await execute("delivery.no_effect.checkpoint", `WITH "no_effect_clock" AS MATERIALIZED (SELECT ${databaseTimeExpression} AS "now")
          UPDATE "__jadpo_delivery_claims_v1" SET "possible_dispatch" = 0
          WHERE "intent_id" = $1 AND "claim_id" = $2 AND "generation" = $3
            AND "lease_until" > ${now} AND "execution_deadline" > ${now} AND "lifetime_deadline" > ${now}
          RETURNING *, CAST("generation" AS TEXT) AS "generation", ${now} AS "observed_at"`, [handle.intentId, handle.claimId, handle.generation]);
        if (updated.length > 1) return invalid("delivery.claim_cardinality");
        if (updated.length === 0) return null;
        const ledger = updated[0], observedAt = ledger.observed_at;
        const requested = await execute("delivery.cancel.read", 'SELECT "intent_id" FROM "__jadpo_delivery_cancellations_v1" WHERE "intent_id" = $1', [handle.intentId]);
        const retryAt = utcAfter(observedAt, retryDelayMs);
        const state = requested.length > 0 ? "cancelled" : failureClass === "recipient_rejected" || Number(ledger.invocations) >= Number(ledger.max_invocations) || retryAt >= ledger.lifetime_deadline ? "failed" : "retry_wait";
        await execute("delivery.attempt.record", `INSERT INTO "__jadpo_delivery_attempts_v1"
          ("intent_id", "generation", "claim_id", "invocation", "provider_attempts", "failure_class", "observed_at", "retry_not_before")
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`, [handle.intentId, ledger.generation, handle.claimId, ledger.invocations, providerAttempts, failureClass, observedAt, state === "retry_wait" ? retryAt : null]);
        if (state === "failed") await deadLetter(intent, ledger, failureClass === "recipient_rejected" ? failureClass : "budget_exhausted", observedAt);
        await setState(handle.intentId, state);
        return state;
      });
    },
    async acknowledge_delivery_mail(handle: DeliveryClaimHandle, receipt: Readonly<{ accepted_at: string }>, compilerObservedAt: string | null = null): Promise<boolean> {
      requireTransaction();
      // Reference SERVICE-001 receipt only. The future compiler worker must
      // supply a checked adapter result; this private host seam is not authored
      // permission to forge a successful provider outcome.
      if (receipt === null || typeof receipt !== "object" || Object.getPrototypeOf(receipt) !== Object.prototype || Reflect.ownKeys(receipt).length !== 1) return invalid("delivery.mail_receipt");
      const field = Object.getOwnPropertyDescriptor(receipt, "accepted_at");
      const acceptedAt = field && "value" in field ? field.value : null;
      if (typeof acceptedAt !== "string" || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(acceptedAt)) return invalid("delivery.mail_receipt");
      const milliseconds = Date.parse(acceptedAt);
      if (!Number.isFinite(milliseconds) || new Date(milliseconds).toISOString() !== acceptedAt) return invalid("delivery.mail_receipt");
      // The generated worker captures this at the adapter's validated receipt,
      // not at a possibly delayed completion transaction. It is private native
      // observation data, never provider time or fencing/deadline authority.
      if (compilerObservedAt !== null && (typeof compilerObservedAt !== "string" || !Number.isFinite(Date.parse(compilerObservedAt)) || new Date(Date.parse(compilerObservedAt)).toISOString() !== compilerObservedAt)) return invalid("delivery.receipt_observation");
      // A savepoint makes state + receipt atomic even if a trusted host catches
      // the method's storage fault and continues its surrounding transaction.
      return atomic(async () => {
        await execute("delivery.receipt.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_mail_receipts_v1" (
          "intent_id" TEXT PRIMARY KEY NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"),
          "claim_id" TEXT NOT NULL, "generation" BIGINT NOT NULL, "payload_version" TEXT NOT NULL,
          "accepted_at" TEXT NOT NULL, "observed_at" TEXT NOT NULL
        )`);
        const completed = await transitionCurrentEffect(handle, "succeeded");
        if (completed === null) return false;
        await execute("delivery.receipt.record", 'INSERT INTO "__jadpo_delivery_mail_receipts_v1" ("intent_id", "claim_id", "generation", "payload_version", "accepted_at", "observed_at") VALUES ($1, $2, $3, $4, $5, $6)',
          [handle.intentId, handle.claimId, handle.generation, completed.payload_version, acceptedAt, compilerObservedAt ?? completed.observed_at]);
        return true;
      });
    },
    async record_delivery_unknown(handle: DeliveryClaimHandle): Promise<boolean> {
      requireTransaction();
      return (await transitionCurrentEffect(handle, "outcome_unknown")) !== null;
    },
    async cancel_delivery_intent(intentId: string): Promise<"missing" | "terminal" | "cancelled" | "requested"> {
      requireTransaction();
      return atomic(async (): Promise<"missing" | "terminal" | "cancelled" | "requested"> => {
        const intent = await lockIntent(intentId);
        if (intent === null) return "missing";
        if (["succeeded", "failed", "cancelled"].includes(intent.state)) return "terminal";
        await ensureClaims();
        const ledger = await readLedger(intentId);
        const possibleDispatch = ledger !== null && Number(ledger.possible_dispatch) === 1;
        if (["running", "outcome_unknown"].includes(intent.state) && ledger === null) return invalid("delivery.claim_missing_ledger");
        if (intent.state === "outcome_unknown" && !possibleDispatch || ["pending", "retry_wait"].includes(intent.state) && possibleDispatch) return invalid("delivery.cancel_state");
        await execute("delivery.cancel.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_cancellations_v1" (
          "intent_id" TEXT PRIMARY KEY NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"), "requested_at" TEXT NOT NULL
        )`);
        await execute("delivery.cancel.request", `INSERT INTO "__jadpo_delivery_cancellations_v1" ("intent_id", "requested_at")
          VALUES ($1, ${databaseTimeExpression}) ON CONFLICT ("intent_id") DO NOTHING`, [intentId]);
        // The key lock makes cancellation and dispatch checkpoint mutually
        // ordered. After possible dispatch, only record a request: never assert
        // rollback, refund budgets or release the unknown/FIFO block.
        if (possibleDispatch) return "requested";
        await setState(intentId, "cancelled");
        return "cancelled";
      });
    },
    async claim_delivery_resolved_replay(intentId: string, limits: DeliveryClaimLimits): Promise<DeliveryClaimHandle | null> {
      requireTransaction(); checkedLimits(limits);
      const intent = await lockIntent(intentId);
      if (intent === null || intent.state !== "retry_wait") return null;
      // A late allocation can enter retry_wait without any adapter evidence.
      // Missing evidence refuses; never create it, reset state or refund limits.
      for (const table of ["__jadpo_delivery_claims_v1", "__jadpo_delivery_attempts_v1"]) {
        const present = await execute("delivery.replay.schema", isPostgres
          ? 'SELECT to_regclass($1) IS NOT NULL AS present'
          : "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=$1) AS present", [isPostgres ? `"${table}"` : table]);
        if (present.length !== 1 || ![true, 1].includes(present[0].present)) return null;
      }
      const ledger = await readLedger(intentId);
      const integer = (value: unknown): number | null => {
        if (typeof value !== "number" && !(typeof value === "string" && /^(0|[1-9][0-9]*)$/u.test(value))) return null;
        const parsed = Number(value); return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
      };
      if (ledger === null || integer(ledger.possible_dispatch) !== 0 || integer(ledger.invocations) === null
          || integer(ledger.invocations)! <= 0 || integer(ledger.invocations)! >= limits.maxInvocations
          || typeof ledger.claim_id !== "string" || ledger.claim_id.length === 0) return null;
      const instant = (value: unknown): value is string => typeof value === "string" && Number.isFinite(Date.parse(value)) && new Date(Date.parse(value)).toISOString() === value;
      const generation = (value: unknown): value is string => typeof value === "string" && /^[1-9][0-9]*$/u.test(value) && value.length <= 19 && BigInt(value) <= 9223372036854775807n;
      if (!generation(ledger.generation) || integer(ledger.max_invocations) !== limits.maxInvocations
          || integer(ledger.lifetime_ms) !== limits.lifetimeMs || integer(ledger.execution_ms) !== limits.executionMs || integer(ledger.lease_ms) !== limits.leaseMs
          || !instant(ledger.first_claim_at) || !instant(ledger.lifetime_deadline) || !instant(ledger.execution_deadline) || !instant(ledger.lease_until)
          || ledger.first_claim_at > ledger.execution_deadline || ledger.execution_deadline > ledger.lifetime_deadline || ledger.lease_until > ledger.execution_deadline) return null;
      const now = await databaseNow();
      if (ledger.lifetime_deadline <= now) return null;
      const outcomes = await execute("delivery.replay.evidence", 'SELECT *, CAST("generation" AS TEXT) AS "generation" FROM "__jadpo_delivery_attempts_v1" WHERE "intent_id"=$1 ORDER BY "__jadpo_delivery_attempts_v1"."generation" DESC LIMIT 1', [intentId]);
      if (outcomes.length !== 1) return null;
      const attempt = outcomes[0], count = integer(attempt.provider_attempts);
      if (!generation(attempt.generation) || attempt.generation !== ledger.generation || attempt.claim_id !== ledger.claim_id
          || integer(attempt.invocation) !== integer(ledger.invocations) || attempt.failure_class !== "service_unavailable"
          || count === null || count < 1 || count > 3 || !instant(attempt.observed_at) || !instant(attempt.retry_not_before)
          || attempt.observed_at < ledger.first_claim_at || attempt.retry_not_before < attempt.observed_at || attempt.retry_not_before > now) return null;
      // The key lock remains held through ordinary admission/allocation, which
      // rechecks immutable profile, FIFO, backoff and fresh database clock.
      return primitives.claim_delivery_intent(intentId, limits);
    },
    async claim_delivery_intent(intentId: string, limits: DeliveryClaimLimits): Promise<DeliveryClaimHandle | null> {
      requireTransaction(); checkedLimits(limits);
      const intent = await lockIntent(intentId);
      if (intent === null || !["pending", "running", "retry_wait"].includes(intent.state)) return null;
      await ensureClaims();
      await ensureOutcomes();
      const now = await databaseNow();
      let ledger = await readLedger(intentId);
      if (ledger !== null) {
        if (String(ledger.max_invocations) !== String(limits.maxInvocations) || String(ledger.lifetime_ms) !== String(limits.lifetimeMs) || String(ledger.execution_ms) !== String(limits.executionMs) || String(ledger.lease_ms) !== String(limits.leaseMs)) invalid("delivery.claim_limits_conflict");
        if (intent.state === "running") {
          if (ledger.lease_until > now && ledger.execution_deadline > now && ledger.lifetime_deadline > now) return null;
          if (Number(ledger.possible_dispatch) === 1) { await setState(intentId, "outcome_unknown"); return null; }
        }
        if (ledger.lifetime_deadline !== null && ledger.lifetime_deadline <= now || Number(ledger.invocations) >= limits.maxInvocations) { await failExhausted(intent, ledger, now); return null; }
      } else if (intent.state !== "pending") return invalid("delivery.claim_missing_ledger");
      const earlier = await execute("delivery.claim.fifo", 'SELECT "intent_id" FROM "__jadpo_deliveries_v1" WHERE "job" = $1 AND "ordering_key" = $2 AND "key_sequence" < $3 AND "state" IN (\'pending\', \'running\', \'retry_wait\', \'outcome_unknown\') LIMIT 1', [intent.job, intent.ordering_key, intent.key_sequence]);
      if (earlier.length !== 0) return null;
      if (intent.state === "retry_wait") {
        const waiting = await execute("delivery.retry.wait", 'SELECT "intent_id" FROM "__jadpo_delivery_attempts_v1" WHERE "intent_id" = $1 AND "generation" = $2 AND "retry_not_before" > $3', [intentId, ledger.generation, now]);
        if (waiting.length !== 0) return null;
      }
      if (ledger === null) {
        // Register immutable limits, but start the lifetime only when the first
        // allocation actually executes, not before earlier awaited statements.
        utcAfter(now, limits.lifetimeMs); utcAfter(now, limits.executionMs); utcAfter(now, limits.leaseMs);
        await execute("delivery.claim.first", 'INSERT INTO "__jadpo_delivery_claims_v1" ("intent_id", "generation", "invocations", "max_invocations", "lifetime_ms", "execution_ms", "lease_ms", "possible_dispatch") VALUES ($1, 0, 0, $2, $3, $4, $5, 0)', [intentId, limits.maxInvocations, limits.lifetimeMs, limits.executionMs, limits.leaseMs]);
        ledger = await readLedger(intentId);
      }
      const allocationNow = '(SELECT "now" FROM "claim_clock")';
      const lifetimeDeadlineSql = `COALESCE("lifetime_deadline", ${timeAfterSql(allocationNow, '"lifetime_ms"')})`;
      const executionDeadlineSql = earliestSql(timeAfterSql(allocationNow, '"execution_ms"'), lifetimeDeadlineSql);
      const leaseUntilSql = earliestSql(timeAfterSql(allocationNow, '"lease_ms"'), executionDeadlineSql);
      const claimId = crypto.randomUUID();
      const claimed = await execute("delivery.claim.allocate", `WITH "claim_clock" AS MATERIALIZED (SELECT ${databaseTimeExpression} AS "now")
        UPDATE "__jadpo_delivery_claims_v1" SET "generation" = "generation" + 1, "invocations" = "invocations" + 1,
          "claim_id" = $1, "first_claim_at" = COALESCE("first_claim_at", ${allocationNow}),
          "lifetime_deadline" = ${lifetimeDeadlineSql}, "execution_deadline" = ${executionDeadlineSql}, "lease_until" = ${leaseUntilSql}
        WHERE "intent_id" = $2 AND "generation" < 9223372036854775807 AND "invocations" < "max_invocations"
          AND "possible_dispatch" = 0 AND ("lifetime_deadline" IS NULL OR "lifetime_deadline" > ${allocationNow})
          AND NOT EXISTS (SELECT 1 FROM "__jadpo_delivery_attempts_v1" AS "last_outcome"
            WHERE "last_outcome"."intent_id" = $2 AND "last_outcome"."generation" = "__jadpo_delivery_claims_v1"."generation" AND "last_outcome"."retry_not_before" > ${allocationNow})
        RETURNING *, CAST("generation" AS TEXT) AS "generation"`, [claimId, intentId]);
      if (claimed.length === 0) {
        // A previously eligible reclaim may have reached its immutable lifetime
        // while waiting. Do not charge a new invocation or return an expired one.
        const stoppedAt = await databaseNow();
        const current = await readLedger(intentId);
        if (current !== null && (current.lifetime_deadline !== null && current.lifetime_deadline <= stoppedAt || Number(current.invocations) >= limits.maxInvocations)) { await failExhausted(intent, current, stoppedAt); return null; }
        if ((await execute("delivery.retry.wait", 'SELECT "intent_id" FROM "__jadpo_delivery_attempts_v1" WHERE "intent_id" = $1 AND "generation" = $2 AND "retry_not_before" > $3', [intentId, current.generation, stoppedAt])).length !== 0) return null;
        return invalid("delivery.claim_allocation_rejected");
      }
      if (claimed.length !== 1) return invalid("delivery.claim_cardinality");
      await setState(intentId, "running");
      const row = claimed[0];
      const admittedAt = await databaseNow();
      if (row.lease_until <= admittedAt || row.execution_deadline <= admittedAt || row.lifetime_deadline <= admittedAt) {
        // Allocation may execute while eligible but return late. Its fence and
        // charged invocation remain durable; neither budget is reset or refunded.
        if (row.lifetime_deadline <= admittedAt || Number(row.invocations) >= limits.maxInvocations) await failExhausted(intent, row, admittedAt);
        else await setState(intentId, "retry_wait");
        return null;
      }
      return Object.freeze({ intentId, claimId, generation: String(row.generation), invocation: Number(row.invocations), firstClaimAt: row.first_claim_at, lifetimeDeadline: row.lifetime_deadline, executionDeadline: row.execution_deadline, leaseUntil: row.lease_until });
    },
    async renew_delivery_claim(handle: DeliveryClaimHandle): Promise<DeliveryClaimHandle | null> {
      requireTransaction();
      const intent = await lockIntent(handle.intentId);
      if (intent === null || intent.state !== "running") return null;
      await ensureClaims();
      const renewalNow = '(SELECT "now" FROM "renewal_clock")';
      const renewedLease = earliestSql(timeAfterSql(renewalNow, '"lease_ms"'), earliestSql('"execution_deadline"', '"lifetime_deadline"'));
      const renewed = await execute("delivery.claim.renew", `WITH "renewal_clock" AS MATERIALIZED (SELECT ${databaseTimeExpression} AS "now")
        UPDATE "__jadpo_delivery_claims_v1" SET "lease_until" = ${renewedLease}
        WHERE "intent_id" = $1 AND "claim_id" = $2 AND "generation" = $3
          AND "lease_until" > ${renewalNow} AND "execution_deadline" > ${renewalNow} AND "lifetime_deadline" > ${renewalNow}
        RETURNING *, CAST("generation" AS TEXT) AS "generation"`, [handle.intentId, handle.claimId, handle.generation]);
      if (renewed.length === 0) return null;
      if (renewed.length !== 1) return invalid("delivery.claim_cardinality");
      const row = renewed[0], after = await databaseNow();
      // A committed renewal never resets an invocation or absolute deadline.
      // A late response does not confer authority, even if its write survived.
      if (row.lease_until <= after || row.execution_deadline <= after || row.lifetime_deadline <= after) return null;
      return Object.freeze({ intentId: handle.intentId, claimId: row.claim_id, generation: String(row.generation), invocation: Number(row.invocations), firstClaimAt: row.first_claim_at, lifetimeDeadline: row.lifetime_deadline, executionDeadline: row.execution_deadline, leaseUntil: row.lease_until });
    },
    async checkpoint_delivery_dispatch(handle: DeliveryClaimHandle): Promise<boolean> {
      requireTransaction();
      const intent = await lockIntent(handle.intentId);
      if (intent === null || intent.state !== "running") return false;
      await ensureClaims();
      const ledger = await readLedger(handle.intentId);
      const now = await databaseNow();
      if (ledger === null || ledger.claim_id !== handle.claimId || String(ledger.generation) !== handle.generation || Number(ledger.possible_dispatch) !== 0 || ledger.lease_until <= now || ledger.execution_deadline <= now || ledger.lifetime_deadline <= now) return false;
      const updated = await execute("delivery.dispatch.checkpoint", `UPDATE "__jadpo_delivery_claims_v1" SET "possible_dispatch" = 1 WHERE "intent_id" = $1 AND "claim_id" = $2 AND "generation" = $3 AND "possible_dispatch" = 0 AND "lease_until" > ${databaseTimeExpression} AND "execution_deadline" > ${databaseTimeExpression} AND "lifetime_deadline" > ${databaseTimeExpression} RETURNING "intent_id"`, [handle.intentId, handle.claimId, handle.generation]);
      // A statement may finish after the lease. The durable marker remains
      // conservative evidence, but the late caller receives no send authority.
      const after = await databaseNow();
      return updated.length === 1 && ledger.lease_until > after && ledger.execution_deadline > after && ledger.lifetime_deadline > after;
    },
  };
  return primitives;
}
