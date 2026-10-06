// Compiler-owned outbox storage. This is included inside the generated
// persistence client, so enqueue uses exactly its source transaction/adapter.
// No authored language capability or public HTTP/operator endpoint is exposed.
const deliveryOutbox = (() => {
  const fault = (operation: string): never => {
    throw new PersistenceFault(operation, "driver", new Error("invalid durable delivery contract"));
  };
  const canonicalPayload = (value: unknown): string => {
    const visit = (item: unknown, parents: Set<object>): unknown => {
      if (item === null || typeof item === "string" || typeof item === "boolean") return item;
      if (typeof item === "number" && Number.isFinite(item)) return item;
      if (typeof item !== "object" || item === null || parents.has(item)) return fault("delivery.payload");
      if (!Array.isArray(item) && Object.getPrototypeOf(item) !== Object.prototype && Object.getPrototypeOf(item) !== null) return fault("delivery.payload");
      parents.add(item);
      try {
        if (Array.isArray(item)) {
          // JSON would silently turn holes into null and discard extra keys.
          // Only dense, ordinary data arrays belong to this payload boundary.
          if (Object.getPrototypeOf(item) !== Array.prototype || Reflect.ownKeys(item).length !== item.length + 1) return fault("delivery.payload");
          const result: unknown[] = [];
          for (let index = 0; index < item.length; index++) {
            const descriptor = Object.getOwnPropertyDescriptor(item, String(index));
            if (descriptor === undefined || !("value" in descriptor)) return fault("delivery.payload");
            result.push(visit(descriptor.value, parents));
          }
          return result;
        }
        const result: Record<string, unknown> = Object.create(null);
        for (const key of Object.keys(item).sort()) result[key] = visit((item as Record<string, unknown>)[key], parents);
        return result;
      } finally { parents.delete(item); }
    };
    return JSON.stringify(visit(value, new Set()));
  };
  const requireTransaction = (): void => {
    if (!transactional || !transactionState.active) fault("delivery.transaction_required");
  };
  const execute = async (operation: string, sql: string, values: unknown[] = []): Promise<any[]> => {
    const checkLifetime = (): void => { if (transactional) requireTransaction(); };
    checkLifetime();
    if (postgres !== null) {
      const created = /^CREATE TABLE IF NOT EXISTS "(__jadpo_[A-Za-z0-9_]+)"/u.exec(sql);
      if (created !== null) {
        // Avoid hot-path DDL locks entirely. IF NOT EXISTS alone does not
        // serialize concurrent first creation in PostgreSQL's type catalogue.
        const present = await persistenceAsync("delivery.schema.present", () => { checkLifetime(); return postgres!.unsafe('SELECT to_regclass($1) IS NOT NULL AS present', [`"${created[1]}"`]); });
        if (present.length !== 1) return fault("delivery.schema.cardinality");
        if (present[0].present === true) return [];
      }
      if (created !== null || /^ALTER TABLE "__jadpo_[A-Za-z0-9_]+"/u.test(sql)) {
        // Compiler-owned transaction lock: no session lock, no blocking
        // acquisition or new wait default. A losing transaction may restart
        // only through the existing bounded no-commit/rollback retry policy.
        const lock = await persistenceAsync("delivery.schema.admission", () => { checkLifetime(); return postgres!.unsafe('SELECT pg_try_advisory_xact_lock(1245791312, 1145392214) AS acquired'); });
        if (lock.length !== 1) return fault("delivery.schema.cardinality");
        if (lock[0].acquired !== true) { const refusal = new PersistenceFault("delivery.schema.admission", "driver", new Error("native delivery schema busy")); deliverySchemaRefusals.add(refusal); throw refusal; }
      }
    }
    return postgres !== null
      ? persistenceAsync(operation, () => { checkLifetime(); return postgres!.unsafe(sql, values); })
      : persistenceSync(operation, () => { checkLifetime(); const statement = sqlite!.prepare(sql); try { return statement.all(...values); } finally { statement.finalize(); } });
  };
  const ensureSchema = async (): Promise<void> => {
    requireTransaction();
    await execute("delivery.schema.keys", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_keys_v1" (
      "job" TEXT NOT NULL, "ordering_key" TEXT NOT NULL, "next_sequence" BIGINT NOT NULL CHECK ("next_sequence" >= 0),
      PRIMARY KEY ("job", "ordering_key")
    )`);
    await execute("delivery.schema.intents", `CREATE TABLE IF NOT EXISTS "__jadpo_deliveries_v1" (
      "intent_id" TEXT PRIMARY KEY, "job" TEXT NOT NULL, "source_operation" TEXT NOT NULL,
      "source_entity" TEXT NOT NULL, "source_entity_id" TEXT NOT NULL, "source_revision" TEXT NOT NULL,
      "ordering_key" TEXT NOT NULL, "key_sequence" BIGINT NOT NULL CHECK ("key_sequence" > 0),
      "payload_version" TEXT NOT NULL, "payload" TEXT NOT NULL,
      "state" TEXT NOT NULL CHECK ("state" IN ('pending', 'running', 'retry_wait', 'succeeded', 'failed', 'cancelled', 'outcome_unknown')),
      "enqueued_at" TEXT NOT NULL,
      UNIQUE ("job", "source_operation", "source_entity", "source_entity_id", "source_revision"),
      UNIQUE ("job", "ordering_key", "key_sequence")
    )`);
  };
  const checkedText = (value: unknown): string => typeof value === "string" && value.length > 0 ? value : fault("delivery.identity");
  const publicRecord = (row: any) => Object.freeze({
    intentId: row.intent_id as string, job: row.job as string,
    sourceOperation: row.source_operation as string, sourceEntity: row.source_entity as string,
    sourceEntityId: row.source_entity_id as string, sourceRevision: row.source_revision as string,
    orderingKey: row.ordering_key as string, keySequence: String(row.key_sequence),
    payloadVersion: row.payload_version as string, payload: JSON.parse(row.payload),
    state: row.state as string, enqueuedAt: row.enqueued_at as string,
  });
  // Business schedule identity is separate from ordinary row/change-log
  // revisions, timestamps, worker fences and compiler source revisions.
  // These private host primitives are not authored mutation hooks yet. The
  // checked generator must bind entity/id and invoke them in the owning create
  // or supplied-due patch transaction; no schedule activation is supplied here.
  const ensureScheduleRevisions = async (): Promise<void> => {
    requireTransaction();
    await execute("delivery.schedule.schema", `CREATE TABLE IF NOT EXISTS "__jadpo_delivery_schedules_v1" (
      "source_entity" TEXT NOT NULL, "source_entity_id" TEXT NOT NULL,
      "schedule_revision" BIGINT NOT NULL CHECK ("schedule_revision" > 0 AND "schedule_revision" <= 9223372036854775807),
      PRIMARY KEY ("source_entity", "source_entity_id")
    )`);
  };
  const scheduleRevision = (rows: any[], missingAllowed = false): string | null => {
    if (rows.length === 0 && missingAllowed) return null;
    if (rows.length !== 1) return fault("delivery.schedule_cardinality");
    const revision = rows[0].schedule_revision;
    if (typeof revision !== "string" || !/^[1-9][0-9]{0,18}$/u.test(revision) || BigInt(revision) > 9223372036854775807n) return fault("delivery.schedule_revision");
    return revision;
  };
  type DeliverySource = Readonly<{ job: string; sourceOperation: string; sourceEntity: string; sourceEntityId: string; sourceRevision: string; orderingKey: string; payloadVersion: string }>;
  // Lexically private construction boundary. Only compiler-emitted callers can
  // construct a request with the allocated ID or validate retained immutable
  // bytes. No callback/key/retain-original switch is exposed on the host API.
  const enqueueIntent = async (spec: DeliverySource, constructPayload: (id: string, original: string | null) => string) => {
    requireTransaction();
    const job = checkedText(spec.job), operation = checkedText(spec.sourceOperation), entity = checkedText(spec.sourceEntity);
    const entityId = checkedText(spec.sourceEntityId), revision = checkedText(spec.sourceRevision);
    const key = checkedText(spec.orderingKey), version = checkedText(spec.payloadVersion);
    if (!/^[1-9][0-9]*$/u.test(revision)) fault("delivery.identity");
    return client.transaction(async () => {
      await ensureSchema();
      await execute("delivery.key.create", 'INSERT INTO "__jadpo_delivery_keys_v1" ("job", "ordering_key", "next_sequence") VALUES ($1, $2, 0) ON CONFLICT ("job", "ordering_key") DO NOTHING', [job, key]);
      // Held through owning COMMIT, including after this savepoint releases.
      await execute("delivery.key.lock", 'UPDATE "__jadpo_delivery_keys_v1" SET "next_sequence" = "next_sequence" WHERE "job" = $1 AND "ordering_key" = $2', [job, key]);
      const existing = await execute("delivery.identity.read", 'SELECT *, CAST("key_sequence" AS TEXT) AS "key_sequence" FROM "__jadpo_deliveries_v1" WHERE "job" = $1 AND "source_operation" = $2 AND "source_entity" = $3 AND "source_entity_id" = $4 AND "source_revision" = $5', [job, operation, entity, entityId, revision]);
      if (existing.length > 1) fault("delivery.identity_cardinality");
      if (existing.length === 1) {
        const row = existing[0];
        if (row.ordering_key !== key || row.payload_version !== version || row.payload !== constructPayload(row.intent_id, row.payload)) fault("delivery.identity_conflict");
        return publicRecord(row);
      }
      const sequence = await execute("delivery.key.sequence", 'UPDATE "__jadpo_delivery_keys_v1" SET "next_sequence" = "next_sequence" + 1 WHERE "job" = $1 AND "ordering_key" = $2 AND "next_sequence" < 9223372036854775807 RETURNING CAST("next_sequence" AS TEXT) AS "key_sequence"', [job, key]);
      if (sequence.length !== 1) fault("delivery.sequence_exhausted");
      const intentId = crypto.randomUUID(), payload = constructPayload(intentId, null);
      const databaseTime = postgres !== null ? "to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')" : "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";
      const rows = await execute("delivery.enqueue", `INSERT INTO "__jadpo_deliveries_v1" ("intent_id", "job", "source_operation", "source_entity", "source_entity_id", "source_revision", "ordering_key", "key_sequence", "payload_version", "payload", "state", "enqueued_at") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', ${databaseTime}) RETURNING *, CAST("key_sequence" AS TEXT) AS "key_sequence"`, [intentId, job, operation, entity, entityId, revision, key, sequence[0].key_sequence, version, payload]);
      if (rows.length !== 1) fault("delivery.enqueue_cardinality");
      return publicRecord(rows[0]);
    });
  };
  return {
    // __JADPO_BOUND_DELIVERY_METHODS__
    ...createDeliveryClaimPrimitives(execute, requireTransaction, postgres !== null, work => client.transaction(() => work())),
    ...createDeliverySchedulePrimitives(execute, requireTransaction, postgres !== null, work => client.transaction(() => work())),
    async establish_delivery_schedule_revision(sourceEntity: string, sourceEntityId: string): Promise<string> {
      requireTransaction();
      const entity = checkedText(sourceEntity), identity = checkedText(sourceEntityId);
      return client.transaction(async () => {
        await ensureScheduleRevisions();
        // Never reset an existing revision, including retained terminal history.
        const rows = await execute("delivery.schedule.create", `INSERT INTO "__jadpo_delivery_schedules_v1"
          ("source_entity", "source_entity_id", "schedule_revision") VALUES ($1, $2, 1)
          ON CONFLICT ("source_entity", "source_entity_id") DO NOTHING
          RETURNING CAST("schedule_revision" AS TEXT) AS "schedule_revision"`, [entity, identity]);
        if (rows.length === 0) return fault("delivery.schedule_identity_conflict");
        return scheduleRevision(rows)!;
      });
    },
    async advance_delivery_schedule_revision(sourceEntity: string, sourceEntityId: string): Promise<string> {
      requireTransaction();
      const entity = checkedText(sourceEntity), identity = checkedText(sourceEntityId);
      return client.transaction(async () => {
        await ensureScheduleRevisions();
        // A single adapter-owned write serializes concurrent successful patches.
        // Equality of supplied due values must not erase a business revision.
        const rows = await execute("delivery.schedule.advance", `UPDATE "__jadpo_delivery_schedules_v1"
          SET "schedule_revision" = "schedule_revision" + 1
          WHERE "source_entity" = $1 AND "source_entity_id" = $2 AND "schedule_revision" < 9223372036854775807
          RETURNING CAST("schedule_revision" AS TEXT) AS "schedule_revision"`, [entity, identity]);
        if (rows.length === 0) return fault("delivery.schedule_missing_or_exhausted");
        return scheduleRevision(rows)!;
      });
    },
    async read_delivery_schedule_revision(sourceEntity: string, sourceEntityId: string): Promise<string | null> {
      requireTransaction();
      const entity = checkedText(sourceEntity), identity = checkedText(sourceEntityId);
      return client.transaction(async () => {
        // A caught driver fault must not leave PostgreSQL's source transaction
        // aborted while the enclosing callback reports successful source work.
        await ensureScheduleRevisions();
        const rows = await execute("delivery.schedule.read", `SELECT CAST("schedule_revision" AS TEXT) AS "schedule_revision"
          FROM "__jadpo_delivery_schedules_v1" WHERE "source_entity" = $1 AND "source_entity_id" = $2`, [entity, identity]);
        return scheduleRevision(rows, true);
      });
    },
    async enqueue_delivery_intent(spec: Readonly<{ job: string; sourceOperation: string; sourceEntity: string; sourceEntityId: string; sourceRevision: string; orderingKey: string; payloadVersion: string; payload: unknown }>) {
      // Generic foundation keeps strict caller-payload equality; only the
      // generated bound constructor may reuse validated original bytes.
      return enqueueIntent(spec, () => canonicalPayload(spec.payload));
    },
    async read_delivery_intent(intentId: string) {
      const rows = await execute("delivery.read", 'SELECT *, CAST("key_sequence" AS TEXT) AS "key_sequence" FROM "__jadpo_deliveries_v1" WHERE "intent_id" = $1', [checkedText(intentId)]);
      if (rows.length > 1) fault("delivery.read_cardinality");
      return rows.length === 0 ? null : publicRecord(rows[0]);
    },
  };
})();
