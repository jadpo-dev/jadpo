import { afterAll, describe, expect, test } from "bun:test";
import { Database, constants } from "bun:sqlite";
import { SQL } from "bun";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createConnection, createServer, type AddressInfo, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { nativeContentionSnapshot, runNativeContentionWave } from "./fixtures/transaction-contention-worker";

// Only the dedicated verifier-owned disposable database may select PostgreSQL.
// Ambient application connection strings are never consumed by this suite.
const postgresUrl = Bun.env.JADPO_VALIDATION_PERSISTENCE_DATABASE_URL;
delete Bun.env.DATABASE_URL;
delete Bun.env.SQLITE_PATH;
const root = mkdtempSync(join(tmpdir(), "jadpo-validation-persistence-"));
const sqlitePath = join(root, "application.sqlite");
if (postgresUrl) Bun.env.DATABASE_URL = postgresUrl;
else Bun.env.SQLITE_PATH = sqlitePath;
const compiler = Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
const source = `
type Template = Object { note: Email? }
type Linked = Object { note: Template.note }
entity Entry {
    id: Uuid
    label: Text
    note: Linked.note
    identity: id
    persistence { store: primary role: authority }
    cache hot { store: redis from: primary strategy: invalidate delivery: durable }
    action create_failing(input: CreateEntry)
        fails EntryConflict, DeliberateFailure
        -> Entry
    {
        var inserted = attempt create Entry {
            id: input.id label: input.label note: input.note
        } conflict: EntryConflict
        reject DeliberateFailure
    }
action insert_entry(input: CreateEntry) fails EntryConflict -> Entry {
    return attempt create Entry { id: input.id label: input.label note: input.note } conflict: EntryConflict
}
query find_entry(input: EntryLookup) freshness: authoritative fails EntryMissing -> Entry {
    return attempt query required Entry { where: id == input.id missing: EntryMissing }
}
action patch_entry(input: PatchRequest) fails EntryMissing, EntryConflict, PatchEmpty -> Entry {
    var changes: EntryPatch = input.changes
    return attempt update required Entry {
        where: id == input.id patch: changes
        empty: PatchEmpty missing: EntryMissing conflict: EntryConflict
    }
}
action update_label(input: BatchUpdate) fails EntryMissing, EntryConflict -> Entry {
    return attempt update required Entry {
        where: label == input.label set: { note: input.note }
        missing: EntryMissing conflict: EntryConflict
    }
}
action insert_pair(input: Pair) fails EntryConflict -> Entry {
    var first = attempt Entry.insert_entry(input.first)
    return attempt Entry.insert_entry(input.second)
}
action shadow_patch(id: Entry.id, changes: NotePatch, replacement: EntryPatch)
    fails EntryMissing, EntryConflict, PatchEmpty -> Entry
{
    if true {
        var changes: EntryPatch = replacement
        return attempt update required Entry {
            where: id == id patch: changes
            empty: PatchEmpty missing: EntryMissing conflict: EntryConflict
        }
    }
    return attempt update required Entry {
        where: id == id patch: changes
        empty: PatchEmpty missing: EntryMissing conflict: EntryConflict
    }
}
}
failure EntryConflict { kind: Conflict code: "entry_conflict" }
failure EntryMissing { kind: NotFound code: "entry_missing" }
failure PatchEmpty { kind: InvalidValue code: "patch_empty" }
failure DeliberateFailure { kind: Rejected code: "deliberate_failure" }
type CreateEntry = Object { id: Entry.id label: Entry.label note: Entry.note }
type EntryLookup = Object { id: Entry.id }
type EntryPatch = Object { label: Entry.label optional note: Entry.note optional }
type PatchRequest = Object { id: Entry.id changes: EntryPatch }
type NotePatch = Object { note: Entry.note optional }
type ShadowRequest = Object { id: Entry.id original: NotePatch replacement: EntryPatch }
type BatchUpdate = Object { label: Entry.label note: Entry.note }
type Pair = Object { first: CreateEntry second: CreateEntry }
action propagated_pair(input: Pair)
    consistency: atomic
    fails EntryConflict, DeliberateFailure -> Entry
{
    var first = attempt Entry.insert_entry(input.first)
    return attempt Entry.create_failing(input.second)
}
action recovered_pair(input: Pair)
    consistency: atomic
    fails EntryConflict -> Entry
{
    var first = attempt Entry.insert_entry(input.first)
    var recovered = match Entry.create_failing(input.second) {
        success(value) => value
        failure DeliberateFailure => first
        failure EntryConflict => propagate
    }
    return first
}
route POST /entries { auth: none input: CreateEntry output: Entry run: Entry.insert_entry(input) }
route POST /entries-deadline { auth: none deadline: 200ms input: CreateEntry output: Entry run: Entry.insert_entry(input) }
route POST /inline-deadline {
    auth: none deadline: 200ms input: EntryLookup output: Entry
    action: fails EntryMissing {
        var first = attempt query required Entry { where: id == input.id missing: EntryMissing }
        return attempt query required Entry { where: id == input.id missing: EntryMissing }
    }
}
route POST /pair { auth: none input: Pair output: Entry run: Entry.insert_pair(input) }
route POST /find { auth: none input: EntryLookup output: Entry run: Entry.find_entry(input) }
query lookup_entry(input: EntryLookup) freshness: authoritative fails EntryMissing -> Entry { return attempt Entry.find_entry(input) }
route POST /lookup { auth: none input: EntryLookup output: Entry run: lookup_entry(input) }
action lookup_action(input: EntryLookup) fails EntryMissing -> Entry { return attempt Entry.find_entry(input) }
route POST /lookup-action { auth: none input: EntryLookup output: Entry run: lookup_action(input) }
route POST /patch { auth: none input: PatchRequest output: Entry run: Entry.patch_entry(input) }
route POST /shadow { auth: none input: ShadowRequest output: Entry run: Entry.shadow_patch(input.id, input.original, input.replacement) }
route POST /batch { auth: none input: BatchUpdate output: Entry run: Entry.update_label(input) }
route POST /propagate { auth: none input: Pair output: Entry run: propagated_pair(input) }
route POST /recover { auth: none input: Pair output: Entry run: recovered_pair(input) }
`;
writeFileSync(join(root, "app.jadpo"), source);
const build = Bun.spawnSync([compiler, "build", root], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Persistence contract failed to build:\n${build.stdout}\n${build.stderr}`);
// Expose the actual generated client only in this disposable test output for
// deterministic control-query fault injection; no production seam is emitted.
if (Bun.env.JADPO_CONTENTION_CAPTURE) cpSync(join(root, "build"), join(Bun.env.JADPO_CONTENTION_CAPTURE, "pristine-build"), { recursive: true });
const persistencePath = join(root, "build/target/persistence.ts");
const nativeContentionPristinePersistence = readFileSync(persistencePath, "utf8");
writeFileSync(persistencePath, readFileSync(persistencePath, "utf8") + "\nexport const __controlQueryProbeClient = createPersistenceClient;\n");
const app = await import(pathToFileURL(join(root, "build/target/app.ts")).href);
const persistenceModule = await import(pathToFileURL(join(root, "build/target/persistence.ts")).href);
const sql = postgresUrl ? new SQL(postgresUrl, { prepare: false }) : null;
const sqlite = postgresUrl ? null : new Database(sqlitePath, { strict: true });
afterAll(async () => {
  sqlite?.close();
  await sql?.close();
  delete Bun.env.DATABASE_URL;
  delete Bun.env.SQLITE_PATH;
  rmSync(root, { recursive: true, force: true });
});
let sequence = 1;
function entry(label: string, note: string | null = "valid@example.com") {
  return { id: `018f57d0-bf42-4f25-9417-${String(sequence++).padStart(12, "0")}`, label, note };
}
async function request(path: string, body: unknown, signal?: AbortSignal) {
  return app.handleRequest(new Request(`https://persistence.test${path}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body), signal,
  }));
}
async function rows(query: string, values: unknown[] = []): Promise<any[]> {
  return sql ? Array.from(await sql.unsafe(query, values)) : sqlite!.query(query).all(...values);
}
async function stored(id: string) {
  return rows('SELECT id, label, note FROM "entry" WHERE id = $1', [id]);
}
async function insert(value: ReturnType<typeof entry>) {
  const response = await request("/entries", value);
  expect(response.status, await response.clone().text()).toBe(200);
  expect(await response.json()).toEqual(value);
  expect(await stored(value.id)).toEqual([value]);
}
async function expectOperationalFailure(response: Response) {
  expect(response.status).toBe(500);
  const body = await response.json();
  expect(body.error.code).toBe("internal_fault");
  expect(body.error).not.toHaveProperty("stack");
  expect(body.error).not.toHaveProperty("details");
  expect(JSON.stringify(body)).not.toContain(sqlitePath);
}

describe(`inherited nullable persistence and transaction contracts (${postgresUrl ? "postgres" : "sqlite"})`, () => {
  // RM-306 storage-primitive evidence only. These host-side calls exercise the
  // generated adapter, not authored job scheduling/dispatch or catalog closure.
  const outboxSpec = (value: ReturnType<typeof entry>, revision = "1") => ({
    job: "validation_delivery", sourceOperation: "Entry.validation_update", sourceEntity: "Entry",
    sourceEntityId: value.id, sourceRevision: revision, orderingKey: value.id, payloadVersion: "1",
    payload: { recipient: "person@example.test", entryId: value.id, label: value.label },
  });
  // Private schedule storage only: host-controlled calls below are not evidence
  // that authored create/patch hooks or the generated worker already exist.
  test("business schedule revision shares source commit/rollback and rejects expired or nontransactional clients", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("schedule-commit");
    let retired: any;
    const revision = await persistence.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (tx: any) => {
      retired = tx;
      await tx.create_Entry(value);
      return tx.establish_delivery_schedule_revision("Entry", value.id);
    });
    expect(revision).toBe("1");
    expect(await stored(value.id)).toEqual([value]);
    const aborted = entry("schedule-rollback");
    await expect(persistence.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (tx: any) => {
      await tx.create_Entry(aborted);
      await tx.establish_delivery_schedule_revision("Entry", aborted.id);
      throw new Error("abort schedule source");
    })).rejects.toThrow("abort schedule source");
    expect(await stored(aborted.id)).toEqual([]);
    expect(await persistence.transaction((tx: any) => tx.read_delivery_schedule_revision("Entry", aborted.id))).toBeNull();
    for (const client of [persistence, retired]) {
      await expect(client.establish_delivery_schedule_revision("Entry", entry("invalid-client").id)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
      await expect(client.advance_delivery_schedule_revision("Entry", value.id)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
      await expect(client.read_delivery_schedule_revision("Entry", value.id)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    }
  });
  test("schedule revision advances for each caller-proved supplied mutation and never resets on duplicate creation", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("schedule-equal-aba");
    await persistence.transaction((tx: any) => tx.establish_delivery_schedule_revision("Entry", value.id));
    // Same-value and A->B->A logical schedules still consume distinct revisions.
    const revisions: string[] = [];
    for (const _value of ["A", "A", "B", "A"]) {
      revisions.push(await persistence.transaction((tx: any) => tx.advance_delivery_schedule_revision("Entry", value.id)));
    }
    expect(revisions).toEqual(["2", "3", "4", "5"]);
    await expect(persistence.transaction((tx: any) => tx.establish_delivery_schedule_revision("Entry", value.id))).rejects.toMatchObject({ operation: "delivery.schedule_identity_conflict" });
    expect(await persistence.transaction((tx: any) => tx.read_delivery_schedule_revision("Entry", value.id))).toBe("5");
    await expect(persistence.transaction(async (tx: any) => {
      expect(await tx.advance_delivery_schedule_revision("Entry", value.id)).toBe("6");
      throw new Error("failed supplied patch");
    })).rejects.toThrow("failed supplied patch");
    expect(await persistence.transaction((tx: any) => tx.read_delivery_schedule_revision("Entry", value.id))).toBe("5");
    // A transaction with no supplied due change does not touch private state.
    await persistence.transaction(async () => undefined);
    expect(await persistence.transaction((tx: any) => tx.read_delivery_schedule_revision("Entry", value.id))).toBe("5");
    await expect(persistence.transaction((tx: any) => tx.advance_delivery_schedule_revision("Entry", entry("not-created").id))).rejects.toMatchObject({ operation: "delivery.schedule_missing_or_exhausted" });
  });
  test("schedule revisions serialize races and survive restart separately from ordinary change revisions", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("schedule-race-restart");
    await persistence.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (tx: any) => {
      await tx.create_Entry(value);
      await tx.establish_delivery_schedule_revision("Entry", value.id);
    });
    const advanced = await Promise.all([0, 1].map(() => persistence.transaction((tx: any) => tx.advance_delivery_schedule_revision("Entry", value.id))));
    expect(advanced.sort()).toEqual(["2", "3"]);
    expect(await rows('SELECT CAST(revision AS TEXT) AS revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id])).toEqual([{ revision: "1" }]);
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", `const {persistence}=await import(${JSON.stringify(pathToFileURL(persistencePath).href)});console.log(JSON.stringify(await persistence.transaction(tx=>tx.read_delivery_schedule_revision("Entry",${JSON.stringify(value.id)}))));process.exit(0);`], { env: { ...Bun.env, DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath }, stdout: "pipe", stderr: "pipe" });
    const stdout = await new Response(child.stdout).text();
    const stderr = await new Response(child.stderr).text();
    expect(await child.exited, stderr).toBe(0);
    expect(JSON.parse(stdout)).toBe("3");
  });
  test("schedule revisions preserve full int64 identity and fail closed at exhaustion", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("schedule-int64");
    await persistence.transaction((tx: any) => tx.establish_delivery_schedule_revision("Entry", value.id));
    await rows('UPDATE "__jadpo_delivery_schedules_v1" SET schedule_revision = 9007199254740992 WHERE source_entity_id = $1', [value.id]);
    expect(await persistence.transaction((tx: any) => tx.advance_delivery_schedule_revision("Entry", value.id))).toBe("9007199254740993");
    await rows('UPDATE "__jadpo_delivery_schedules_v1" SET schedule_revision = 9223372036854775807 WHERE source_entity_id = $1', [value.id]);
    await expect(persistence.transaction((tx: any) => tx.advance_delivery_schedule_revision("Entry", value.id))).rejects.toMatchObject({ operation: "delivery.schedule_missing_or_exhausted" });
    expect(await persistence.transaction((tx: any) => tx.read_delivery_schedule_revision("Entry", value.id))).toBe("9223372036854775807");
  });
  test("caught schedule read driver faults recover source work instead of falsely acknowledging rollback", async () => {
    const persistence = persistenceModule.persistence as any;
    const factory = (persistenceModule as any).__controlQueryProbeClient;
    let injected = 0;
    const isRead = (statement: string) => statement.startsWith('SELECT CAST("schedule_revision" AS TEXT)') && statement.includes('"__jadpo_delivery_schedules_v1"');
    let controlled: any;
    if (sql !== null) {
      const connection = new Proxy(sql, { get(target, property) {
        if (property === "begin") return (work: (tx: any) => Promise<any>) => target.begin(tx => work(new Proxy(tx, { get(inner, key) {
          if (key === "unsafe") return (statement: string, values: any[]) => {
            if (isRead(statement)) { injected++; return inner.unsafe("SELECT 1 / 0", []); }
            return inner.unsafe(statement, values);
          };
          const value = Reflect.get(inner, key, inner); return typeof value === "function" ? value.bind(inner) : value;
        } })));
        const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value;
      } });
      controlled = factory(connection, null);
    } else {
      const connection = new Proxy(sqlite!, { get(target, property) {
        if (property === "prepare") return (statement: string) => {
          if (isRead(statement)) { injected++; return target.prepare("SELECT missing_schedule_column"); }
          return target.prepare(statement);
        };
        const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value;
      } });
      controlled = factory(null, connection);
    }
    const value = entry("schedule-read-recovery");
    const result = await controlled.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (tx: any) => {
      await tx.create_Entry(value);
      await tx.establish_delivery_schedule_revision("Entry", value.id);
      await expect(tx.read_delivery_schedule_revision("Entry", value.id)).rejects.toMatchObject({ operation: "delivery.schedule.read" });
      return "source_kept";
    });
    expect(injected).toBe(1);
    expect(result).toBe("source_kept");
    expect(await stored(value.id)).toEqual([value]);
    expect(await persistence.transaction((tx: any) => tx.read_delivery_schedule_revision("Entry", value.id))).toBe("1");
  });
  test("compiler-owned outbox shares source commit and rollback and refuses untransactional enqueue", async () => {
    const persistence = persistenceModule.persistence as any;
    const committed = entry("outbox-atomic-commit");
    const queued = await persistence.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (transaction: any) => {
      await transaction.create_Entry(committed);
      return transaction.enqueue_delivery_intent(outboxSpec(committed));
    });
    expect(queued.intentId).toMatch(/^[0-9a-f-]{36}$/);
    expect(queued.state).toBe("pending");
    expect(queued.keySequence).toBe("1");
    expect(queued.enqueuedAt).toMatch(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/);
    expect(await stored(committed.id)).toEqual([committed]);
    expect((await persistence.read_delivery_intent(queued.intentId)).intentId).toBe(queued.intentId);
    const aborted = entry("outbox-atomic-rollback");
    await expect(persistence.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (transaction: any) => {
      await transaction.create_Entry(aborted);
      await transaction.enqueue_delivery_intent(outboxSpec(aborted));
      throw new Error("abort source transaction");
    })).rejects.toThrow("abort source transaction");
    expect(await stored(aborted.id)).toEqual([]);
    expect(await rows('SELECT intent_id FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1', [aborted.id])).toEqual([]);
    expect(await rows('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1', [aborted.id])).toEqual([]);
    await expect(persistence.enqueue_delivery_intent(outboxSpec(entry("not-transactional"))))
      .rejects.toMatchObject({ operation: "delivery.transaction_required" });
  });
  test("outbox identity and payload survive a fresh process and duplicate enqueue cannot rewrite them", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("outbox-restart");
    const spec = outboxSpec(value);
    const first = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(spec));
    const duplicate = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent({ ...spec, payload: { label: value.label, entryId: value.id, recipient: "person@example.test" } }));
    expect(duplicate).toEqual(first);
    for (const change of [{ payloadVersion: "2" }, { orderingKey: "different-key" }, { payload: { ...spec.payload, label: "replacement" } }]) {
      await expect(persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent({ ...spec, ...change })))
        .rejects.toMatchObject({ operation: "delivery.identity_conflict" });
    }
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", `const {persistence}=await import(${JSON.stringify(pathToFileURL(persistencePath).href)});console.log(JSON.stringify(await persistence.read_delivery_intent(${JSON.stringify(first.intentId)})));process.exit(0);`], { env: { ...Bun.env, DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath }, stdout: "pipe", stderr: "pipe" });
    const stdout = await new Response(child.stdout).text();
    const stderr = await new Response(child.stderr).text();
    expect(await child.exited, stderr).toBe(0);
    expect(JSON.parse(stdout)).toEqual(first);
    expect(await rows('SELECT intent_id FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1', [value.id])).toHaveLength(1);
  });
  test("outbox clients expire after commit and rollback, including views during a later transaction", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const rollback of [false, true]) {
      const value = entry(`outbox-expiry-${rollback}`);
      let retained: any;
      let view: any;
      const source = persistence.transaction(async (transaction: any) => {
        retained = transaction;
        view = transaction.withOperationTime("2026-10-04T08:00:00.000Z").withSignal(null).withDeadline(null);
        await transaction.enqueue_delivery_intent(outboxSpec(value));
        if (rollback) throw new Error("retire via rollback");
      });
      if (rollback) await expect(source).rejects.toThrow("retire via rollback");
      else await source;
      const rejectStale = async () => {
        for (const expired of [retained, view]) {
          await expect(expired.enqueue_delivery_intent(outboxSpec(value, "2")))
            .rejects.toMatchObject({ operation: "delivery.transaction_required" });
          await expect(expired.transaction(async () => "invalid"))
            .rejects.toMatchObject({ operation: "transaction.inactive" });
        }
      };
      await rejectStale();
      const valid = await persistence.transaction(async (transaction: any) => {
        await rejectStale();
        await expect(transaction.transaction(async (nested: any) => {
          await nested.enqueue_delivery_intent(outboxSpec(value, "3"));
          throw new Error("rollback nested intent");
        })).rejects.toThrow("rollback nested intent");
        return transaction.transaction((nested: any) => nested.enqueue_delivery_intent(outboxSpec(value, "4")));
      });
      expect(valid.keySequence).toBe(rollback ? "1" : "2");
      expect(await rows('SELECT source_revision FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1 ORDER BY key_sequence', [value.id]))
        .toEqual(rollback ? [{ source_revision: "4" }] : [{ source_revision: "1" }, { source_revision: "4" }]);
      expect(await rows('SELECT CAST(next_sequence AS TEXT) AS next_sequence FROM "__jadpo_delivery_keys_v1" WHERE job = $1 AND ordering_key = $2', ["validation_delivery", value.id]))
        .toEqual([{ next_sequence: rollback ? "1" : "2" }]);
    }
  });
  test("a retired retry attempt cannot enqueue in its replacement transaction", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("outbox-retry-expiry");
    const cause: any = new Error("injected retry retirement");
    if (postgresUrl) cause.errno = "40001";
    else cause.code = "SQLITE_BUSY";
    let retired: any;
    let attempts = 0;
    const result = await persistence.transaction(async (transaction: any) => {
      attempts++;
      if (attempts === 1) {
        retired = transaction.withOperationTime("2026-10-04T08:00:00.000Z");
        await transaction.enqueue_delivery_intent(outboxSpec(value));
        throw new persistenceModule.PersistenceFault("transaction.retry_probe", "driver", cause);
      }
      await expect(retired.enqueue_delivery_intent(outboxSpec(value, "2")))
        .rejects.toMatchObject({ operation: "delivery.transaction_required" });
      return transaction.enqueue_delivery_intent(outboxSpec(value, "3"));
    }, { replayable: true, operationId: "outbox-retry-expiry", operation: "validation.outbox_retry", startedAt: performance.now(), signal: null });
    expect(attempts).toBe(2);
    expect(result.keySequence).toBe("1");
    await expect(retired.enqueue_delivery_intent(outboxSpec(value, "4")))
      .rejects.toMatchObject({ operation: "delivery.transaction_required" });
    expect(await rows('SELECT source_revision FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1', [value.id]))
      .toEqual([{ source_revision: "3" }]);
  });
  test("outbox payload rejects sparse and unsupported arrays without aliasing explicit null", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("outbox-array-shapes");
    const spec = outboxSpec(value);
    const extra = Object.assign([null], { extra: true });
    const accessor = [null];
    Object.defineProperty(accessor, "0", { get: () => null });
    for (const payload of [Array(1), [undefined], extra, accessor]) {
      await expect(persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent({ ...spec, payload })))
        .rejects.toMatchObject({ operation: "delivery.payload" });
      expect(await rows('SELECT intent_id FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1', [value.id])).toEqual([]);
      expect(await rows('SELECT next_sequence FROM "__jadpo_delivery_keys_v1" WHERE job = $1 AND ordering_key = $2', [spec.job, value.id])).toEqual([]);
    }
    const accepted = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent({ ...spec, payload: [null] }));
    expect(accepted.payload).toEqual([null]);
    expect(accepted.keySequence).toBe("1");
    expect(await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent({ ...spec, payload: [null] }))).toEqual(accepted);
  });
  test("same-key outbox enqueue ordering is serialized through source commit", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("outbox-commit-order");
    let release!: () => void;
    let inserted!: () => void;
    const hold = new Promise<void>(resolve => { release = resolve; });
    const ready = new Promise<void>(resolve => { inserted = resolve; });
    const first = persistence.transaction(async (transaction: any) => { const result = await transaction.enqueue_delivery_intent(outboxSpec(value, "1")); inserted(); await hold; return result; });
    await ready;
    let secondCommitted = false;
    const second = persistence.transaction(async (transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "2"))).then((result: any) => { secondCommitted = true; return result; });
    try {
      await Bun.sleep(25);
      expect(secondCommitted).toBe(false);
    } finally { release(); }
    const [a, b] = await Promise.all([first, second]);
    expect(a.keySequence).toBe("1");
    expect(b.keySequence).toBe("2");
    expect(a.intentId).not.toBe(b.intentId);
  });
  const claimLimits = { maxInvocations: 3, lifetimeMs: 600000, executionMs: 30000, leaseMs: 10000 };
  const claimIntent = (intentId: string, limits = claimLimits) => (persistenceModule.persistence as any).transaction((transaction: any) => transaction.claim_delivery_intent(intentId, limits));
  const checkpoint = (handle: any) => (persistenceModule.persistence as any).transaction((transaction: any) => transaction.checkpoint_delivery_dispatch(handle));
  const expireClaim = (intentId: string) => rows('UPDATE "__jadpo_delivery_claims_v1" SET lease_until = $1 WHERE intent_id = $2', ["2000-01-01T00:00:00.000Z", intentId]);
  const delayedDeliveryClient = (marker: string, phase: "before" | "after", delayMs: number) => {
    const factory = (persistenceModule as any).__controlQueryProbeClient;
    if (sql !== null) {
      const connection = new Proxy(sql, { get(target, property) {
        if (property === "begin") return (work: (tx: any) => Promise<any>) => target.begin(tx => work(new Proxy(tx, { get(inner, key) {
          if (key === "unsafe") return async (statement: string, values: any[]) => {
            if (statement.includes(marker) && phase === "before") await Bun.sleep(delayMs);
            const result = await inner.unsafe(statement, values);
            if (statement.includes(marker) && phase === "after") await Bun.sleep(delayMs);
            return result;
          };
          const value = Reflect.get(inner, key, inner); return typeof value === "function" ? value.bind(inner) : value;
        } })));
        const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value;
      } });
      return factory(connection, null);
    }
    const connection = new Proxy(sqlite!, { get(target, property) {
      if (property === "prepare") return (statement: string) => {
        const prepared = target.prepare(statement);
        if (!statement.includes(marker)) return prepared;
        return { all(...values: any[]) {
          if (phase === "before") Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, delayMs);
          const result = prepared.all(...values);
          if (phase === "after") Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, delayMs);
          return result;
        }, finalize: () => prepared.finalize() };
      };
      const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value;
    } });
    return factory(null, connection);
  };
  test("durable claims share rollback, finite immutable budgets and source-attempt lifetime", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("claim-rollback"))));
    let retired: any;
    await expect(persistence.transaction(async (transaction: any) => {
      retired = transaction;
      expect((await transaction.claim_delivery_intent(intent.intentId, claimLimits)).generation).toBe("1");
      throw new Error("rollback claim");
    })).rejects.toThrow("rollback claim");
    expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("pending");
    const claimed = await claimIntent(intent.intentId);
    expect(claimed.generation).toBe("1");
    expect(claimed.invocation).toBe(1);
    expect(Date.parse(claimed.lifetimeDeadline) - Date.parse(claimed.firstClaimAt)).toBe(claimLimits.lifetimeMs);
    expect(claimed.leaseUntil <= claimed.executionDeadline).toBe(true);
    expect(claimed.executionDeadline <= claimed.lifetimeDeadline).toBe(true);
    await expect(retired.claim_delivery_intent(intent.intentId, claimLimits)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    await expect(persistence.claim_delivery_intent(intent.intentId, claimLimits)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    await expect(claimIntent(intent.intentId, { ...claimLimits, maxInvocations: 4 })).rejects.toMatchObject({ operation: "delivery.claim_limits_conflict" });
    for (const invalid of [{ ...claimLimits, leaseMs: 30001 }, { ...claimLimits, maxInvocations: 0 }, { ...claimLimits, lifetimeMs: Infinity }]) {
      await expect(claimIntent(intent.intentId, invalid)).rejects.toMatchObject({ operation: "delivery.claim_limits" });
    }
  });
  test("concurrent durable claims admit exactly one worker and fence safe pre-dispatch reclaim", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("claim-race"))));
    const results = await Promise.all([claimIntent(intent.intentId), claimIntent(intent.intentId)]);
    expect(results.filter(Boolean)).toHaveLength(1);
    const first = results.find(Boolean)!;
    expect(first.generation).toBe("1");
    expect(await claimIntent(intent.intentId)).toBeNull();
    await expireClaim(intent.intentId);
    expect(await checkpoint(first)).toBe(false);
    const second = await claimIntent(intent.intentId);
    expect(second.generation).toBe("2");
    expect(second.invocation).toBe(2);
    expect(second.claimId).not.toBe(first.claimId);
    expect(second.firstClaimAt).toBe(first.firstClaimAt);
    expect(second.lifetimeDeadline).toBe(first.lifetimeDeadline);
    expect(await checkpoint(first)).toBe(false);
    expect(await checkpoint(second)).toBe(true);
    expect(await checkpoint(second)).toBe(false);
  });
  test("possible-dispatch expiry becomes durable unknown and blocks the next same-key intent", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("claim-unknown-fifo");
    const [first, second] = await persistence.transaction(async (transaction: any) => [await transaction.enqueue_delivery_intent(outboxSpec(value, "1")), await transaction.enqueue_delivery_intent(outboxSpec(value, "2"))]);
    expect(await claimIntent(second.intentId)).toBeNull();
    const handle = await claimIntent(first.intentId);
    expect(await checkpoint(handle)).toBe(true);
    await expireClaim(first.intentId);
    expect(await claimIntent(first.intentId)).toBeNull();
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("outcome_unknown");
    expect(await checkpoint(handle)).toBe(false);
    expect(await claimIntent(second.intentId)).toBeNull();
    // Fresh generated process uses the same authority DB: neither restart nor
    // another wake-up manufactures permission to retry an uncertain effect.
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", `const {persistence}=await import(${JSON.stringify(pathToFileURL(persistencePath).href)});console.log(JSON.stringify(await persistence.transaction(tx=>tx.claim_delivery_intent(${JSON.stringify(first.intentId)},${JSON.stringify(claimLimits)}))));process.exit(0);`], { env: { ...Bun.env, DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath }, stdout: "pipe", stderr: "pipe" });
    const output = await new Response(child.stdout).text();
    const errors = await new Response(child.stderr).text();
    expect(await child.exited, errors).toBe(0);
    expect(JSON.parse(output)).toBeNull();
    expect((await rows('SELECT invocations FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [first.intentId]))[0].invocations).toBe(1);
  });
  test("claim invocation and persisted lifetime exhaustion never reset on reclaim", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const lifetime of [false, true]) {
      const value = entry(`claim-exhaustion-${lifetime}`);
      const intents = await persistence.transaction(async (transaction: any) => [await transaction.enqueue_delivery_intent(outboxSpec(value, "1")), await transaction.enqueue_delivery_intent(outboxSpec(value, "2"))]);
      const limits = { ...claimLimits, maxInvocations: lifetime ? 3 : 1 };
      const first = await claimIntent(intents[0].intentId, limits);
      expect(first.invocation).toBe(1);
      if (lifetime) await rows('UPDATE "__jadpo_delivery_claims_v1" SET lifetime_deadline = $1 WHERE intent_id = $2', ["2000-01-01T00:00:00.000Z", first.intentId]);
      else await expireClaim(first.intentId);
      expect(await claimIntent(first.intentId, limits)).toBeNull();
      expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("failed");
      expect(await claimIntent(first.intentId, limits)).toBeNull();
      const successor = await claimIntent(intents[1].intentId, limits);
      expect(successor.invocation).toBe(1);
      expect(successor.generation).toBe("1");
      expect((await rows('SELECT invocations FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [first.intentId]))[0].invocations).toBe(1);
    }
  });
  test("dispatch checkpoints roll back and BIGINT fencing/FIFO retain precision", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("claim-precision");
    const [first, second] = await persistence.transaction(async (transaction: any) => [await transaction.enqueue_delivery_intent(outboxSpec(value, "1")), await transaction.enqueue_delivery_intent(outboxSpec(value, "2"))]);
    await rows('UPDATE "__jadpo_deliveries_v1" SET key_sequence = key_sequence + 9007199254740992 WHERE source_entity_id = $1', [value.id]);
    expect(await claimIntent(second.intentId)).toBeNull();
    const handle = await claimIntent(first.intentId);
    await expect(persistence.transaction(async (transaction: any) => {
      expect(await transaction.checkpoint_delivery_dispatch(handle)).toBe(true);
      throw new Error("rollback checkpoint");
    })).rejects.toThrow("rollback checkpoint");
    expect(Number((await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [first.intentId]))[0].possible_dispatch)).toBe(0);
    await rows('UPDATE "__jadpo_delivery_claims_v1" SET generation = 9007199254740992 WHERE intent_id = $1', [first.intentId]);
    await expireClaim(first.intentId);
    const next = await claimIntent(first.intentId);
    expect(next.generation).toBe("9007199254740993");
    expect(await checkpoint(handle)).toBe(false);
    expect(await checkpoint(next)).toBe(true);
  });
  test("a checkpoint returning after lease expiry gives no send admission but retains possible-dispatch evidence", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("late-checkpoint"))));
    const limits = { maxInvocations: 3, lifetimeMs: 60000, executionMs: 10000, leaseMs: 100 };
    const handle = await claimIntent(intent.intentId, limits);
    const originalPrepare = Database.prototype.prepare;
    if (postgresUrl) {
      await rows(`CREATE FUNCTION rm306_delay_checkpoint() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.possible_dispatch = 0 AND NEW.possible_dispatch = 1 THEN PERFORM pg_sleep(0.2); END IF; RETURN NEW; END $$`);
      await rows('CREATE TRIGGER rm306_delay_checkpoint BEFORE UPDATE ON "__jadpo_delivery_claims_v1" FOR EACH ROW EXECUTE FUNCTION rm306_delay_checkpoint()');
    } else {
      Database.prototype.prepare = function (statement: string, ...values: any[]) {
        const prepared = originalPrepare.call(this, statement, ...values);
        if (statement.startsWith('UPDATE "__jadpo_delivery_claims_v1" SET "possible_dispatch" = 1')) {
          const originalAll = prepared.all.bind(prepared);
          prepared.all = ((...args: any[]) => {
            const result = originalAll(...args);
            const until = performance.now() + 200;
            while (performance.now() < until) { /* simulate delayed native return, not a hard I/O bound */ }
            return result;
          }) as typeof prepared.all;
        }
        return prepared;
      } as typeof Database.prototype.prepare;
    }
    try {
      expect(await checkpoint(handle)).toBe(false);
      expect(Number((await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0].possible_dispatch)).toBe(1);
      expect(await claimIntent(intent.intentId, limits)).toBeNull();
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("outcome_unknown");
    } finally {
      Database.prototype.prepare = originalPrepare;
      if (postgresUrl) { await rows('DROP TRIGGER rm306_delay_checkpoint ON "__jadpo_delivery_claims_v1"'); await rows('DROP FUNCTION rm306_delay_checkpoint()'); }
    }
  });
  test("allocation uses fresh database time and rejects first or reclaimed handles returning after expiry", async () => {
    const persistence = persistenceModule.persistence as any;
    const factory = (persistenceModule as any).__controlQueryProbeClient;
    const delayedClient = (phase: "before" | "after") => {
      const isAllocation = (statement: string) => statement.includes('WITH "claim_clock"');
      if (sql !== null) {
        const connection = new Proxy(sql, { get(target, property) {
          if (property === "begin") return (work: (tx: any) => Promise<any>) => target.begin(tx => work(new Proxy(tx, { get(inner, key) {
            if (key === "unsafe") return async (statement: string, values: any[]) => {
              if (isAllocation(statement) && phase === "before") await Bun.sleep(250);
              const result = await inner.unsafe(statement, values);
              if (isAllocation(statement) && phase === "after") await Bun.sleep(250);
              return result;
            };
            const value = Reflect.get(inner, key, inner); return typeof value === "function" ? value.bind(inner) : value;
          } })));
          const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value;
        } });
        return factory(connection, null);
      }
      const connection = new Proxy(sqlite!, { get(target, property) {
        if (property === "prepare") return (statement: string) => {
          const prepared = target.prepare(statement);
          if (!isAllocation(statement)) return prepared;
          return { all(...values: any[]) {
            if (phase === "before") Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
            const result = prepared.all(...values);
            if (phase === "after") Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
            return result;
          }, finalize: () => prepared.finalize() };
        };
        const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value;
      } });
      return factory(null, connection);
    };
    const nowSql = postgresUrl ? `SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS "now"` : `SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now') AS "now"`;
    const limits = { maxInvocations: 3, lifetimeMs: 150, executionMs: 100, leaseMs: 100 };
    for (const reclaim of [false, true]) {
      for (const phase of ["before", "after"] as const) {
        const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry(`allocation-time-${reclaim}-${phase}`))));
        const first = reclaim ? await claimIntent(intent.intentId, limits) : null;
        if (reclaim) await expireClaim(intent.intentId);
        const before = (await rows(nowSql))[0].now;
        const result = await delayedClient(phase).transaction((transaction: any) => transaction.claim_delivery_intent(intent.intentId, limits));
        const ledger = (await rows('SELECT invocations, first_claim_at, lifetime_deadline FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0];
        if (!reclaim && phase === "before") {
          // First valid allocation starts lifetime now, not before the pause.
          expect(result).not.toBeNull();
          expect(Date.parse(result.firstClaimAt) - Date.parse(before)).toBeGreaterThanOrEqual(200);
          expect(result.leaseUntil > (await rows(nowSql))[0].now).toBe(true);
          expect(ledger.invocations).toBe(1);
        } else {
          expect(result).toBeNull();
          expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("failed");
          expect(ledger.invocations).toBe(reclaim && phase === "after" ? 2 : 1);
        }
        if (reclaim) {
          expect(ledger.first_claim_at).toBe(first.firstClaimAt);
          expect(ledger.lifetime_deadline).toBe(first.lifetimeDeadline);
          expect(await checkpoint(first)).toBe(false);
        }
      }
    }
    // Delays surround real adapter SQL via the disposable factory seam; this
    // is controlled native-statement timing, not a hard I/O-interruption claim.
  });
  test("lease renewal preserves fence, invocation and absolute deadlines across rollback and possible dispatch", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("renew-current"))));
    const handle = await claimIntent(intent.intentId);
    const shortenedLease = new Date(Date.parse(handle.firstClaimAt) + 2000).toISOString();
    await rows('UPDATE "__jadpo_delivery_claims_v1" SET lease_until = $1 WHERE intent_id = $2', [shortenedLease, intent.intentId]);
    await expect(persistence.renew_delivery_claim(handle)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    await expect(persistence.transaction(async (transaction: any) => {
      expect(await transaction.renew_delivery_claim(handle)).not.toBeNull();
      throw new Error("rollback renewal");
    })).rejects.toThrow("rollback renewal");
    expect((await rows('SELECT lease_until FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0].lease_until).toBe(shortenedLease);
    const renewed = await persistence.transaction((transaction: any) => transaction.renew_delivery_claim(handle));
    expect(renewed).toEqual({ ...handle, leaseUntil: renewed.leaseUntil });
    expect(renewed.leaseUntil > shortenedLease).toBe(true);
    expect(renewed.leaseUntil <= handle.executionDeadline && renewed.leaseUntil <= handle.lifetimeDeadline).toBe(true);
    expect(await checkpoint(renewed)).toBe(true);
    expect(await persistence.transaction((transaction: any) => transaction.renew_delivery_claim(renewed))).not.toBeNull();
    expect((await rows('SELECT invocations, possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0])
      .toEqual({ invocations: 1, possible_dispatch: 1 });
    expect(await checkpoint(renewed)).toBe(false);
  });
  test("lease renewal never revives expired or stale claims and stays capped by immutable execution and lifetime", async () => {
    const persistence = persistenceModule.persistence as any;
    const renew = (handle: any) => persistence.transaction((transaction: any) => transaction.renew_delivery_claim(handle));
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("renew-stale"))));
    const first = await claimIntent(intent.intentId);
    await expireClaim(intent.intentId);
    expect(await renew(first)).toBeNull();
    const current = await claimIntent(intent.intentId);
    expect(current.generation).toBe("2");
    expect(await renew(first)).toBeNull();
    expect(await renew({ ...current, claimId: crypto.randomUUID() })).toBeNull();
    const valid = await renew(current);
    expect(valid).toEqual({ ...current, leaseUntil: valid.leaseUntil });
    for (const lifetimeMs of [600000, 10000]) {
      const cappedIntent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry(`renew-cap-${lifetimeMs}`))));
      const capped = await claimIntent(cappedIntent.intentId, { ...claimLimits, lifetimeMs, executionMs: 10000, leaseMs: 10000 });
      const result = await renew(capped);
      expect(result.leaseUntil).toBe(capped.executionDeadline);
      expect(result.executionDeadline).toBe(capped.executionDeadline);
      expect(result.lifetimeDeadline).toBe(capped.lifetimeDeadline);
      expect(result.invocation).toBe(1);
    }
  });
  test("renewal samples fresh SQL time and returns no handle when real adapter SQL crosses expiry", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const phase of ["before", "after"] as const) {
      const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry(`renew-late-${phase}`))));
      const handle = await claimIntent(intent.intentId, { ...claimLimits, executionMs: 100, leaseMs: 100 });
      const result = await delayedDeliveryClient('WITH "renewal_clock"', phase, 250)
        .transaction((transaction: any) => transaction.renew_delivery_claim(handle));
      expect(result).toBeNull();
      const ledger = (await rows('SELECT invocations, CAST(generation AS TEXT) AS generation, first_claim_at, lifetime_deadline, execution_deadline, lease_until FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0];
      expect(ledger).toEqual({ invocations: 1, generation: handle.generation, first_claim_at: handle.firstClaimAt,
        lifetime_deadline: handle.lifetimeDeadline, execution_deadline: handle.executionDeadline, lease_until: handle.leaseUntil });
      expect(await checkpoint(handle)).toBe(false);
      const reclaimed = await claimIntent(intent.intentId, { ...claimLimits, executionMs: 100, leaseMs: 100 });
      expect(reclaimed.generation).toBe("2");
      expect(reclaimed.invocation).toBe(2);
    }
  });
  test("cancellation before possible dispatch is atomic, ends claims and releases same-key FIFO", async () => {
    const persistence = persistenceModule.persistence as any;
    const cancel = (id: string) => persistence.transaction((transaction: any) => transaction.cancel_delivery_intent(id));
    const value = entry("cancel-before");
    const first = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "1")));
    const second = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "2")));
    await expect(persistence.cancel_delivery_intent(first.intentId)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    // Establish the lazy table in a different key before inspecting rollback.
    const setup = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("cancel-table"))));
    expect(await cancel(setup.intentId)).toBe("cancelled");
    await expect(persistence.transaction(async (transaction: any) => {
      expect(await transaction.cancel_delivery_intent(first.intentId)).toBe("cancelled");
      throw new Error("rollback cancellation");
    })).rejects.toThrow("rollback cancellation");
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("pending");
    expect(await rows('SELECT intent_id FROM "__jadpo_delivery_cancellations_v1" WHERE intent_id = $1', [first.intentId])).toEqual([]);
    const handle = await claimIntent(first.intentId);
    expect(await cancel(first.intentId)).toBe("cancelled");
    expect(await checkpoint(handle)).toBe(false);
    expect(await persistence.transaction((transaction: any) => transaction.renew_delivery_claim(handle))).toBeNull();
    expect(await claimIntent(first.intentId)).toBeNull();
    expect(await claimIntent(second.intentId)).not.toBeNull();
    const record = (await rows('SELECT requested_at FROM "__jadpo_delivery_cancellations_v1" WHERE intent_id = $1', [first.intentId]))[0];
    expect(record.requested_at).toMatch(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u);
    expect(await cancel(first.intentId)).toBe("terminal");
    expect(await cancel(crypto.randomUUID())).toBe("missing");
    const waiting = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("cancel-retry-wait"))));
    await claimIntent(waiting.intentId);
    await rows('UPDATE "__jadpo_deliveries_v1" SET state = $1 WHERE intent_id = $2', ["retry_wait", waiting.intentId]);
    expect(await cancel(waiting.intentId)).toBe("cancelled");
    expect(await claimIntent(waiting.intentId)).toBeNull();
  });
  test("cancellation after dispatch records a durable request without claiming rollback or releasing unknown FIFO", async () => {
    const persistence = persistenceModule.persistence as any;
    const cancel = (id: string) => persistence.transaction((transaction: any) => transaction.cancel_delivery_intent(id));
    const value = entry("cancel-after");
    const first = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "1")));
    const second = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "2")));
    const handle = await claimIntent(first.intentId);
    expect(await checkpoint(handle)).toBe(true);
    await expect(persistence.transaction(async (transaction: any) => {
      expect(await transaction.cancel_delivery_intent(first.intentId)).toBe("requested");
      throw new Error("rollback request");
    })).rejects.toThrow("rollback request");
    expect(await rows('SELECT intent_id FROM "__jadpo_delivery_cancellations_v1" WHERE intent_id = $1', [first.intentId])).toEqual([]);
    expect(await cancel(first.intentId)).toBe("requested");
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("running");
    expect((await rows('SELECT invocations, possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [first.intentId]))[0])
      .toEqual({ invocations: 1, possible_dispatch: 1 });
    const request = (await rows('SELECT requested_at FROM "__jadpo_delivery_cancellations_v1" WHERE intent_id = $1', [first.intentId]))[0];
    expect(await cancel(first.intentId)).toBe("requested");
    expect((await rows('SELECT requested_at FROM "__jadpo_delivery_cancellations_v1" WHERE intent_id = $1', [first.intentId]))[0]).toEqual(request);
    await expireClaim(first.intentId);
    expect(await claimIntent(first.intentId)).toBeNull();
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("outcome_unknown");
    expect(await cancel(first.intentId)).toBe("requested");
    expect(await claimIntent(second.intentId)).toBeNull();
    const childSource = `const { persistence } = await import(${JSON.stringify(pathToFileURL(persistencePath).href)});
      console.log(JSON.stringify(await persistence.transaction(tx => tx.cancel_delivery_intent(${JSON.stringify(first.intentId)}))));`;
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", childSource], {
      env: { ...Bun.env }, stdout: "pipe", stderr: "pipe",
    });
    expect(await child.exited).toBe(0);
    expect(JSON.parse(await new Response(child.stdout).text())).toBe("requested");
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("outcome_unknown");
    expect(await claimIntent(second.intentId)).toBeNull();
  });
  test("checkpoint and cancellation serialize into safe opposite outcomes", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const dispatchFirst of [true, false]) {
      const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry(`cancel-order-${dispatchFirst}`))));
      const handle = await claimIntent(intent.intentId);
      let release!: () => void, locked!: () => void;
      const hold = new Promise<void>(resolve => { release = resolve; });
      const ready = new Promise<void>(resolve => { locked = resolve; });
      const first = persistence.transaction(async (transaction: any) => {
        const result = dispatchFirst ? await transaction.checkpoint_delivery_dispatch(handle) : await transaction.cancel_delivery_intent(intent.intentId);
        locked(); await hold; return result;
      });
      await ready;
      let secondFinished = false;
      const second = persistence.transaction((transaction: any) => dispatchFirst ? transaction.cancel_delivery_intent(intent.intentId) : transaction.checkpoint_delivery_dispatch(handle))
        .then((result: any) => { secondFinished = true; return result; });
      try { await Bun.sleep(25); expect(secondFinished).toBe(false); } finally { release(); }
      expect(await first).toBe(dispatchFirst ? true : "cancelled");
      expect(await second).toBe(dispatchFirst ? "requested" : false);
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe(dispatchFirst ? "running" : "cancelled");
    }
  });
  test("caught native cancellation-state failure rolls its request back before the outer host transaction commits", async () => {
    const persistence = persistenceModule.persistence.withOperationTime(new Date().toISOString()) as any;
    const setup = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("cancel-fault-schema"))));
    expect(await persistence.transaction((transaction: any) => transaction.cancel_delivery_intent(setup.intentId))).toBe("cancelled");
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("cancel-fault-target"))));
    if (postgresUrl) {
      await rows(`CREATE FUNCTION rm306_cancel_state_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
        IF NEW.state = 'cancelled' THEN RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'controlled cancellation fault'; END IF;
        RETURN NEW; END; $$`);
      await rows('CREATE TRIGGER rm306_cancel_state_fault BEFORE UPDATE OF state ON "__jadpo_deliveries_v1" FOR EACH ROW EXECUTE FUNCTION rm306_cancel_state_fault()');
    } else {
      await rows(`CREATE TRIGGER rm306_cancel_state_fault BEFORE UPDATE OF state ON "__jadpo_deliveries_v1"
        WHEN NEW.state = 'cancelled' BEGIN SELECT RAISE(ABORT, 'controlled cancellation fault'); END`);
    }
    const continued = entry("host-commits-after-caught-cancellation-fault");
    try {
      await persistence.transaction(async (transaction: any) => {
        await expect(transaction.cancel_delivery_intent(intent.intentId)).rejects.toMatchObject({ operation: "delivery.claim.state" });
        expect((await transaction.read_delivery_intent(intent.intentId)).state).toBe("pending");
        await transaction.create_Entry(continued);
      });
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("pending");
      expect(await rows('SELECT intent_id FROM "__jadpo_delivery_cancellations_v1" WHERE intent_id = $1', [intent.intentId])).toEqual([]);
      expect(await rows('SELECT id FROM "entry" WHERE id = $1', [continued.id])).toEqual([{ id: continued.id }]);
    } finally {
      await rows(`DROP TRIGGER rm306_cancel_state_fault${postgresUrl ? ' ON "__jadpo_deliveries_v1"' : ''}`);
      if (postgresUrl) await rows('DROP FUNCTION rm306_cancel_state_fault()');
    }
    const handle = await claimIntent(intent.intentId);
    expect(handle).not.toBeNull();
    expect(await checkpoint(handle)).toBe(true);
  });
  const mailReceipt = { accepted_at: "1999-01-01T00:00:00.000Z" };
  test("fenced mail acknowledgement commits exact receipt and authority changes together and releases only the original key", async () => {
    const persistence = persistenceModule.persistence.withOperationTime(new Date().toISOString()) as any;
    const value = entry("mail-ack");
    const first = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "1")));
    const second = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "2")));
    const handle = await claimIntent(first.intentId);
    const acknowledge = (receipt = mailReceipt) => persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(handle, receipt));
    await expect(persistence.acknowledge_delivery_mail(handle, mailReceipt)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    expect(await acknowledge()).toBe(false);
    expect(await checkpoint(handle)).toBe(true);
    const authorityChange = entry("mail-completion-authority");
    await expect(persistence.transaction(async (transaction: any) => {
      await transaction.create_Entry(authorityChange);
      expect(await transaction.acknowledge_delivery_mail(handle, mailReceipt)).toBe(true);
      throw new Error("rollback acknowledgement");
    })).rejects.toThrow("rollback acknowledgement");
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("running");
    expect(await rows('SELECT intent_id FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id = $1', [first.intentId])).toEqual([]);
    expect(await rows('SELECT id FROM "entry" WHERE id = $1', [authorityChange.id])).toEqual([]);
    expect(await claimIntent(second.intentId)).toBeNull();
    await persistence.transaction(async (transaction: any) => {
      expect(await transaction.cancel_delivery_intent(first.intentId)).toBe("requested");
      await transaction.create_Entry(authorityChange);
      expect(await transaction.acknowledge_delivery_mail(handle, mailReceipt)).toBe(true);
    });
    const receipt = (await rows('SELECT claim_id, CAST(generation AS TEXT) AS generation, payload_version, accepted_at, observed_at FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id = $1', [first.intentId]))[0];
    expect(receipt).toMatchObject({ claim_id: handle.claimId, generation: handle.generation, payload_version: first.payloadVersion, accepted_at: mailReceipt.accepted_at });
    expect(receipt.observed_at > handle.firstClaimAt || receipt.observed_at === handle.firstClaimAt).toBe(true);
    expect(receipt.observed_at).not.toBe(receipt.accepted_at);
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("succeeded");
    expect(await rows('SELECT id FROM "entry" WHERE id = $1', [authorityChange.id])).toEqual([{ id: authorityChange.id }]);
    expect(await acknowledge({ accepted_at: "2000-01-01T00:00:00.000Z" })).toBe(false);
    expect((await rows('SELECT accepted_at FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id = $1', [first.intentId]))[0].accepted_at).toBe(mailReceipt.accepted_at);
    expect(await claimIntent(second.intentId)).not.toBeNull();
    const childSource = `const { persistence } = await import(${JSON.stringify(pathToFileURL(persistencePath).href)});
      console.log(JSON.stringify(await persistence.read_delivery_intent(${JSON.stringify(first.intentId)})));`;
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", childSource], { env: { ...Bun.env }, stdout: "pipe", stderr: "pipe" });
    expect(await child.exited).toBe(0);
    expect(JSON.parse(await new Response(child.stdout).text())).toMatchObject({ intentId: first.intentId, state: "succeeded", sourceRevision: "1" });
  });
  test("mail receipt shape and stale or expired fences cannot manufacture a terminal outcome", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("mail-receipt-checks"))));
    const first = await claimIntent(intent.intentId);
    let getterCalled = false;
    const accessor = Object.defineProperty({}, "accepted_at", { get() { getterCalled = true; return mailReceipt.accepted_at; } });
    for (const receipt of [null, {}, { accepted_at: "invalid" }, { accepted_at: "2026-02-30T00:00:00.000Z" },
      { accepted_at: mailReceipt.accepted_at, raw_response: "secret" }, accessor, Object.assign(Object.create(null), mailReceipt)]) {
      await expect(persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(first, receipt)))
        .rejects.toMatchObject({ operation: "delivery.mail_receipt" });
    }
    expect(getterCalled).toBe(false);
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(first, mailReceipt))).toBe(false);
    await expireClaim(intent.intentId);
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(first, mailReceipt))).toBe(false);
    const current = await claimIntent(intent.intentId);
    expect(current.generation).toBe("2");
    expect(await checkpoint(current)).toBe(true);
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(first, mailReceipt))).toBe(false);
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail({ ...current, claimId: crypto.randomUUID() }, mailReceipt))).toBe(false);
    expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(current, mailReceipt))).toBe(true);
  });
  test("acknowledgement savepoint rolls terminal state back even when a host catches a native receipt constraint fault", async () => {
    const persistence = persistenceModule.persistence.withOperationTime(new Date().toISOString()) as any;
    const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry("mail-savepoint"))));
    const handle = await claimIntent(intent.intentId);
    // Create the lazy receipt schema without allowing completion before dispatch.
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(handle, mailReceipt))).toBe(false);
    expect(await checkpoint(handle)).toBe(true);
    // An existing test-only row forces actual adapter UNIQUE failure after the
    // state statement, rather than a fabricated persistence fault envelope.
    await rows('INSERT INTO "__jadpo_delivery_mail_receipts_v1" (intent_id, claim_id, generation, payload_version, accepted_at, observed_at) VALUES ($1, $2, 0, $3, $4, $4)',
      [intent.intentId, "test-existing-receipt", intent.payloadVersion, mailReceipt.accepted_at]);
    await persistence.transaction(async (transaction: any) => {
      await expect(transaction.acknowledge_delivery_mail(handle, mailReceipt)).rejects.toMatchObject({ operation: "delivery.receipt.record" });
      expect((await transaction.read_delivery_intent(intent.intentId)).state).toBe("running");
      await transaction.create_Entry(entry("host-continues-after-native-receipt-fault"));
    });
    expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
    expect((await rows('SELECT claim_id FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id = $1', [intent.intentId]))[0].claim_id).toBe("test-existing-receipt");
  });
  test("explicit uncertainty requires a current possible-dispatch claim and remains blocked after restart", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("explicit-unknown");
    const first = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "1")));
    const second = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(value, "2")));
    const handle = await claimIntent(first.intentId);
    const unknown = (claim = handle) => persistence.transaction((transaction: any) => transaction.record_delivery_unknown(claim));
    expect(await unknown()).toBe(false);
    expect(await checkpoint(handle)).toBe(true);
    expect(await unknown({ ...handle, generation: "2" })).toBe(false);
    await expect(persistence.transaction(async (transaction: any) => {
      expect(await transaction.record_delivery_unknown(handle)).toBe(true);
      throw new Error("rollback unknown");
    })).rejects.toThrow("rollback unknown");
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("running");
    expect(await unknown()).toBe(true);
    expect((await persistence.read_delivery_intent(first.intentId)).state).toBe("outcome_unknown");
    expect(await unknown()).toBe(false);
    expect(await claimIntent(first.intentId)).toBeNull();
    expect(await claimIntent(second.intentId)).toBeNull();
    expect(await persistence.transaction((transaction: any) => transaction.acknowledge_delivery_mail(handle, mailReceipt))).toBe(false);
    const childSource = `const { persistence } = await import(${JSON.stringify(pathToFileURL(persistencePath).href)});
      console.log(JSON.stringify({ state: (await persistence.read_delivery_intent(${JSON.stringify(first.intentId)})).state,
        successor: await persistence.transaction(tx => tx.claim_delivery_intent(${JSON.stringify(second.intentId)}, ${JSON.stringify(claimLimits)})) }));`;
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", childSource], { env: { ...Bun.env }, stdout: "pipe", stderr: "pipe" });
    expect(await child.exited).toBe(0);
    expect(JSON.parse(await new Response(child.stdout).text())).toEqual({ state: "outcome_unknown", successor: null });
  });
  test("mail outcome state admission uses fresh SQL time without losing a known eligible acknowledgement to return latency", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const phase of ["before", "after"] as const) {
      const intent = await persistence.transaction((transaction: any) => transaction.enqueue_delivery_intent(outboxSpec(entry(`mail-late-${phase}`))));
      const handle = await claimIntent(intent.intentId, { ...claimLimits, executionMs: 100, leaseMs: 100 });
      expect(await checkpoint(handle)).toBe(true);
      const result = await delayedDeliveryClient('WITH "outcome_clock"', phase, 250)
        .transaction((transaction: any) => transaction.acknowledge_delivery_mail(handle, mailReceipt));
      expect(result).toBe(phase === "after");
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe(phase === "after" ? "succeeded" : "running");
      const receipts = await rows('SELECT accepted_at FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id = $1', [intent.intentId]);
      expect(receipts).toEqual(phase === "after" ? [{ accepted_at: mailReceipt.accepted_at }] : []);
      expect(await claimIntent(intent.intentId, { ...claimLimits, executionMs: 100, leaseMs: 100 })).toBeNull();
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe(phase === "after" ? "succeeded" : "outcome_unknown");
    }
  });
  const noEffect = { failureClass: "service_unavailable", providerAttempts: 3, retryDelayMs: 60000 };
  const mailNoEffect = (handle: any, outcome: any = noEffect) => (persistenceModule.persistence as any).transaction((tx: any) => tx.record_delivery_mail_no_effect(handle, outcome));
  test("durable no-effect retry wait survives restart and retains identity budgets and FIFO", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("retry-persist");
    const [first, second] = await persistence.transaction(async (tx: any) => [await tx.enqueue_delivery_intent(outboxSpec(value, "1")), await tx.enqueue_delivery_intent(outboxSpec(value, "2"))]);
    const handle = await claimIntent(first.intentId);
    expect(await checkpoint(handle)).toBe(true);
    expect(await mailNoEffect(handle)).toBe("retry_wait");
    expect(await mailNoEffect(handle)).toBeNull();
    expect(await claimIntent(first.intentId)).toBeNull();
    expect(await claimIntent(second.intentId)).toBeNull();
    const attempt = (await rows('SELECT *, CAST(generation AS TEXT) AS generation FROM "__jadpo_delivery_attempts_v1" WHERE intent_id = $1', [first.intentId]))[0];
    expect(attempt).toMatchObject({ generation: "1", invocation: 1, provider_attempts: 3, failure_class: "service_unavailable" });
    expect(Date.parse(attempt.retry_not_before) - Date.parse(attempt.observed_at)).toBe(60000);
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", `const {persistence}=await import(${JSON.stringify(pathToFileURL(persistencePath).href)});console.log(JSON.stringify(await persistence.transaction(tx=>tx.claim_delivery_intent(${JSON.stringify(first.intentId)},${JSON.stringify(claimLimits)}))));process.exit(0);`], { env: { ...Bun.env, DATABASE_URL: postgresUrl ?? "", SQLITE_PATH: sqlitePath }, stdout: "pipe", stderr: "pipe" });
    expect(await child.exited).toBe(0);
    expect(JSON.parse(await new Response(child.stdout).text())).toBeNull();
    // Advance only this fixture's eligibility, not its lifetime or count.
    await rows('UPDATE "__jadpo_delivery_attempts_v1" SET retry_not_before = $1 WHERE intent_id = $2', ["2000-01-01T00:00:00.000Z", first.intentId]);
    const next = await claimIntent(first.intentId);
    expect(next).toMatchObject({ intentId: first.intentId, invocation: 2, generation: "2", firstClaimAt: handle.firstClaimAt, lifetimeDeadline: handle.lifetimeDeadline });
    expect(await mailNoEffect(handle)).toBeNull();
    expect(await checkpoint(handle)).toBe(false);
    expect(await checkpoint(next)).toBe(true);
  });
  test("durable no-effect outcomes reject unsafe classifications shapes and expired claims", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-validate"))));
    const handle = await claimIntent(intent.intentId);
    let accessed = false;
    const accessor = { ...noEffect };
    Object.defineProperty(accessor, "failureClass", { get() { accessed = true; return "service_unavailable"; } });
    for (const outcome of [{ ...noEffect, failureClass: "outcome_unknown" }, { ...noEffect, providerAttempts: 4 }, { ...noEffect, providerAttempts: -1 }, { ...noEffect, retryDelayMs: -1 }, { ...noEffect, retryDelayMs: Infinity }, { ...noEffect, raw: "secret-canary" }, { ...noEffect, failureClass: "recipient_rejected" }, accessor]) {
      await expect(mailNoEffect(handle, outcome)).rejects.toMatchObject({ operation: "delivery.no_effect_outcome" });
    }
    expect(accessed).toBe(false);
    await expect(persistence.record_delivery_mail_no_effect(handle, noEffect)).rejects.toMatchObject({ operation: "delivery.transaction_required" });
    expect(await mailNoEffect({ ...handle, generation: "2" })).toBeNull();
    await expireClaim(intent.intentId);
    expect(await mailNoEffect(handle)).toBeNull();
    expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
  });
  test("durable no-effect outcomes exhaust cumulative counts into bounded dead letters", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("retry-exhaust");
    const [first, second] = await persistence.transaction(async (tx: any) => [await tx.enqueue_delivery_intent(outboxSpec(value, "1")), await tx.enqueue_delivery_intent(outboxSpec(value, "2"))]);
    for (let invocation = 1; invocation <= 3; invocation++) {
      const handle = await claimIntent(first.intentId);
      expect(handle.invocation).toBe(invocation);
      expect(await checkpoint(handle)).toBe(true);
      expect(await mailNoEffect(handle, { ...noEffect, retryDelayMs: 0 })).toBe(invocation === 3 ? "failed" : "retry_wait");
    }
    expect(await claimIntent(first.intentId)).toBeNull();
    expect((await rows('SELECT CAST(generation AS TEXT) AS generation, invocations, CAST(provider_attempts AS TEXT) AS provider_attempts, failure_class, payload_version FROM "__jadpo_delivery_dead_letters_v1" WHERE intent_id = $1', [first.intentId]))[0])
      .toEqual({ generation: "3", invocations: 3, provider_attempts: "9", failure_class: "budget_exhausted", payload_version: first.payloadVersion });
    expect(await rows('SELECT invocation FROM "__jadpo_delivery_attempts_v1" WHERE intent_id = $1 ORDER BY invocation', [first.intentId])).toEqual([{ invocation: 1 }, { invocation: 2 }, { invocation: 3 }]);
    expect((await claimIntent(second.intentId)).invocation).toBe(1);
  });
  test("durable no-effect permanent failures and lifetime limits stop safe retry", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const permanent of [true, false]) {
      const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry(`retry-stop-${permanent}`))));
      const handle = await claimIntent(intent.intentId);
      expect(await mailNoEffect(handle, permanent ? { failureClass: "recipient_rejected", providerAttempts: 1, retryDelayMs: 0 } : { ...noEffect, retryDelayMs: claimLimits.lifetimeMs })).toBe("failed");
      expect((await rows('SELECT failure_class FROM "__jadpo_delivery_dead_letters_v1" WHERE intent_id = $1', [intent.intentId]))[0].failure_class).toBe(permanent ? "recipient_rejected" : "budget_exhausted");
      expect(await claimIntent(intent.intentId)).toBeNull();
    }
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-life-recovery"))));
    const handle = await claimIntent(intent.intentId);
    expect(await mailNoEffect(handle)).toBe("retry_wait");
    await rows('UPDATE "__jadpo_delivery_claims_v1" SET lifetime_deadline = $1 WHERE intent_id = $2', ["2000-01-01T00:00:00.000Z", intent.intentId]);
    expect(await claimIntent(intent.intentId)).toBeNull();
    expect((await rows('SELECT invocations, CAST(provider_attempts AS TEXT) AS provider_attempts, failure_class FROM "__jadpo_delivery_dead_letters_v1" WHERE intent_id = $1', [intent.intentId]))[0])
      .toEqual({ invocations: 1, provider_attempts: "3", failure_class: "budget_exhausted" });
  });
  test("durable no-effect proof honours cancellation without reopening unknown sends", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const unknown of [true, false]) {
      const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry(`retry-cancel-${unknown}`))));
      const handle = await claimIntent(intent.intentId);
      expect(await checkpoint(handle)).toBe(true);
      expect(await persistence.transaction((tx: any) => tx.cancel_delivery_intent(intent.intentId))).toBe("requested");
      if (unknown) expect(await persistence.transaction((tx: any) => tx.record_delivery_unknown(handle))).toBe(true);
      expect(await mailNoEffect(handle)).toBe(unknown ? null : "cancelled");
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe(unknown ? "outcome_unknown" : "cancelled");
      expect(await claimIntent(intent.intentId)).toBeNull();
      expect(await rows('SELECT intent_id FROM "__jadpo_delivery_dead_letters_v1" WHERE intent_id = $1', [intent.intentId])).toEqual([]);
    }
  });
  test("durable no-effect outcome history and state roll back even when native faults are caught", async () => {
    const persistence = persistenceModule.persistence as any;
    const setup = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-schema"))));
    await claimIntent(setup.intentId);
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-native-fault"))));
    const handle = await claimIntent(intent.intentId, { ...claimLimits, maxInvocations: 1 });
    expect(await checkpoint(handle)).toBe(true);
    if (postgresUrl) {
      await rows(`CREATE FUNCTION rm306_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.intent_id = '${intent.intentId}' THEN RAISE EXCEPTION 'fixture retry fault'; END IF; RETURN NEW; END $$`);
      await rows('CREATE TRIGGER rm306_retry_fault BEFORE INSERT ON "__jadpo_delivery_dead_letters_v1" FOR EACH ROW EXECUTE FUNCTION rm306_retry_fault()');
    } else await rows(`CREATE TRIGGER rm306_retry_fault BEFORE INSERT ON "__jadpo_delivery_dead_letters_v1" WHEN NEW.intent_id = '${intent.intentId}' BEGIN SELECT RAISE(ABORT, 'fixture retry fault'); END`);
    const unrelated = entry("retry-caught-outer-commit");
    try {
      await persistence.withOperationTime(new Date().toISOString()).transaction(async (tx: any) => {
        await expect(tx.record_delivery_mail_no_effect(handle, { ...noEffect, retryDelayMs: 0 })).rejects.toMatchObject({ operation: "delivery.dead_letter.record" });
        await tx.create_Entry(unrelated);
      });
      expect(await stored(unrelated.id)).toEqual([unrelated]);
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
      expect((await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0].possible_dispatch).toBe(1);
      expect(await rows('SELECT intent_id FROM "__jadpo_delivery_attempts_v1" WHERE intent_id = $1', [intent.intentId])).toEqual([]);
    } finally {
      if (postgresUrl) { await rows('DROP TRIGGER rm306_retry_fault ON "__jadpo_delivery_dead_letters_v1"'); await rows('DROP FUNCTION rm306_retry_fault()'); }
      else await rows('DROP TRIGGER rm306_retry_fault');
    }
    await expect(persistence.transaction(async (tx: any) => { expect(await tx.record_delivery_mail_no_effect(handle, { ...noEffect, retryDelayMs: 0 })).toBe("failed"); throw new Error("fixture outer rollback"); })).rejects.toThrow("fixture outer rollback");
    expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
    expect(await mailNoEffect(handle, { ...noEffect, retryDelayMs: 0 })).toBe("failed");
  });
  test("durable no-effect retry resumes only after its real persisted delay with golden cumulative limits", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-real-delay"))));
    // Accepted RM-108 values; generic fixtures elsewhere keep their own limits.
    const goldenLimits = { ...claimLimits, maxInvocations: 3, lifetimeMs: 3600000 };
    const handle = await claimIntent(intent.intentId, goldenLimits);
    expect(Date.parse(handle.lifetimeDeadline) - Date.parse(handle.firstClaimAt)).toBe(3600000);
    expect(await mailNoEffect(handle, { ...noEffect, providerAttempts: 0, retryDelayMs: 300 })).toBe("retry_wait");
    expect(await claimIntent(intent.intentId, goldenLimits)).toBeNull();
    await Bun.sleep(350);
    const resumed = await claimIntent(intent.intentId, goldenLimits);
    expect(resumed).toMatchObject({ invocation: 2, generation: "2", firstClaimAt: handle.firstClaimAt, lifetimeDeadline: handle.lifetimeDeadline });
    expect(Date.parse(resumed.leaseUntil)).toBeGreaterThan(Date.parse(handle.leaseUntil));
  });
  test("durable no-effect admission samples fresh SQL time and retains eligible proof despite late return", async () => {
    const persistence = persistenceModule.persistence as any;
    for (const phase of ["before", "after"] as const) {
      const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry(`retry-late-${phase}`))));
      const limits = { ...claimLimits, executionMs: 100, leaseMs: 100 };
      const handle = await claimIntent(intent.intentId, limits);
      expect(await checkpoint(handle)).toBe(true);
      const result = await delayedDeliveryClient('WITH "no_effect_clock"', phase, 250)
        .transaction((tx: any) => tx.record_delivery_mail_no_effect(handle, noEffect));
      expect(result).toBe(phase === "after" ? "retry_wait" : null);
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe(phase === "after" ? "retry_wait" : "running");
      expect(Number((await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0].possible_dispatch)).toBe(phase === "after" ? 0 : 1);
      expect(await claimIntent(intent.intentId, limits)).toBeNull();
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe(phase === "after" ? "retry_wait" : "outcome_unknown");
    }
  });
  test("durable no-effect exhaustion recovery is savepoint atomic when terminal-state faults are caught", async () => {
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-exhaust-native-fault"))));
    const limits = { ...claimLimits, maxInvocations: 1 };
    await claimIntent(intent.intentId, limits);
    await expireClaim(intent.intentId);
    if (postgresUrl) {
      await rows(`CREATE FUNCTION rm306_exhaust_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.intent_id = '${intent.intentId}' AND NEW.state = 'failed' THEN RAISE EXCEPTION 'fixture exhausted state fault'; END IF; RETURN NEW; END $$`);
      await rows('CREATE TRIGGER rm306_exhaust_fault BEFORE UPDATE ON "__jadpo_deliveries_v1" FOR EACH ROW EXECUTE FUNCTION rm306_exhaust_fault()');
    } else await rows(`CREATE TRIGGER rm306_exhaust_fault BEFORE UPDATE OF state ON "__jadpo_deliveries_v1" WHEN NEW.intent_id = '${intent.intentId}' AND NEW.state = 'failed' BEGIN SELECT RAISE(ABORT, 'fixture exhausted state fault'); END`);
    const unrelated = entry("retry-exhaust-caught-outer");
    try {
      await persistence.withOperationTime(new Date().toISOString()).transaction(async (tx: any) => {
        await expect(tx.claim_delivery_intent(intent.intentId, limits)).rejects.toMatchObject({ operation: "delivery.claim.state" });
        await tx.create_Entry(unrelated);
      });
      expect(await stored(unrelated.id)).toEqual([unrelated]);
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
      expect(await rows('SELECT intent_id FROM "__jadpo_delivery_dead_letters_v1" WHERE intent_id = $1', [intent.intentId])).toEqual([]);
    } finally {
      if (postgresUrl) { await rows('DROP TRIGGER rm306_exhaust_fault ON "__jadpo_deliveries_v1"'); await rows('DROP FUNCTION rm306_exhaust_fault()'); }
      else await rows('DROP TRIGGER rm306_exhaust_fault');
    }
    expect(await claimIntent(intent.intentId, limits)).toBeNull();
    expect((await rows('SELECT invocations, CAST(provider_attempts AS TEXT) AS provider_attempts, failure_class FROM "__jadpo_delivery_dead_letters_v1" WHERE intent_id = $1', [intent.intentId]))[0])
      .toEqual({ invocations: 1, provider_attempts: "0", failure_class: "budget_exhausted" });
  });
  test("durable no-effect whole-transaction rollback revokes caught clients before any autocommit write", async () => {
    if (postgresUrl) return; // SQLite native whole-transaction abort, not a PostgreSQL equivalence claim.
    const persistence = persistenceModule.persistence as any;
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(entry("retry-whole-rollback"))));
    const handle = await claimIntent(intent.intentId, { ...claimLimits, maxInvocations: 1 });
    expect(await checkpoint(handle)).toBe(true);
    await rows(`CREATE TRIGGER rm306_whole_rollback BEFORE INSERT ON "__jadpo_delivery_dead_letters_v1" WHEN NEW.intent_id = '${intent.intentId}' BEGIN SELECT RAISE(ROLLBACK, 'fixture whole transaction rollback'); END`);
    const unrelated = entry("retry-no-autocommit");
    let captured: any;
    try {
      await expect(persistence.withOperationTime(new Date().toISOString()).transaction(async (tx: any) => {
        captured = tx;
        await expect(tx.record_delivery_mail_no_effect(handle, { ...noEffect, retryDelayMs: 0 })).rejects.toMatchObject({ operation: "transaction.savepoint.rollback" });
        await expect(tx.enqueue_delivery_intent(outboxSpec(unrelated))).rejects.toMatchObject({ operation: "delivery.transaction_required" });
        await expect(tx.create_Entry(unrelated)).rejects.toMatchObject({ operation: "transaction.inactive" });
        await expect(tx.allowsPolicy("Entry", "create")).rejects.toMatchObject({ operation: "transaction.inactive" });
        await expect(tx.withOperationTime(new Date().toISOString()).create_Entry(unrelated)).rejects.toMatchObject({ operation: "transaction.inactive" });
      })).rejects.toMatchObject({ transaction: { outcome: "unknown", rollbackProven: false } });
      expect(await stored(unrelated.id)).toEqual([]);
      expect(await rows('SELECT intent_id FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1', [unrelated.id])).toEqual([]);
      expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("running");
      expect(Number((await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id = $1', [intent.intentId]))[0].possible_dispatch)).toBe(1);
      expect(await rows('SELECT intent_id FROM "__jadpo_delivery_attempts_v1" WHERE intent_id = $1', [intent.intentId])).toEqual([]);
      await expect(captured.transaction(() => undefined)).rejects.toMatchObject({ operation: "transaction.inactive" });
    } finally { await rows('DROP TRIGGER rm306_whole_rollback'); }
  });
  test("failed PostgreSQL savepoint recovery poisons the shared client and prevents callback commit", async () => {
    if (!postgresUrl) return; // Deterministic cleanup fault on the real PostgreSQL connection.
    const persistence = persistenceModule.persistence as any;
    const value = entry("postgres-poison");
    const intent = await persistence.transaction((tx: any) => tx.enqueue_delivery_intent(outboxSpec(value)));
    const connection = new Proxy(sql!, { get(target, property) {
      if (property === "begin") return (work: (tx: any) => Promise<any>) => target.begin(tx => work(new Proxy(tx, { get(inner, key) {
        if (key === "unsafe") return async (statement: string, values: any[]) => {
          if (statement.startsWith('ROLLBACK TO SAVEPOINT')) throw Object.assign(new Error("fixture unproved cleanup"), { code: "XX000" });
          return inner.unsafe(statement, values);
        };
        const member = Reflect.get(inner, key, inner); return typeof member === "function" ? member.bind(inner) : member;
      } })));
      const member = Reflect.get(target, property, target); return typeof member === "function" ? member.bind(target) : member;
    } });
    const controlled = (persistenceModule as any).__controlQueryProbeClient(connection, null).withOperationTime(new Date().toISOString());
    const unrelated = entry("postgres-no-poison-commit");
    await expect(controlled.transaction(async (tx: any) => {
      await tx.create_Entry(value);
      await expect(tx.transaction(async () => { throw new Error("fixture savepoint failure"); })).rejects.toMatchObject({ operation: "transaction.savepoint.rollback" });
      await expect(tx.enqueue_delivery_intent(outboxSpec(unrelated))).rejects.toMatchObject({ operation: "delivery.transaction_required" });
      await expect(tx.create_Entry(unrelated)).rejects.toMatchObject({ operation: "transaction.inactive" });
      await expect(tx.allowsPolicy("Entry", "create")).rejects.toMatchObject({ operation: "transaction.inactive" });
    })).rejects.toMatchObject({ operation: "transaction.poisoned", transaction: { outcome: "no_commit", rollbackProven: true } });
    expect(await stored(value.id)).toEqual([]);
    expect(await stored(unrelated.id)).toEqual([]);
    expect((await persistence.read_delivery_intent(intent.intentId)).state).toBe("pending");
  });
  test("SQL and persistence metadata preserve inherited nullable contracts", async () => {
    const manifest = JSON.parse(readFileSync(join(root, "build/persistence/entities.json"), "utf8"));
    const fields = manifest.entities.find((entity: any) => entity.entity === "Entry").fields;
    expect(fields.find((field: any) => field.name === "note").nullable).toBe(true);
    expect(fields.find((field: any) => field.name === "label").nullable).toBe(false);
    for (const dialect of ["sqlite", "postgres"]) {
      const ddl = readFileSync(join(root, `build/sql/${dialect}/schema.sql`), "utf8");
      const noteColumn = ddl.split("\n").find((line) => line.trimStart().startsWith('"note" '));
      expect(noteColumn).toBeDefined();
      expect(noteColumn).not.toContain("NOT NULL");
    }
  });
  test("explicit none survives HTTP creation, actual storage and a later read", async () => {
    const value = entry("nullable-roundtrip", null);
    await insert(value);
    const response = await request("/find", { id: value.id });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(value);
  });
  test("a nullable field is still mandatory and required fields cannot become null", async () => {
    const value = entry("boundary-reject");
    for (const body of [{ id: value.id, label: value.label }, { ...value, label: null }]) {
      const response = await request("/entries", body);
      expect(response.status).toBe(422);
      expect(await stored(value.id)).toEqual([]);
    }
  });
  test("patch omission preserves a nullable value while supplied none clears it", async () => {
    const value = entry("patch-before");
    await insert(value);
    const omitted = await request("/patch", { id: value.id, changes: { label: "patch-after" } });
    expect(omitted.status).toBe(200);
    expect(await stored(value.id)).toEqual([{ ...value, label: "patch-after" }]);
    const cleared = await request("/patch", { id: value.id, changes: { note: null } });
    expect(cleared.status).toBe(200);
    expect(await cleared.json()).toEqual({ ...value, label: "patch-after", note: null });
    expect(await stored(value.id)).toEqual([{ ...value, label: "patch-after", note: null }]);
  });
  test("patch lowering uses the local binding at its lexical use, not a shadowed parameter", async () => {
    const value = entry("shadow-before");
    await insert(value);
    const response = await request("/shadow", {
      id: value.id, original: { note: "unused@example.com" },
      replacement: { label: "shadow-after", note: null },
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ...value, label: "shadow-after", note: null });
    expect(await stored(value.id)).toEqual([{ ...value, label: "shadow-after", note: null }]);
  });
  test("ambiguous required mutation rolls back without clearing either nullable field", async () => {
    const first = entry("ambiguous", "first@example.com");
    const second = entry("ambiguous", "second@example.com");
    await insert(first); await insert(second);
    await expectOperationalFailure(await request("/batch", { label: "ambiguous", note: null }));
    expect(await stored(first.id)).toEqual([first]);
    expect(await stored(second.id)).toEqual([second]);
  });
  test("SQLite begin contention records a proved no-commit phase", async () => {
    if (postgresUrl) return;
    const blocker = new Database(sqlitePath, { strict: true });
    blocker.exec("BEGIN IMMEDIATE");
    try {
      let failure: any;
      try { await (persistenceModule.persistence as any).transaction(async () => undefined); }
      catch (error) { failure = error; }
      expect(failure).toBeInstanceOf(persistenceModule.PersistenceFault);
      expect(failure.transaction).toEqual({
        phase: "begin", database: "sqlite", outcome: "no_commit",
        retryableCause: expect.stringMatching(/^sqlite_(busy|locked)$/),
        rollbackProven: false, commitAcknowledged: false,
      });
    } finally {
      blocker.exec("ROLLBACK");
      blocker.close();
    }
  });
  test("cross-process SQLite contention retries the whole public action once", async () => {
    if (postgresUrl) return;
    const value = entry("retry-after-sqlite-begin-busy");
    const lockScript = [
      'import { Database } from "bun:sqlite";',
      `const db = new Database(${JSON.stringify(sqlitePath)}, { strict: true });`,
      'db.exec("BEGIN IMMEDIATE");',
      'console.log("LOCK_READY");',
      'await new Promise(resolve => process.stdin.once("data", resolve));',
      'db.exec("ROLLBACK"); db.close();',
    ].join("\n");
    const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "-e", lockScript], {
      stdin: "pipe", stdout: "pipe", stderr: "pipe",
    });
    const reader = child.stdout.getReader();
    let output = "";
    try {
      while (!output.includes("LOCK_READY")) {
        const chunk = await reader.read();
        if (chunk.done) throw new Error(`SQLite lock process exited before acquiring its lock: ${output}`);
        output += new TextDecoder().decode(chunk.value);
      }
    } finally { reader.releaseLock(); }
    const prototype: any = Database.prototype;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    let begins = 0;
    let commits = 0;
    prototype.exec = function (statement: string, ...values: unknown[]) {
      if (statement.trim().toUpperCase() === "BEGIN IMMEDIATE") begins++;
      if (statement.trim().toUpperCase() === "COMMIT") commits++;
      return originalExec.call(this, statement, ...values);
    };
    Math.random = () => 0.5;
    const release = setTimeout(() => { child.stdin.write("release\n"); child.stdin.end(); }, 10);
    try {
      const response = await request("/entries", value);
      expect(response.status).toBe(200);
      expect(await response.json()).toEqual(value);
      expect(await stored(value.id)).toEqual([value]);
      expect(begins).toBe(2);
      expect(commits).toBe(1);
      expect(await child.exited).toBe(0);
      expect(await new Response(child.stderr).text()).toBe("");
    } finally {
      clearTimeout(release);
      if (child.exitCode === null) { child.stdin.write("release\n"); child.stdin.end(); }
      prototype.exec = originalExec;
      Math.random = originalRandom;
    }
  });
  test("cancellation during retry backoff prevents the next SQLite attempt", async () => {
    if (postgresUrl) return;
    const value = entry("cancel-before-sqlite-retry");
    const blocker = new Database(sqlitePath, { strict: true });
    blocker.exec("BEGIN IMMEDIATE");
    const controller = new AbortController();
    const prototype: any = Database.prototype;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    let begins = 0;
    let commits = 0;
    prototype.exec = function (statement: string, ...values: unknown[]) {
      if (statement.trim().toUpperCase() === "BEGIN IMMEDIATE") begins++;
      if (statement.trim().toUpperCase() === "COMMIT") commits++;
      return originalExec.call(this, statement, ...values);
    };
    Math.random = () => 0.5;
    const cancel = setTimeout(() => {
      controller.abort();
      blocker.exec("ROLLBACK");
    }, 5);
    try {
      const response = await request("/entries", value, controller.signal);
      expect(response.status).toBe(503);
      expect((await response.json()).error.code).toBe("transaction_unavailable");
      expect(await stored(value.id)).toEqual([]);
      expect(begins).toBe(1);
      expect(commits).toBe(0);
    } finally {
      clearTimeout(cancel);
      if (blocker.inTransaction) blocker.exec("ROLLBACK");
      blocker.close();
      prototype.exec = originalExec;
      Math.random = originalRandom;
    }
  });
  test("SQLite commit busy retries only after rollback proves no commit", async () => {
    if (postgresUrl) return;
    const value = entry("retry-after-sqlite-commit-busy");
    const prototype: any = Database.prototype;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    let injected = false;
    let begins = 0;
    let commits = 0;
    prototype.exec = function (statement: string, ...values: unknown[]) {
      const command = statement.trim().toUpperCase();
      if (command === "BEGIN IMMEDIATE") begins++;
      if (command === "COMMIT") {
        commits++;
        if (!injected) {
          injected = true;
          throw Object.assign(new Error("injected busy COMMIT"), { code: "SQLITE_BUSY" });
        }
      }
      return originalExec.call(this, statement, ...values);
    };
    Math.random = () => 0;
    try {
      const response = await request("/entries", value);
      expect(response.status).toBe(200);
      expect(await response.json()).toEqual(value);
      expect(await stored(value.id)).toEqual([value]);
      expect(begins).toBe(2);
      expect(commits).toBe(2);
      const changes = await rows('SELECT entity_id, revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id]);
      expect(changes).toEqual([{ entity_id: value.id, revision: 1 }]);
    } finally {
      prototype.exec = originalExec;
      Math.random = originalRandom;
    }
  });
  test("a nested SQLite busy failure rolls back and retries only the outer operation", async () => {
    if (postgresUrl) return;
    const first = entry("nested-retry-first", null);
    const second = entry("nested-retry-second", null);
    const prototype: any = Database.prototype;
    const originalPrepare = prototype.prepare;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    const originalDate = globalThis.Date;
    const refreshedAt = "2026-10-02T20:00:01.000Z";
    let clockReads = 0;
    class SteppedDate extends originalDate {
      toISOString() {
        return clockReads++ === 0 ? "2026-10-02T20:00:00.000Z" : refreshedAt;
      }
    }
    let changeInserts = 0;
    let begins = 0;
    let commits = 0;
    prototype.prepare = function (statement: string, ...values: unknown[]) {
      if (statement.includes('INSERT INTO "__jadpo_changes"')) {
        changeInserts++;
        if (changeInserts === 2) {
          throw Object.assign(new Error("injected SQLite contention after a prior write"), { code: "SQLITE_BUSY" });
        }
      }
      return originalPrepare.call(this, statement, ...values);
    };
    prototype.exec = function (statement: string, ...values: unknown[]) {
      if (statement.trim().toUpperCase() === "BEGIN IMMEDIATE") begins++;
      if (statement.trim().toUpperCase() === "COMMIT") commits++;
      return originalExec.call(this, statement, ...values);
    };
    Math.random = () => 0;
    (globalThis as any).Date = SteppedDate;
    try {
      const response = await request("/pair", { first, second });
      expect(response.status).toBe(200);
      expect(await response.json()).toEqual(second);
      expect(await stored(first.id)).toEqual([first]);
      expect(await stored(second.id)).toEqual([second]);
      expect(begins).toBe(2);
      expect(commits).toBe(1);
      const changes = await rows('SELECT entity_id, revision, created_at FROM "__jadpo_changes" WHERE entity_id IN ($1, $2) ORDER BY entity_id', [first.id, second.id]);
      expect(changes).toEqual([
        { entity_id: first.id, revision: 1, created_at: refreshedAt },
        { entity_id: second.id, revision: 1, created_at: refreshedAt },
      ]);
      expect(clockReads).toBeGreaterThanOrEqual(2);
    } finally {
      prototype.prepare = originalPrepare;
      prototype.exec = originalExec;
      Math.random = originalRandom;
      (globalThis as any).Date = originalDate;
    }
  });
  test("native SQLite shared-cache write LOCKED retries only after outer rollback", async () => {
    if (postgresUrl) return;
    // Explicit disposable shared-cache connections produce native write-time
    // LOCKED. Production file adapters are not silently switched to this mode.
    const uri = `file:rm402-locked-${crypto.randomUUID()}?mode=memory&cache=shared`;
    const flags = constants.SQLITE_OPEN_READWRITE | constants.SQLITE_OPEN_CREATE | constants.SQLITE_OPEN_URI | constants.SQLITE_OPEN_SHAREDCACHE;
    const writer = new Database(uri, flags);
    const blocker = new Database(uri, flags);
    writer.exec(readFileSync(join(root, "build/sql/sqlite/schema.sql"), "utf8"));
    writer.exec('CREATE TABLE "__jadpo_changes" (change_id TEXT PRIMARY KEY, entity TEXT NOT NULL, entity_id TEXT NOT NULL, revision INTEGER NOT NULL, operation TEXT NOT NULL, payload TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(entity, entity_id, revision))');
    const client = (persistenceModule as any).__controlQueryProbeClient(null, writer).withOperationTime("2026-10-04T08:00:00.000Z");
    blocker.exec("BEGIN");
    blocker.query('SELECT * FROM "entry"').all();
    const value = entry("native-write-locked");
    let attempts = 0;
    let nativeCode: string | null = null;
    const logs: any[] = [];
    const originalLog = console.error;
    console.error = (...args: unknown[]) => { if (typeof args[0] === "string") { try { logs.push(JSON.parse(args[0])); } catch {} } };
    try {
      const result = await client.transaction(async (transaction: any) => {
        attempts++;
        if (attempts === 2) {
          expect(writer.inTransaction).toBe(true);
          expect(writer.query('SELECT * FROM "__jadpo_changes"').all()).toEqual([]);
          blocker.exec("ROLLBACK");
        }
        try { return await transaction.create_Entry(value); }
        catch (error) { nativeCode = (error as any).cause?.code ?? null; throw error; }
      }, { replayable: true, operationId: "native-locked-write", operation: "validation.native_locked_write", startedAt: performance.now(), signal: null });
      expect(nativeCode).toBe("SQLITE_LOCKED_SHAREDCACHE");
      expect(attempts).toBe(2);
      expect(result).toEqual(value);
      expect(writer.query('SELECT revision FROM "__jadpo_changes" WHERE entity_id = ?1').all(value.id)).toEqual([{ revision: 1 }]);
      expect(logs.filter(log => log.eventName === "transaction.attempt").map(log => ({ attempt: log.attempt, outcome: log.outcome, rollbackProven: log.rollbackProven, retryableCause: log.retryableCause }))).toEqual([
        { attempt: 1, outcome: "no_commit", rollbackProven: true, retryableCause: "sqlite_locked" },
        { attempt: 2, outcome: "committed", rollbackProven: false, retryableCause: null },
      ]);
    } finally {
      console.error = originalLog;
      if (blocker.inTransaction) blocker.exec("ROLLBACK");
      blocker.close(); writer.close();
    }
  });
  test("full-jitter samples cover the interval and remaining deadline cap on both adapters", async () => {
    const persistence = persistenceModule.persistence as any;
    const originalRandom = Math.random;
    const originalNow = performance.now;
    const originalTimer = globalThis.setTimeout;
    const originalLog = console.error;
    const cause: any = new Error("structured jitter probe");
    if (postgresUrl) cause.errno = "40001";
    else cause.code = "SQLITE_BUSY";
    // Exactly representable origin avoids host-clock ulps in budget arithmetic.
    const now = 500;
    let selectedWaits: number[] = [];
    performance.now = () => now;
    globalThis.setTimeout = ((callback: (...args: any[]) => void, delay?: number, ...args: any[]) => { if (typeof delay === "number") selectedWaits.push(delay); return originalTimer(callback, 0, ...args); }) as typeof setTimeout;
    try {
      for (const fraction of [0, 0.125, 0.25, 0.5, 0.75, 0.999999]) {
        for (const remaining of [1000, 40]) {
          const logs: any[] = [];
          selectedWaits = [];
          console.error = (...args: unknown[]) => { if (typeof args[0] === "string") { try { logs.push(JSON.parse(args[0])); } catch {} } };
          Math.random = () => fraction;
          let attempts = 0;
          const result = await persistence.transaction(async () => {
            attempts++;
            if (attempts < 3) throw new persistenceModule.PersistenceFault("transaction.jitter_probe", "driver", cause);
            return "committed";
          }, { replayable: true, operationId: `jitter-${remaining}-${fraction}`, operation: "validation.jitter", startedAt: now, deadlineAt: now + remaining, signal: null });
          expect(result).toBe("committed");
          expect(attempts).toBe(3);
          const trace = logs.filter(log => log.eventName === "transaction.attempt");
          const expectedWaits = [fraction * Math.min(50, remaining), fraction * Math.min(100, remaining)];
          expect(selectedWaits).toEqual(expectedWaits.filter(delay => delay > 0));
          expect(trace.map(log => log.selectedDelayMs)).toEqual([...expectedWaits.map(Math.round), null]);
          expect(trace.map(log => log.retryScheduled)).toEqual([true, true, false]);
        }
      }
    } finally {
      Math.random = originalRandom; performance.now = originalNow; globalThis.setTimeout = originalTimer; console.error = originalLog;
    }
    // Deterministic mapping/bounds, not a statistical RNG-quality claim.
  });
  test("seeded native mutation retries consume progressive unrounded waits and one original budget on both adapters", async () => {
    // Finite reproducible consumer coverage, not production entropy quality.
    // LCG(seed JADP): uint32(1664525*x+1013904223), divide by2^32.
    const reference = [2932296559,3799187970,3242514809,3093640324,224605715,3118416982,1315286973,3843239416,4293990647,3149832682,2306500673,730408108,2882426907,3349622462,2367293189,1963609248,3290208127,1638285010,2439630857,412293588,2649078563,269772326,669075277,4224532552];
    let seed = 0x4a414450, referenceIndex = 0;
    const observedDraws: number[] = [], normalizedByRetry: number[][] = [[], []];
    const originalRandom = Math.random, originalNow = performance.now, originalTimer = globalThis.setTimeout, originalLog = console.error;
    const originalDateNow = Date.now;
    const operationTime = "2026-10-04T08:00:00.000Z";
    let logicalNow = 500, remaining = 1000, operationId = "", selectedWait: number | null = null, schedulerOverrun = 0;
    let draws: number[] = [], actualWaits: number[] = [], waitStarts: number[] = [], waitSettlements: number[] = [], events: any[] = [];
    let begins = 0, rollbackAcknowledgements = 0, commitAcknowledgements = 0;
    let client: any, activePostgresTransaction: any = null;
    const factory = persistenceModule.__controlQueryProbeClient;
    const bind = (target: any, property: PropertyKey) => { const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value; };
    // Delegate to actual native transaction owner; no fake driver result.
    if (sql !== null) {
      const connection = new Proxy(sql, { get(target, property) {
        if (property === "begin") return async (work: (tx: any) => Promise<any>) => {
          begins++;
          try { const result = await target.begin(async tx => { activePostgresTransaction = tx; try { return await work(tx); } finally { activePostgresTransaction = null; } }); commitAcknowledgements++; return result; }
          catch (error) { rollbackAcknowledgements++; throw error; }
        };
        return bind(target, property);
      } });
      client = factory(connection, null);
    } else {
      const connection = new Proxy(sqlite!, { get(target, property) {
        if (property === "exec") return (statement: string, ...args: any[]) => {
          const result = target.exec(statement, ...args);
          switch (statement.trim().toUpperCase()) { case "BEGIN IMMEDIATE": begins++; break; case "ROLLBACK": rollbackAcknowledgements++; break; case "COMMIT": commitAcknowledgements++; break; }
          return result;
        };
        return bind(target, property);
      } });
      client = factory(null, connection);
    }
    const faultCause: any = new Error("listed transient after native mutation");
    if (postgresUrl) faultCause.errno = "40001"; else faultCause.code = "SQLITE_BUSY";
    let supplied: number[] = [], suppliedIndex = 0;
    Math.random = () => {
      if (selectedWait !== null) throw new Error("unattributed RNG draw before wait settlement");
      let fraction: number;
      if (operationId.startsWith("seeded-native-")) {
        seed = (Math.imul(1664525, seed) + 1013904223) >>> 0;
        expect(seed).toBe(reference[referenceIndex++]);
        fraction = seed / 4294967296;
        observedDraws.push(fraction);
        normalizedByRetry[draws.length].push(fraction);
      } else {
        if (suppliedIndex >= supplied.length) throw new Error("unexpected progressive RNG draw");
        fraction = supplied[suppliedIndex++];
      }
      draws.push(fraction);
      selectedWait = fraction * Math.min(50 * (2 ** (draws.length - 1)), remaining - (logicalNow - 500));
      return fraction;
    };
    performance.now = () => logicalNow;
    // Wall time is separate; this does not qualify fresh TIME-D05 snapshots.
    Date.now = () => 1791100800000;
    globalThis.setTimeout = ((callback: (...args: any[]) => void, delay?: number, ...args: any[]) => {
      if (selectedWait !== null) {
        expect(delay).toBe(selectedWait);
        actualWaits.push(delay!); waitStarts.push(logicalNow);
        const wait = selectedWait; selectedWait = null;
        return originalTimer(() => {
          logicalNow += wait + schedulerOverrun;
          waitSettlements.push(logicalNow);
          callback(...args);
        }, 0);
      }
      // Native deadline/background timers retain their actual scheduling.
      return originalTimer(callback, delay, ...args);
    }) as typeof setTimeout;
    console.error = (...args: unknown[]) => {
      if (typeof args[0] === "string") { try { const event = JSON.parse(args[0]); if (event.eventName === "transaction.attempt") { expect(event.operationId).toBe(operationId); events.push(event); } } catch (error) { if (!(error instanceof SyntaxError)) throw error; } }
    };
    try {
      for (let index = 0; index < 17; index++) {
        const seeded = index < 12, clamp = index === 12, overrun = index === 13, workExpiry = index === 14, largePair = index === 15, originalBudgetExpiry = index === 16;
        remaining = seeded || largePair || originalBudgetExpiry ? 1000 : 40; operationId = seeded ? `seeded-native-${index}` : clamp ? "progressive-clamp" : overrun ? "progressive-overrun" : workExpiry ? "progressive-work-expiry" : largePair ? "progressive-large-pair" : "progressive-original-budget-expiry";
        logicalNow = 500; selectedWait = null; schedulerOverrun = overrun ? 12 : originalBudgetExpiry ? 980 : 0;
        supplied = [0.75, 0.5]; suppliedIndex = 0; draws = []; actualWaits = []; waitStarts = []; waitSettlements = []; events = [];
        begins = 0; rollbackAcknowledgements = 0; commitAcknowledgements = 0;
        const value = entry(operationId, null); let callbackAttempts = 0;
        const originalStartedAt = logicalNow, originalDeadlineAt = logicalNow + remaining;
        let result: any, terminalFault: any = null;
        try {
          result = await client.withOperationTime(operationTime).transaction(async (transaction: any) => {
            callbackAttempts++;
            // Previous native outer rollback completed before another BEGIN.
            expect(await stored(value.id)).toEqual([]);
            expect(await rows('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id])).toEqual([]);
            const created = await transaction.create_Entry(value);
            const provisionalSource = activePostgresTransaction !== null ? Array.from(await activePostgresTransaction.unsafe('SELECT id FROM "entry" WHERE id = $1', [value.id])) : sqlite!.query('SELECT id FROM "entry" WHERE id = $1').all(value.id);
            const provisionalChanges = activePostgresTransaction !== null ? Array.from(await activePostgresTransaction.unsafe('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id])) : sqlite!.query('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1').all(value.id);
            expect(provisionalSource).toEqual([{ id: value.id }]); expect(provisionalChanges.map((r: any) => Number(r.revision))).toEqual([1]);
            if (callbackAttempts < 3) {
              if (workExpiry) logicalNow += 41;
              throw new persistenceModule.PersistenceFault("transaction.seeded_native_probe", "driver", faultCause);
            }
            return created;
          }, { replayable: true, operationId, operation: "validation.seeded_native", startedAt: originalStartedAt, deadlineAt: originalBudgetExpiry ? null : originalDeadlineAt, signal: null });
        } catch (error) { terminalFault = error; }
        const successful = seeded || clamp || largePair;
        const expectedDraws = seeded ? reference.slice(index * 2, index * 2 + 2).map(n => n / 4294967296) : workExpiry ? [] : overrun || originalBudgetExpiry ? [0.75] : [0.75, 0.5];
        const expectedWaits = successful ? [expectedDraws[0] * Math.min(50, remaining), expectedDraws[1] * Math.min(100, remaining - expectedDraws[0] * Math.min(50, remaining))] : workExpiry ? [] : [0.75 * Math.min(50, remaining)];
        originalLog(JSON.stringify({ kind: "seeded_native_progressive_budget", database: postgresUrl ? "postgres" : "sqlite", operationId, originalStartedAt, originalDeadlineAt, declaredDeadlineAt: originalBudgetExpiry ? null : originalDeadlineAt, draws, actualWaits, waitStarts, waitSettlements, separatelyAccountedSchedulerOverrunMs: schedulerOverrun, separatelyAccountedWorkMs: workExpiry ? 41 : 0, terminalNow: logicalNow, remainingMs: Math.max(0, originalDeadlineAt-logicalNow), callbackAttempts, begins, rollbackAcknowledgements, commitAcknowledgements, events, claim: "finite_seeded_consumer_native_rollback_not_entropy_quality_or_TIME_D05" }));
        expect(draws).toEqual(expectedDraws); expect(actualWaits).toEqual(expectedWaits);
        expect(selectedWait).toBeNull(); expect(originalStartedAt).toBe(500); expect(originalDeadlineAt).toBe(500 + remaining);
        expect(waitStarts).toEqual(expectedWaits.map((_, i) => 500 + expectedWaits.slice(0, i).reduce((a, b) => a + b, 0)));
        expect(waitSettlements).toEqual(expectedWaits.map((_, i) => 500 + expectedWaits.slice(0, i + 1).reduce((a, b) => a + b, 0) + schedulerOverrun));
        expect(logicalNow).toBe(500 + expectedWaits.reduce((a, b) => a + b, 0) + schedulerOverrun + (workExpiry ? 41 : 0));
        expect(events.map(e => e.operationId)).toEqual(Array(successful ? 3 : 1).fill(operationId));
        expect(events.map(e => e.selectedDelayMs)).toEqual(successful ? [...expectedWaits.map(Math.round), null] : workExpiry ? [null] : expectedWaits.map(Math.round));
        expect(events.map(e => e.elapsedMs)).toEqual(successful ? [0, 0, 0] : workExpiry ? [41] : [0]);
        expect(events.map(e => e.retryScheduled)).toEqual(successful ? [true, true, false] : [false]);
        expect([begins, rollbackAcknowledgements, commitAcknowledgements, callbackAttempts]).toEqual(successful ? [3, 2, 1, 3] : [1, 1, 0, 1]);
        expect(events.map(e => [e.outcome, e.rollbackProven])).toEqual(successful ? [["no_commit", true], ["no_commit", true], ["committed", false]] : [["no_commit", true]]);
        if (successful) {
          expect(terminalFault).toBeNull(); expect(result).toEqual(value); expect(await stored(value.id)).toEqual([value]);
          const revisions = await rows('SELECT revision, payload FROM "__jadpo_changes" WHERE entity_id = $1', [value.id]);
          expect(revisions.map(r => Number(r.revision))).toEqual([1]); expect(JSON.parse(revisions[0].payload)).toEqual(value);
        } else {
          expect(terminalFault).toBeInstanceOf(persistenceModule.PersistenceFault); expect(terminalFault.deadlineExceeded).toBe(!originalBudgetExpiry); expect(terminalFault.transaction).toMatchObject({ outcome: "no_commit", rollbackProven: true, commitAcknowledged: false });
          expect(await stored(value.id)).toEqual([]); expect(await rows('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id])).toEqual([]);
        }
      }
      expect(referenceIndex).toBe(24); expect(observedDraws).toEqual(reference.map(n => n / 4294967296));
      const coverage = normalizedByRetry.map(samples => [0,1,2,3].map(quartile => samples.filter(value => value >= quartile / 4 && value < (quartile + 1) / 4).length));
      expect(coverage).toEqual([[2,1,6,3], [3,2,3,4]]);
      originalLog(JSON.stringify({ kind: "finite_seeded_draw_coverage", seed: "0x4a414450", draws: observedDraws, normalizedByRetry, quartileCountsByRetry: coverage, reachableCapsMs: [50,100], formulaCeiling250DynamicallyReached: false }));
    } finally {
      Math.random = originalRandom; performance.now = originalNow; globalThis.setTimeout = originalTimer; console.error = originalLog; Date.now = originalDateNow;
    }
    expect(Math.random).toBe(originalRandom); expect(performance.now).toBe(originalNow); expect(globalThis.setTimeout).toBe(originalTimer); expect(Date.now).toBe(originalDateNow); expect(console.error).toBe(originalLog);
  });
  test("private native retries jointly preserve contiguous source changes and one immutable intent per committed revision", async () => {
    // Explicit compiler-private primitive composition; public creates and
    // authored enqueue remain excluded. Rolled-back random IDs are not durable.
    const value = entry("joint-native-revision-1", null);
    const job = "validation_delivery", orderingKey = value.id;
    const originalRandom = Math.random, originalLog = console.error;
    const factory = persistenceModule.__controlQueryProbeClient;
    const query = async (owner: any, statement: string, values: any[] = []) => sql !== null ? Array.from(await owner.unsafe(statement, values)) : owner.query(statement).all(...values);
    const present = async (owner: any, table: string) => sql !== null ? (await query(owner, 'SELECT to_regclass($1) IS NOT NULL AS present', [`"${table}"`]))[0].present === true : (await query(owner, 'SELECT name FROM sqlite_master WHERE type = \'table\' AND name = $1', [table])).length === 1;
    const snapshot = async (owner: any) => {
      const deliveriesPresent = await present(owner, "__jadpo_deliveries_v1"), keysPresent = await present(owner, "__jadpo_delivery_keys_v1");
      return { source: await query(owner, 'SELECT * FROM "entry" WHERE id = $1', [value.id]), changes: await query(owner, 'SELECT *, CAST(revision AS TEXT) AS revision FROM "__jadpo_changes" WHERE entity_id = $1 ORDER BY "__jadpo_changes".revision', [value.id]), deliveriesPresent, keysPresent,
        intents: deliveriesPresent ? await query(owner, 'SELECT *, CAST(key_sequence AS TEXT) AS key_sequence FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1 ORDER BY "__jadpo_deliveries_v1".key_sequence', [value.id]) : [],
        keys: keysPresent ? await query(owner, 'SELECT *, CAST(next_sequence AS TEXT) AS next_sequence FROM "__jadpo_delivery_keys_v1" WHERE job = $1 AND ordering_key = $2', [job, orderingKey]) : [] };
    };
    // SQLite synchronous original methods permit an exact before-BEGIN view.
    const sqliteSnapshot = () => {
      const read = (statement: string, values: any[] = []) => sqlite!.query(statement).all(...values);
      const has = (table: string) => read('SELECT name FROM sqlite_master WHERE type = \'table\' AND name = $1', [table]).length === 1;
      const deliveriesPresent = has("__jadpo_deliveries_v1"), keysPresent = has("__jadpo_delivery_keys_v1");
      return { source: read('SELECT * FROM "entry" WHERE id = $1', [value.id]), changes: read('SELECT *, CAST(revision AS TEXT) AS revision FROM "__jadpo_changes" WHERE entity_id = $1 ORDER BY "__jadpo_changes".revision', [value.id]), deliveriesPresent, keysPresent,
        intents: deliveriesPresent ? read('SELECT *, CAST(key_sequence AS TEXT) AS key_sequence FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1 ORDER BY "__jadpo_deliveries_v1".key_sequence', [value.id]) : [],
        keys: keysPresent ? read('SELECT *, CAST(next_sequence AS TEXT) AS next_sequence FROM "__jadpo_delivery_keys_v1" WHERE job = $1 AND ordering_key = $2', [job, orderingKey]) : [] };
    };
    const observer = sql ?? sqlite!;
    let owner: any = null, client: any, beforeState: any = null;
    let begins = 0, rollbacks = 0, commits = 0, operationId = "", callbackAttempts = 0;
    let phases: string[] = [], events: any[] = [], beforeBeginStates: any[] = [];
    const bind = (target: any, property: PropertyKey) => { const result = Reflect.get(target, property, target); return typeof result === "function" ? result.bind(target) : result; };
    if (sql !== null) {
      const connection = new Proxy(sql, { get(target, property) {
        if (property === "begin") return async (work: (tx: any) => Promise<any>) => {
          phases.push("before_begin"); const state = await snapshot(target); beforeBeginStates.push(state); expect(JSON.stringify(state)).toBe(JSON.stringify(beforeState));
          begins++;
          try {
            const result = await target.begin(async tx => { owner = tx; try { return await work(tx); } finally { owner = null; } });
            commits++; phases.push("commit_ack"); return result;
          } catch (error) { rollbacks++; phases.push("rollback_ack"); throw error; }
        };
        return bind(target, property);
      } });
      client = factory(connection, null);
    } else {
      const connection = new Proxy(sqlite!, { get(target, property) {
        if (property === "exec") return (statement: string, ...args: any[]) => {
          const command = statement.trim().toUpperCase();
          if (command === "BEGIN IMMEDIATE") { expect(target.inTransaction).toBe(false); phases.push("before_begin"); const state = sqliteSnapshot(); beforeBeginStates.push(state); expect(JSON.stringify(state)).toBe(JSON.stringify(beforeState)); }
          const result = target.exec(statement, ...args);
          if (command === "BEGIN IMMEDIATE") { begins++; owner = target; }
          else if (command === "ROLLBACK") { rollbacks++; phases.push("rollback_ack"); owner = null; }
          else if (command === "COMMIT") { commits++; phases.push("commit_ack"); owner = null; }
          return result;
        };
        return bind(target, property);
      } });
      client = factory(null, connection);
    }
    const cause: any = new Error("listed joint abort after full provisional native set");
    if (sql !== null) cause.errno = "40001"; else cause.code = "SQLITE_BUSY";
    Math.random = () => 0;
    console.error = (...args: unknown[]) => { if (typeof args[0] === "string") { try { const event = JSON.parse(args[0]); if (event.eventName === "transaction.attempt") events.push(event); } catch {} } };
    try {
      for (let revision = 1; revision <= 4; revision++) {
        const updated = { ...value, label: `joint-native-revision-${revision}` };
        beforeState = await snapshot(observer); begins = 0; rollbacks = 0; commits = 0; callbackAttempts = 0; phases = []; events = []; beforeBeginStates = [];
        operationId = `joint-native-${revision}`;
        const startedAt = performance.now(), options = Object.freeze({ replayable: true, operationId, operation: "validation.joint_native", startedAt, signal: null });
        const provisionals: any[] = [];
        const terminal = await client.withOperationTime(`2026-10-04T08:00:0${revision}.000Z`).transaction(async (transaction: any) => {
          callbackAttempts++;
          const changed = revision === 1 ? await transaction.create_Entry(updated) : await transaction.update_required_Entry_by_id_set_label(value.id, updated.label);
          const actualChanges = await query(owner, 'SELECT CAST(revision AS TEXT) AS revision FROM "__jadpo_changes" WHERE entity_id = $1 ORDER BY "__jadpo_changes".revision', [value.id]);
          const actualRevision = actualChanges.at(-1)!.revision;
          expect(actualRevision).toBe(String(revision));
          const spec = outboxSpec(updated, actualRevision);
          let enrolled: any = null;
          if (callbackAttempts === 1) enrolled = await transaction.enqueue_delivery_intent(spec);
          else enrolled = await transaction.enqueue_delivery_intent(spec);
          const provisional = await snapshot(owner); provisionals.push(provisional);
          originalLog(JSON.stringify({ kind: "joint_native_provisional_before_abort", operationId, callbackAttempts, provisional }));
          if (callbackAttempts === 1) {
            expect(provisional.source).toEqual([updated]);
            expect(provisional.changes.map((row: any) => row.revision)).toEqual(Array.from({ length: revision }, (_, i) => String(i + 1)));
            expect(provisional.changes.slice(0, -1)).toEqual(beforeState.changes);
            expect(JSON.parse(provisional.changes.at(-1)!.payload)).toEqual(updated);
            expect(provisional.intents).toHaveLength(revision);
            expect(provisional.intents.slice(0, -1)).toEqual(beforeState.intents);
            const intent = provisional.intents.at(-1)!;
            expect(intent).toMatchObject({ intent_id: enrolled.intentId, job, source_operation: spec.sourceOperation, source_entity: "Entry", source_entity_id: value.id, source_revision: actualRevision, ordering_key: orderingKey, key_sequence: actualRevision, payload_version: "1", state: "pending" });
            expect(JSON.parse(intent.payload)).toEqual(spec.payload);
            expect(provisional.keys).toEqual([{ job, ordering_key: orderingKey, next_sequence: actualRevision }]);
            throw new persistenceModule.PersistenceFault("transaction.joint_native_probe", "driver", cause);
          }
          if (enrolled !== null) {
            const duplicate = await transaction.enqueue_delivery_intent(spec);
            expect(duplicate).toEqual(enrolled); expect(await snapshot(owner)).toEqual(provisional);
          }
          return { changed, enrolled };
        }, options);
        const durable = await snapshot(observer);
        originalLog(JSON.stringify({ kind: "joint_native_source_change_outbox_retry", database: sql !== null ? "postgres" : "sqlite", operationId, originalStartedAt: startedAt, elapsedMs: performance.now() - startedAt, begins, rollbacks, commits, callbackAttempts, phases, events, beforeState, beforeBeginStates, provisionals, durable, rolledBackIntentId: provisionals[0]?.intents.at(-1)?.intent_id ?? null, committedIntentId: durable.intents.at(-1)?.intent_id ?? null, claim: "private_native_composition_no_public_create_or_rolledback_uuid_stability" }));
        expect(options.startedAt).toBe(startedAt); expect(options.operationId).toBe(operationId);
        expect([begins, rollbacks, commits, callbackAttempts]).toEqual([2, 1, 1, 2]);
        expect(phases).toEqual(["before_begin", "rollback_ack", "before_begin", "commit_ack"]); expect(beforeBeginStates).toEqual([beforeState, beforeState]);
        expect(events.map(event => [event.operationId, event.attempt, event.outcome, event.rollbackProven, event.selectedDelayMs])).toEqual([[operationId, 1, "no_commit", true, 0], [operationId, 2, "committed", false, null]]);
        expect(durable.source).toEqual([updated]); expect(durable.changes.map((row: any) => row.revision)).toEqual(Array.from({ length: revision }, (_, i) => String(i + 1)));
        expect(durable.changes.slice(0, -1)).toEqual(beforeState.changes); expect(JSON.parse(durable.changes.at(-1)!.payload)).toEqual(updated);
        expect(durable.intents).toHaveLength(revision); expect(durable.intents.slice(0, -1)).toEqual(beforeState.intents);
        expect(durable.intents.map((row: any) => row.source_revision)).toEqual(durable.changes.map((row: any) => row.revision));
        expect(durable.intents.map((row: any) => row.key_sequence)).toEqual(Array.from({ length: revision }, (_, i) => String(i + 1)));
        expect(durable.keys).toEqual([{ job, ordering_key: orderingKey, next_sequence: String(revision) }]);
        expect(durable.intents.at(-1)!.intent_id).toBe(terminal.enrolled.intentId); expect(terminal.changed).toEqual(updated);
        expect(durable.intents.at(-1)!).toMatchObject({ job, source_entity: "Entry", source_entity_id: value.id, source_revision: String(revision), ordering_key: orderingKey, payload_version: "1", state: "pending" });
        expect(JSON.parse(durable.intents.at(-1)!.payload)).toEqual(outboxSpec(updated, String(revision)).payload);
      }
    } finally { Math.random = originalRandom; console.error = originalLog; }
    expect(Math.random).toBe(originalRandom); expect(console.error).toBe(originalLog); expect(owner).toBeNull();
  });
  for (const wave of ["success", "expiry", "cancel"] as const) {
    test(`four independent seeded native worker retries preserve ${wave} contention and one original budget`, async () => {
      const value = entry(`many-native-${wave}`, null);
      await persistenceModule.persistence.withOperationTime("2026-10-04T08:00:00.000Z").transaction(async (tx: any) => {
        await tx.create_Entry(value);
        await tx.enqueue_delivery_intent({ ...outboxSpec(value), job: "validation_contention_baseline" });
      });
      const baseline = await nativeContentionSnapshot(sql ?? sqlite!, postgresUrl !== undefined, value.id);
      expect(baseline.source).toEqual([value]); expect(baseline.changes).toHaveLength(1);
      expect(baseline.intents).toHaveLength(1); expect(baseline.keys).toHaveLength(1);
      const report = await runNativeContentionWave({ module: persistencePath, pristine: nativeContentionPristinePersistence, pg: postgresUrl !== undefined, observer: sql ?? sqlite!, sourceId: value.id, wave, gate: 973020 + ["success", "expiry", "cancel"].indexOf(wave), baseline });
      expect(new Set(report.workerPIDs).size).toBe(4); expect(report.watchdog).toBe(false);
      expect(report.nativeOwnersClosed).toBe(true); expect(report.results).toHaveLength(4);
      expect(report.results.every((r: any) => r.attemptCalls <= 3)).toBe(true);
      expect(report.durable.changes).toHaveLength(wave === "success" ? 5 : 1);
    }, 15000);
  }
  test("retried updates retain a contiguous multi-revision change history on both adapters", async () => {
    const persistence = persistenceModule.persistence as any;
    const value = entry("contiguous-revisions");
    await insert(value);
    const cause: any = new Error("structured rollback between durable revisions");
    if (postgresUrl) cause.errno = "40001";
    else cause.code = "SQLITE_BUSY";
    const originalRandom = Math.random;
    const originalLog = console.error;
    const logs: any[] = [];
    console.error = (...args: unknown[]) => { if (typeof args[0] === "string") { try { logs.push(JSON.parse(args[0])); } catch {} } };
    Math.random = () => 0;
    try {
      for (let revision = 2; revision <= 4; revision++) {
        let attempts = 0;
        await persistence.withOperationTime(`2026-10-04T08:00:0${revision}.000Z`).transaction(async (transaction: any) => {
          attempts++;
          const updated = await transaction.update_required_Entry_by_id_set_label(value.id, `revision-${revision}`);
          if (attempts === 1) throw new persistenceModule.PersistenceFault("transaction.history_probe", "driver", cause);
          return updated;
        }, { replayable: true, operationId: `history-${revision}`, operation: "validation.history", startedAt: performance.now(), signal: null });
        expect(attempts).toBe(2);
      }
      const history = await rows('SELECT revision, payload FROM "__jadpo_changes" WHERE entity_id = $1 ORDER BY revision', [value.id]);
      expect(history.map(row => Number(row.revision))).toEqual([1, 2, 3, 4]);
      expect(history.map(row => JSON.parse(row.payload).label)).toEqual([value.label, "revision-2", "revision-3", "revision-4"]);
      expect(await stored(value.id)).toEqual([{ ...value, label: "revision-4" }]);
      expect(logs.filter(log => log.eventName === "transaction.attempt").map(log => log.outcome)).toEqual(["no_commit", "committed", "no_commit", "committed", "no_commit", "committed"]);
    } finally { Math.random = originalRandom; console.error = originalLog; }
  });
  test("SQLite retry budget caps at three total attempts with bounded full jitter", async () => {
    if (postgresUrl) return;
    const value = entry("retry-budget-exhausted");
    const blocker = new Database(sqlitePath, { strict: true });
    blocker.exec("BEGIN IMMEDIATE");
    const prototype: any = Database.prototype;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    const originalSetTimeout = globalThis.setTimeout;
    let begins = 0;
    let commits = 0;
    const delays: number[] = [];
    prototype.exec = function (statement: string, ...values: unknown[]) {
      if (statement.trim().toUpperCase() === "BEGIN IMMEDIATE") begins++;
      if (statement.trim().toUpperCase() === "COMMIT") commits++;
      return originalExec.call(this, statement, ...values);
    };
    (globalThis as any).setTimeout = function (callback: (...args: any[]) => void, delay?: number, ...args: any[]) {
      if (typeof delay === "number" && delay < 250) delays.push(delay);
      return originalSetTimeout.call(globalThis, callback, delay, ...args);
    };
    Math.random = () => 0.5;
    try {
      const response = await request("/entries", value);
      expect(response.status).toBe(503);
      expect((await response.json()).error.code).toBe("transaction_unavailable");
      expect(await stored(value.id)).toEqual([]);
      expect(begins).toBe(3);
      expect(commits).toBe(0);
      expect(delays).toEqual([25, 50]);
    } finally {
      if (blocker.inTransaction) blocker.exec("ROLLBACK");
      blocker.close();
      prototype.exec = originalExec;
      Math.random = originalRandom;
      (globalThis as any).setTimeout = originalSetTimeout;
    }
  });
  test("SQLite adapter commit acknowledgement loss remains unknown and never replays", async () => {
    if (postgresUrl) return;
    const value = entry("adapter-commit-acknowledgement-lost");
    let injected = false;
    let commitCount = 0;
    const prototype: any = Database.prototype;
    const originalExec = prototype.exec;
    prototype.exec = function (statement: string, ...values: unknown[]) {
      const result = originalExec.call(this, statement, ...values);
      if (!injected && statement.trim().toUpperCase() === "COMMIT") {
        injected = true;
        commitCount++;
        throw Object.assign(new Error("injected lost COMMIT acknowledgement"), { code: "SQLITE_IOERR" });
      }
      return result;
    };
    try {
      const response = await request("/entries", value);
      expect(response.status).toBe(500);
      expect((await response.json()).error.code).toBe("outcome_unknown");
      expect(await stored(value.id)).toEqual([value]);
      expect(commitCount).toBe(1);
    } finally { prototype.exec = originalExec; }
  });
  test("PostgreSQL transport faults preserve pre-BEGIN retry, read unavailability and post-COMMIT uncertainty", async () => {
    if (!postgresUrl) return;
    const direct = new URL(postgresUrl);
    const childScript = [
      'import { pathToFileURL } from "node:url";',
      'Math.random = () => 0;',
      'const app = await import(pathToFileURL(Bun.env.JADPO_VALIDATION_APP_MODULE!).href);',
      'console.log(JSON.stringify({ transportFixturePhase: "startup_complete" }));',
      'if (await Bun.stdin.text() !== "armed\\n") throw new Error("transport fixture was not armed");',
      'console.log(JSON.stringify({ transportFixturePhase: "business_started" }));',
      'const input = JSON.parse(Bun.env.JADPO_VALIDATION_INPUT!);',
      'const read = Bun.env.JADPO_VALIDATION_FAULT === "after_read";',
      'const response = await app.handleRequest(new Request("https://persistence.test" + (read ? "/lookup" : "/entries"), { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(read ? { id: input.id } : input) }));',
      'console.log(JSON.stringify({ status: response.status, body: await response.json() }));',
    ].join("\n");
    async function runThroughProxy(input: ReturnType<typeof entry>, fault: "before_begin" | "after_commit" | "after_read") {
      const sockets: Socket[] = [];
      const phaseTrace: string[] = [];
      let armed = false;
      let startupCompletions = 0;
      let beginForwarded = false;
      let commitForwarded = false;
      let responseDropped = false;
      let readsForwarded = 0;
      const proxy = createServer((downstream) => {
        const upstream = createConnection({ host: direct.hostname, port: Number(direct.port || 5432) });
        sockets.push(downstream, upstream);
        downstream.on("data", (chunk: Buffer) => {
          const query = chunk.toString("utf8").toUpperCase();
          if (query.includes("BEGIN\0")) phaseTrace.push(armed ? "business_begin" : "startup_begin");
          if (query.includes("COMMIT\0")) phaseTrace.push(armed ? "business_commit" : "startup_commit");
          if (query.includes("SELECT ") && query.includes('FROM "ENTRY"')) phaseTrace.push(armed ? "business_read" : "startup_read");
          if (armed && fault === "before_begin" && !beginForwarded && query.includes("BEGIN\0")) beginForwarded = true;
          if (armed && fault === "after_commit" && query.includes("COMMIT\0")) commitForwarded = true;
          if (armed && fault === "after_read" && query.includes("SELECT ") && query.includes('FROM "ENTRY"')) readsForwarded++;
          upstream.write(chunk);
        });
        upstream.on("data", (chunk: Buffer) => {
          if ((fault === "before_begin" && beginForwarded || fault === "after_commit" && commitForwarded || fault === "after_read" && readsForwarded > 0) && !responseDropped) {
            responseDropped = true;
            phaseTrace.push("business_response_dropped");
            downstream.destroy();
            upstream.destroy();
          } else {
            downstream.write(chunk);
          }
        });
        downstream.on("error", () => {});
        upstream.on("error", () => {});
        downstream.on("close", () => upstream.destroy());
        upstream.on("close", () => downstream.destroy());
      });
      await new Promise<void>((resolve) => proxy.listen(0, "127.0.0.1", resolve));
      const proxyUrl = new URL(postgresUrl!);
      proxyUrl.hostname = "127.0.0.1";
      proxyUrl.port = String((proxy.address() as AddressInfo).port);
      try {
        const child = Bun.spawn(
          [process.execPath, "--no-install", "--env-file=/dev/null", "-e", childScript],
          {
            cwd: root,
            env: {
              ...Bun.env,
              DATABASE_URL: proxyUrl.toString(),
              JADPO_VALIDATION_APP_MODULE: join(root, "build/target/app.ts"),
              JADPO_VALIDATION_INPUT: JSON.stringify(input),
              JADPO_VALIDATION_FAULT: fault,
            },
            stdin: "pipe",
            stdout: "pipe",
            stderr: "pipe",
          },
        );
        // The generated module awaits native schema initialization at import.
        // Arm only after its child observation, then acknowledge over stdin;
        // the child cannot start the business request before this acknowledgement.
        const stdoutPromise = (async () => {
          let stdout = "";
          let pending = "";
          const decoder = new TextDecoder();
          for await (const chunk of child.stdout) {
            const text = decoder.decode(chunk, { stream: true });
            stdout += text;
            pending += text;
            let newline: number;
            while ((newline = pending.indexOf("\n")) !== -1) {
              const line = pending.slice(0, newline);
              pending = pending.slice(newline + 1);
              if (line === JSON.stringify({ transportFixturePhase: "startup_complete" })) {
                startupCompletions++;
                if (startupCompletions !== 1) throw new Error("duplicate transport startup observation");
                phaseTrace.push("startup_complete");
                armed = true;
                phaseTrace.push("parent_armed");
                phaseTrace.push("business_allowed");
                child.stdin.write("armed\n");
                await child.stdin.end();
              }
            }
          }
          return stdout + decoder.decode();
        })();
        const [exitCode, stdout, stderr] = await Promise.all([
          child.exited,
          stdoutPromise,
          new Response(child.stderr).text(),
        ]);
        return { exitCode, stdout, stderr, phaseTrace, startupCompletions, beginForwarded, commitForwarded, responseDropped, readsForwarded };
      } finally {
        for (const socket of sockets) socket.destroy();
        await new Promise<void>((resolve) => proxy.close(() => resolve()));
      }
    }

    function expectStartupIsolated(result: Awaited<ReturnType<typeof runThroughProxy>>) {
      expect(result.startupCompletions, result.stderr).toBe(1);
      const startupComplete = result.phaseTrace.indexOf("startup_complete");
      const armed = result.phaseTrace.indexOf("parent_armed");
      const businessAllowed = result.phaseTrace.indexOf("business_allowed");
      const childPhases = result.stdout.split("\n").flatMap((line) => {
        try { const value = JSON.parse(line); return value.transportFixturePhase ? [value.transportFixturePhase] : []; }
        catch { return []; }
      });
      expect(childPhases).toEqual(["startup_complete", "business_started"]);
      expect(result.phaseTrace.slice(0, startupComplete)).toEqual(["startup_begin", "startup_commit"]);
      expect(armed).toBe(startupComplete + 1);
      expect(businessAllowed).toBe(armed + 1);
      expect(result.phaseTrace.indexOf("business_response_dropped")).toBeGreaterThan(businessAllowed);
      expect(result.phaseTrace.slice(armed + 1)).not.toContain("startup_begin");
      expect(result.phaseTrace.slice(armed + 1)).not.toContain("startup_commit");
    }

    const before = entry("proxy-pre-begin-retry");
    const beforeResult = await runThroughProxy(before, "before_begin");
    expect(beforeResult.exitCode, beforeResult.stderr).toBe(0);
    expectStartupIsolated(beforeResult);
    expect(beforeResult.beginForwarded).toBe(true);
    expect(beforeResult.responseDropped).toBe(true);
    expect(JSON.parse(beforeResult.stdout.trim().split("\n").at(-1)!).status).toBe(200);
    const beforeAttempts = beforeResult.stderr.split("\n").flatMap((line) => {
      try {
        const event = JSON.parse(line);
        return event.eventName === "transaction.attempt" ? [event] : [];
      } catch { return []; }
    });
    expect(beforeAttempts).toContainEqual(expect.objectContaining({
      phase: "begin", outcome: "no_commit", retryableCause: "postgres_connection", retryScheduled: true,
    }));
    expect(beforeAttempts).toContainEqual(expect.objectContaining({ attempt: 2, outcome: "committed" }));
    expect(await stored(before.id)).toEqual([before]);

    const after = entry("proxy-post-commit-unknown");
    const afterResult = await runThroughProxy(after, "after_commit");
    expect(afterResult.exitCode, afterResult.stderr).toBe(0);
    expectStartupIsolated(afterResult);
    expect(afterResult.commitForwarded).toBe(true);
    expect(afterResult.responseDropped).toBe(true);
    const response = JSON.parse(afterResult.stdout.trim().split("\n").at(-1)!);
    expect(response.status).toBe(500);
    expect(response.body.error.code, afterResult.stderr).toBe("outcome_unknown");
    const afterAttempts = afterResult.stderr.split("\n").flatMap((line) => {
      try {
        const event = JSON.parse(line);
        return event.eventName === "transaction.attempt" ? [event] : [];
      } catch { return []; }
    });
    expect(afterAttempts).toHaveLength(1);
    expect(afterAttempts[0]).toMatchObject({ phase: "commit", outcome: "unknown", retryScheduled: false });
    expect(await stored(after.id)).toEqual([after]);

    const readResult = await runThroughProxy(after, "after_read");
    expect(readResult.exitCode, readResult.stderr).toBe(0);
    expectStartupIsolated(readResult);
    expect(readResult.responseDropped).toBe(true);
    expect(readResult.readsForwarded).toBe(1);
    const readResponse = JSON.parse(readResult.stdout.trim().split("\n").at(-1)!);
    expect(readResponse.status, readResult.stderr).toBe(503);
    expect(readResponse.body.error.code).toBe("read_unavailable");
    expect(readResult.stderr).not.toContain('"eventName":"transaction.attempt"');
    expect(await stored(after.id)).toEqual([after]);
    console.log(JSON.stringify({
      transportFixture: "native-startup-parent-arm",
      probes: [beforeResult, afterResult, readResult].map((result, index) => ({
        fault: ["before_begin", "after_commit", "after_read"][index],
        phaseTrace: result.phaseTrace,
        startupCompletions: result.startupCompletions,
        status: JSON.parse(result.stdout.trim().split("\n").at(-1)!).status,
        responseDropped: result.responseDropped,
        readsForwarded: result.readsForwarded,
      })),
    }));
  });
  test("PostgreSQL structured serialization abort identifies the statement phase", async () => {
    if (!postgresUrl) return;
    const cause: any = new Error("synthetic server abort; classification must use errno");
    cause.errno = "40001";
    const failure = new persistenceModule.PersistenceFault("transaction.probe", "driver", cause);
    let observed: any;
    try {
      await (persistenceModule.persistence as any).transaction(async () => { throw failure; });
    } catch (error) { observed = error; }
    expect(observed).toBeInstanceOf(persistenceModule.PersistenceFault);
    expect(observed.transaction).toEqual({
      phase: "statement", database: "postgres", outcome: "no_commit",
      retryableCause: "postgres_serialization", rollbackProven: true,
      commitAcknowledged: false,
    });
  });
  test("the adapter retries one structured no-commit abort within the shared outer budget", async () => {
    const cause: any = new Error("injected structured abort");
    if (postgresUrl) cause.errno = "40001";
    else cause.code = "SQLITE_BUSY";
    const transient = new persistenceModule.PersistenceFault("transaction.retry_probe", "driver", cause);
    let attempts = 0;
    let refreshed = 0;
    const originalRandom = Math.random;
    Math.random = () => 0;
    try {
      const result = await (persistenceModule.persistence as any).transaction(async () => {
        attempts++;
        if (attempts === 1) throw transient;
        return "committed";
      }, {
        replayable: true,
        operationId: "validation-retry-probe",
        operation: "validation.retry_probe",
        startedAt: performance.now(),
        signal: null,
        refreshOperationTime: () => {
          refreshed++;
          return `2026-10-02T20:00:0${refreshed}.000Z`;
        },
      });
      expect(result).toBe("committed");
      expect(attempts).toBe(2);
      expect(refreshed).toBe(1);
    } finally { Math.random = originalRandom; }
  });
  test("the three-attempt budget is shared by both adapters", async () => {
    const cause: any = new Error("injected transient abort");
    if (postgresUrl) cause.errno = "40001";
    else cause.code = "SQLITE_BUSY";
    const failure = new persistenceModule.PersistenceFault("transaction.exhaustion_probe", "driver", cause);
    const originalRandom = Math.random;
    let attempts = 0;
    Math.random = () => 0;
    try {
      await (persistenceModule.persistence as any).transaction(async () => {
        attempts++;
        throw failure;
      }, {
        replayable: true,
        operationId: "validation-exhaustion-probe",
        operation: "validation.exhaustion_probe",
        startedAt: performance.now(),
        signal: null,
      });
    } catch (error) {
      expect(error).toBeInstanceOf(persistenceModule.PersistenceFault);
    } finally { Math.random = originalRandom; }
    expect(attempts).toBe(3);
  });
  test("a constraint rejection is never retried on either adapter", async () => {
    const cause: any = new Error("structured uniqueness violation");
    if (postgresUrl) cause.errno = "23505";
    else cause.code = "SQLITE_CONSTRAINT_UNIQUE";
    const failure = new persistenceModule.PersistenceFault("transaction.constraint_probe", "constraint", cause);
    let attempts = 0;
    try {
      await (persistenceModule.persistence as any).transaction(async () => {
        attempts++;
        throw failure;
      }, {
        replayable: true,
        operationId: "validation-constraint-probe",
        operation: "validation.constraint_probe",
        startedAt: performance.now(),
        signal: null,
      });
    } catch (error) {
      expect(error).toBeInstanceOf(persistenceModule.PersistenceFault);
      expect((error as any).transaction.retryableCause).toBeNull();
    }
    expect(attempts).toBe(1);
  });
  test("the retry window stops another attempt after its deadline on both adapters", async () => {
    const cause: any = new Error("deadline test abort");
    if (postgresUrl) cause.errno = "40001";
    else cause.code = "SQLITE_BUSY";
    const failure = new persistenceModule.PersistenceFault("transaction.deadline_probe", "driver", cause);
    let attempts = 0;
    try {
      await (persistenceModule.persistence as any).transaction(async () => {
        attempts++;
        throw failure;
      }, {
        replayable: true,
        operationId: "validation-deadline-probe",
        operation: "validation.deadline_probe",
        startedAt: performance.now() - 1001,
        signal: null,
      });
    } catch (error) {
      expect(error).toBeInstanceOf(persistenceModule.PersistenceFault);
    }
    expect(attempts).toBe(1);
  });
  test("SQLite route deadline maps cooperative transaction rollback to 504", async () => {
    if (postgresUrl) return;
    const value = entry("sqlite-route-deadline");
    const prototype: any = Database.prototype;
    const originalPrepare = prototype.prepare;
    prototype.prepare = function (statement: string, ...values: unknown[]) {
      const prepared = originalPrepare.call(this, statement, ...values);
      if (statement.trim().toUpperCase().startsWith('INSERT INTO "ENTRY"')) {
        const originalGet = prepared.get;
        prepared.get = function (...bindings: unknown[]) {
          if (bindings[1] === value.label) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 400);
          return originalGet.apply(this, bindings);
        };
      }
      return prepared;
    };
    try {
      const response = await request("/entries-deadline", value);
      expect(response.status, await response.clone().text()).toBe(504);
      expect((await response.json()).error.code).toBe("deadline_exceeded");
      expect(await stored(value.id)).toEqual([]);
    } finally {
      prototype.prepare = originalPrepare;
    }
  });
  test("PostgreSQL route deadline cancels an active query and maps proven rollback to 504", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe('DROP TRIGGER IF EXISTS validation_route_deadline ON "entry"');
    await sql!.unsafe("DROP FUNCTION IF EXISTS validation_route_deadline() CASCADE");
    await sql!.unsafe(`CREATE FUNCTION validation_route_deadline() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'postgres-route-deadline' THEN PERFORM pg_sleep(2); END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE TRIGGER validation_route_deadline BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_route_deadline()');
    const value = entry("postgres-route-deadline");
    try {
      const startedAt = performance.now();
      const response = await request("/entries-deadline", value);
      expect(performance.now() - startedAt).toBeLessThan(1500);
      expect(response.status, await response.clone().text()).toBe(504);
      expect((await response.json()).error.code).toBe("deadline_exceeded");
      expect(await stored(value.id)).toEqual([]);
    } finally {
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_route_deadline ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_route_deadline() CASCADE");
    }
  });
  test("policy-free inline reads retain the deadline and cannot start another read after expiry", async () => {
    const value = entry("inline-read-deadline");
    await insert(value);
    if (postgresUrl) {
      let release!: () => void;
      let acquired!: () => void;
      const released = new Promise<void>(resolve => { release = resolve; });
      const locked = new Promise<void>(resolve => { acquired = resolve; });
      const blocker = sql!.begin(async tx => { await tx.unsafe('LOCK TABLE "entry" IN ACCESS EXCLUSIVE MODE'); acquired(); await released; });
      await locked;
      const timer = setTimeout(release, 300);
      try {
        const response = await request("/inline-deadline", { id: value.id });
        expect(response.status).toBe(504);
        expect((await response.json()).error.code).toBe("deadline_exceeded");
      } finally { clearTimeout(timer); release(); await blocker; }
    } else {
      const prototype: any = Database.prototype;
      const originalPrepare = prototype.prepare;
      let reads = 0;
      prototype.prepare = function (statement: string, ...values: unknown[]) {
        const prepared = originalPrepare.call(this, statement, ...values);
        if (statement.startsWith('SELECT') && statement.includes('FROM "entry"')) {
          const originalAll = prepared.all;
          prepared.all = function (...bindings: unknown[]) {
            reads++;
            Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 300);
            return originalAll.apply(this, bindings);
          };
        }
        return prepared;
      };
      try {
        const response = await request("/inline-deadline", { id: value.id });
        expect(response.status).toBe(504);
        expect((await response.json()).error.code).toBe("deadline_exceeded");
        expect(reads).toBe(1);
      } finally { prototype.prepare = originalPrepare; }
    }
  });

  test("route deadline expiry during retry backoff returns 504 with no second attempt", async () => {
    const value = entry("deadline-during-backoff");
    const originalRandom = Math.random;
    const originalNow = performance.now;
    const originalTimer = globalThis.setTimeout;
    const originalExec = Database.prototype.exec;
    let attempts = 0;
    let offset = 0;
    const clockBase = originalNow.call(performance);
    if (postgresUrl) {
      await sql!.unsafe("CREATE SEQUENCE validation_backoff_deadline_count");
      await sql!.unsafe(`CREATE FUNCTION validation_backoff_deadline() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.label = 'deadline-during-backoff' THEN PERFORM nextval('validation_backoff_deadline_count'); RAISE EXCEPTION 'serialization abort' USING ERRCODE = '40001'; END IF; RETURN NEW; END $$`);
      await sql!.unsafe('CREATE TRIGGER validation_backoff_deadline BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_backoff_deadline()');
    } else {
      Database.prototype.exec = function (statement: string) {
        if (statement.trim().toUpperCase() === "BEGIN IMMEDIATE") { attempts++; throw Object.assign(new Error("injected contention"), { code: "SQLITE_BUSY" }); }
        return originalExec.call(this, statement);
      };
    }
    Math.random = () => 0.5;
    performance.now = () => clockBase + offset;
    globalThis.setTimeout = ((callback: any, delay: number, ...args: any[]) => {
      if (delay > 0 && delay <= 50) { offset = 250; return originalTimer(callback, 0, ...args); }
      return originalTimer(callback, delay, ...args);
    }) as typeof setTimeout;
    try {
      const response = await request("/entries-deadline", value);
      expect(response.status, await response.clone().text()).toBe(504);
      expect((await response.json()).error.code).toBe("deadline_exceeded");
      expect(await stored(value.id)).toEqual([]);
      if (postgresUrl) attempts = Number((await sql!.unsafe("SELECT last_value FROM validation_backoff_deadline_count"))[0].last_value);
      expect(attempts).toBe(1);
    } finally {
      Math.random = originalRandom; performance.now = originalNow; globalThis.setTimeout = originalTimer; Database.prototype.exec = originalExec;
      if (postgresUrl) {
        await sql!.unsafe('DROP TRIGGER validation_backoff_deadline ON "entry"');
        await sql!.unsafe("DROP FUNCTION validation_backoff_deadline()");
        await sql!.unsafe("DROP SEQUENCE validation_backoff_deadline_count");
      }
    }
  });
  test("generated timeout and savepoint controls classify 57014 only with expired deadline evidence", async () => {
    const originalNow = performance.now;
    let now = originalNow.call(performance);
    performance.now = () => now;
    try {
      for (const expired of [false, true]) {
        for (const phase of ["setup", "refresh", "reset", "savepoint"]) {
          const deadlineAt = now + 200;
          let timeoutSetups = 0;
          const fake: any = {
            begin: async (callback: any) => callback(fake),
            unsafe: async (statement: string) => {
              const timeoutSetup = statement.includes("set_config");
              if (timeoutSetup) timeoutSetups++;
              const faultHere = phase === "setup" && timeoutSetups === 1 || phase === "refresh" && timeoutSetup && timeoutSetups === 2 || phase === "reset" && statement.startsWith("SET LOCAL") || phase === "savepoint" && statement.startsWith("SAVEPOINT");
              if (faultHere) { if (expired) now = deadlineAt + 1; throw Object.assign(new Error("injected control failure"), { errno: "57014" }); }
              return [];
            },
          };
          const client = persistenceModule.__controlQueryProbeClient(fake, null).withDeadline(deadlineAt);
          let observed: any = null;
          try {
            await client.transaction(async (transaction: any) => {
              if (phase === "refresh") return transaction.query_optional_Entry_by_id("00000000-0000-4000-8000-000000000001");
              if (phase === "savepoint") return transaction.transaction(async () => undefined);
              return undefined;
            });
          } catch (error) { observed = error; }
          expect(observed).toBeInstanceOf(persistenceModule.PersistenceFault);
          expect(observed.deadlineExceeded).toBe(expired);
          expect(observed.transaction).toMatchObject({ phase: "statement", outcome: "no_commit", commitAcknowledged: false });
        }
      }
    } finally { performance.now = originalNow; }
  });
  test("SQLite rolls back a retry whose synchronous statement returns after the shared deadline", async () => {
    if (postgresUrl) return;
    const value = entry("sqlite-retry-deadline-in-flight", null);
    const prototype: any = Database.prototype;
    const originalPrepare = prototype.prepare;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    let inserts = 0;
    let begins = 0;
    let commits = 0;
    prototype.prepare = function (statement: string, ...values: unknown[]) {
      if (statement.trim().toUpperCase().startsWith('INSERT INTO "ENTRY"')) {
        inserts++;
        if (inserts === 1) throw Object.assign(new Error("injected pre-write contention"), { code: "SQLITE_BUSY" });
        if (inserts === 2) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1100);
      }
      return originalPrepare.call(this, statement, ...values);
    };
    prototype.exec = function (statement: string, ...values: unknown[]) {
      const command = statement.trim().toUpperCase();
      if (command === "BEGIN IMMEDIATE") begins++;
      if (command === "COMMIT") commits++;
      return originalExec.call(this, statement, ...values);
    };
    Math.random = () => 0;
    try {
      const response = await request("/entries", value);
      expect(response.status, await response.clone().text()).toBe(503);
      expect((await response.json()).error.code).toBe("transaction_unavailable");
      expect(await stored(value.id)).toEqual([]);
      expect(inserts).toBe(2);
      expect(begins).toBe(2);
      expect(commits).toBe(0);
    } finally {
      prototype.prepare = originalPrepare;
      prototype.exec = originalExec;
      Math.random = originalRandom;
    }
  });
  test("SQLite preserves an acknowledged retry commit when cancellation and deadline arrive during COMMIT", async () => {
    if (postgresUrl) return;
    const value = entry("sqlite-commit-race-preserves-ack", null);
    const controller = new AbortController();
    const prototype: any = Database.prototype;
    const originalExec = prototype.exec;
    const originalRandom = Math.random;
    const originalConsoleError = console.error;
    const attempts: any[] = [];
    let begins = 0;
    let commits = 0;
    prototype.exec = function (statement: string, ...values: unknown[]) {
      const command = statement.trim().toUpperCase();
      if (command === "BEGIN IMMEDIATE") begins++;
      if (command === "COMMIT") {
        commits++;
        if (commits === 1) throw Object.assign(new Error("injected busy COMMIT"), { code: "SQLITE_BUSY" });
        if (commits === 2) {
          controller.abort();
          Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1100);
        }
      }
      return originalExec.call(this, statement, ...values);
    };
    console.error = (...args: any[]) => {
      try {
        const event = JSON.parse(String(args[0]));
        if (event.eventName === "transaction.attempt") attempts.push(event);
      } catch {}
      return originalConsoleError.apply(console, args);
    };
    Math.random = () => 0;
    try {
      const response = await request("/entries", value, controller.signal);
      expect(response.status, await response.clone().text()).toBe(200);
      expect(await response.json()).toEqual(value);
      expect(await stored(value.id)).toEqual([value]);
      expect(begins).toBe(2);
      expect(commits).toBe(2);
      expect(attempts).toHaveLength(2);
      expect(attempts[1]).toMatchObject({ attempt: 2, outcome: "committed", commitAcknowledged: true, rollbackProven: false, cancelled: true });
    } finally {
      console.error = originalConsoleError;
      prototype.exec = originalExec;
      Math.random = originalRandom;
    }
  });
  test("PostgreSQL rolls back a retry whose active statement returns after the shared deadline", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_deadline ON "entry"');
    await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_deadline() CASCADE");
    await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_retry_deadline_count");
    await sql!.unsafe("CREATE SEQUENCE validation_retry_deadline_count");
    await sql!.unsafe(`CREATE FUNCTION validation_retry_deadline() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'postgres-retry-deadline-in-flight' THEN
          IF nextval('validation_retry_deadline_count') = 1 THEN
            RAISE EXCEPTION 'injected serialization abort' USING ERRCODE = '40001';
          END IF;
          PERFORM pg_sleep(1.1);
        END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE TRIGGER validation_retry_deadline BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_retry_deadline()');
    const value = entry("postgres-retry-deadline-in-flight", null);
    const originalRandom = Math.random;
    Math.random = () => 0;
    try {
      const response = await request("/entries", value);
      expect(response.status, await response.clone().text()).toBe(503);
      expect((await response.json()).error.code).toBe("transaction_unavailable");
      expect(await stored(value.id)).toEqual([]);
      const invocations = await sql!.unsafe("SELECT last_value FROM validation_retry_deadline_count");
      expect(Number(invocations[0].last_value)).toBe(2);
    } finally {
      Math.random = originalRandom;
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_deadline ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_deadline() CASCADE");
      await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_retry_deadline_count");
    }
  });
  test("PostgreSQL preserves an acknowledged retry commit when cancellation and deadline arrive during COMMIT", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_commit_race ON "entry"');
    await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_commit_race() CASCADE");
    await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_retry_commit_race_count");
    await sql!.unsafe("CREATE SEQUENCE validation_retry_commit_race_count");
    await sql!.unsafe(`CREATE FUNCTION validation_retry_commit_race() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'postgres-commit-race-preserves-ack' THEN
          IF nextval('validation_retry_commit_race_count') = 1 THEN
            RAISE EXCEPTION 'injected serialization abort' USING ERRCODE = '40001';
          END IF;
          PERFORM pg_sleep(1.1);
        END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE CONSTRAINT TRIGGER validation_retry_commit_race AFTER INSERT ON "entry" DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validation_retry_commit_race()');
    const value = entry("postgres-commit-race-preserves-ack", null);
    const controller = new AbortController();
    const originalRandom = Math.random;
    const originalConsoleError = console.error;
    const attempts: any[] = [];
    console.error = (...args: any[]) => {
      try {
        const event = JSON.parse(String(args[0]));
        if (event.eventName === "transaction.attempt") attempts.push(event);
      } catch {}
      return originalConsoleError.apply(console, args);
    };
    Math.random = () => 0;
    try {
      const responsePromise = request("/entries", value, controller.signal);
      const pollStartedAt = performance.now();
      let invocations = 0;
      while (invocations < 2 && performance.now() - pollStartedAt < 5000) {
        const rows = await sql!.unsafe("SELECT last_value FROM validation_retry_commit_race_count");
        invocations = Number(rows[0]?.last_value ?? 0);
        if (invocations < 2) await new Promise(resolve => setTimeout(resolve, 10));
      }
      expect(invocations).toBe(2);
      controller.abort();
      const response = await responsePromise;
      expect(response.status, await response.clone().text()).toBe(200);
      expect(await response.json()).toEqual(value);
      expect(await stored(value.id)).toEqual([value]);
      expect(attempts).toHaveLength(2);
      expect(attempts[1]).toMatchObject({ attempt: 2, phase: "commit", outcome: "committed", commitAcknowledged: true, rollbackProven: false, cancelled: true });
    } finally {
      console.error = originalConsoleError;
      Math.random = originalRandom;
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_commit_race ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_commit_race() CASCADE");
      await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_retry_commit_race_count");
    }
  });
  test("PostgreSQL cancellation during retry backoff prevents another attempt", async () => {
    if (!postgresUrl) return;
    const cause: any = new Error("cancellation test abort");
    cause.errno = "40001";
    const failure = new persistenceModule.PersistenceFault("transaction.cancel_probe", "driver", cause);
    const controller = new AbortController();
    const originalRandom = Math.random;
    let attempts = 0;
    Math.random = () => 0.5;
    const cancel = setTimeout(() => controller.abort(), 1);
    try {
      await (persistenceModule.persistence as any).transaction(async () => {
        attempts++;
        throw failure;
      }, {
        replayable: true,
        operationId: "validation-cancel-probe",
        operation: "validation.cancel_probe",
        startedAt: performance.now(),
        signal: controller.signal,
      });
    } catch (error) {
      expect(error).toBeInstanceOf(persistenceModule.PersistenceFault);
    } finally {
      clearTimeout(cancel);
      Math.random = originalRandom;
    }
    expect(attempts).toBe(1);
  });
  test("PostgreSQL cancellation during an in-flight statement rolls back before commit and does not retry", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe('DROP TRIGGER IF EXISTS validation_cancel_in_flight ON "entry"');
    await sql!.unsafe("DROP FUNCTION IF EXISTS validation_cancel_in_flight() CASCADE");
    await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_cancel_in_flight_count");
    await sql!.unsafe("CREATE SEQUENCE validation_cancel_in_flight_count");
    await sql!.unsafe(`CREATE FUNCTION validation_cancel_in_flight() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'cancel-in-flight' THEN
          PERFORM nextval('validation_cancel_in_flight_count');
          PERFORM pg_sleep(1);
        END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE TRIGGER validation_cancel_in_flight BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_cancel_in_flight()');
    const value = entry("cancel-in-flight", null);
    const controller = new AbortController();
    const response = request("/entries", value, controller.signal);
    const cancel = setTimeout(() => controller.abort(), 100);
    try {
      await response;
      expect(await stored(value.id)).toEqual([]);
      const invocations = await rows("SELECT last_value FROM validation_cancel_in_flight_count");
      expect(Number(invocations[0].last_value)).toBe(1);
    } finally {
      clearTimeout(cancel);
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_cancel_in_flight ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_cancel_in_flight() CASCADE");
      await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_cancel_in_flight_count");
    }
  });
  test("SQLite captured cancellation at a returning native statement rolls back before commit without replay", async () => {
    if (postgresUrl) return;
    // Cooperative boundary injection: the original native write returns before
    // this fixture aborts its captured request. No synchronous preemption claim.
    for (const mode of ["abort_at_return", "normal_commit", "abort_before_call"] as const) {
      const value = entry(`sqlite-statement-${mode}`, null);
      const controller = new AbortController();
      const capturedRequest = new Request("https://persistence.test/entries", {
        method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify(value), signal: controller.signal,
      });
      const prototype: any = Database.prototype;
      const originalPrepare = prototype.prepare, originalExec = prototype.exec;
      const originalLog = console.error;
      const events: any[] = [];
      let begins = 0, rollbacks = 0, commits = 0, attemptedWrites = 0;
      let activeNativeTransaction = false, provisionalRow = false;
      let capturedSignalAbortedAtStatementReturn: boolean | null = null;
      prototype.prepare = function (statement: string, ...bindings: unknown[]) {
        const prepared = originalPrepare.call(this, statement, ...bindings);
        if (statement.trim().toUpperCase().startsWith('INSERT INTO "ENTRY"')) {
          const writer = this, originalGet = prepared.get;
          prepared.get = (...values: unknown[]) => {
            attemptedWrites++;
            const result = originalGet.apply(prepared, values);
            activeNativeTransaction = writer.inTransaction === true;
            // Use original prepare on that same native writer, never an observer
            // connection or a recursive interception of the business statement.
            const inspect = originalPrepare.call(writer, 'SELECT id FROM "entry" WHERE id = ?1');
            try { provisionalRow = inspect.get(value.id)?.id === value.id; }
            finally { inspect.finalize(); }
            if (mode === "abort_at_return") controller.abort();
            capturedSignalAbortedAtStatementReturn = capturedRequest.signal.aborted;
            return result;
          };
        }
        return prepared;
      };
      prototype.exec = function (statement: string, ...values: unknown[]) {
        const command = statement.trim().toUpperCase();
        if (command === "BEGIN IMMEDIATE") begins++;
        if (command === "ROLLBACK") rollbacks++;
        if (command === "COMMIT") commits++;
        return originalExec.call(this, statement, ...values);
      };
      console.error = (...args: unknown[]) => {
        try { const event = JSON.parse(String(args[0])); if (event.eventName === "transaction.attempt") events.push(event); } catch {}
        originalLog(...args);
      };
      try {
        if (mode === "abort_before_call") controller.abort();
        const response = await app.handleRequest(capturedRequest);
        originalLog(JSON.stringify({ kind: "sqlite_cooperative_statement_cancellation", mode, status: response.status, nativeStatementReturned: attemptedWrites === 1, activeNativeTransaction, provisionalRow, capturedSignalAbortedAtStatementReturn, capturedRequestSignalAborted: capturedRequest.signal.aborted, begins, rollbacks, commits, attemptedWrites, events, claim: "cooperative_boundary_injection_not_synchronous_preemption" }));
        if (mode === "normal_commit") {
          expect(response.status).toBe(200); expect(await response.json()).toEqual(value);
          expect(await stored(value.id)).toEqual([value]);
          expect(await rows('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id])).toEqual([{ revision: 1 }]);
          expect([begins, rollbacks, commits, attemptedWrites]).toEqual([1, 0, 1, 1]);
          expect(events).toHaveLength(1); expect(events[0]).toMatchObject({ attempt: 1, outcome: "committed", commitAcknowledged: true, retryScheduled: false, cancelled: false });
          expect(activeNativeTransaction).toBe(true); expect(provisionalRow).toBe(true); expect(capturedSignalAbortedAtStatementReturn).toBe(false);
        } else {
          expect(response.status).toBe(503); expect((await response.json()).error.code).toBe("transaction_unavailable");
          expect(await stored(value.id)).toEqual([]);
          expect(await rows('SELECT revision FROM "__jadpo_changes" WHERE entity_id = $1', [value.id])).toEqual([]);
          expect(events).toHaveLength(1); expect(events[0]).toMatchObject({ attempt: 1, outcome: "no_commit", commitAcknowledged: false, retryScheduled: false, cancelled: true });
          expect(capturedRequest.signal.aborted).toBe(true);
          if (mode === "abort_at_return") {
            expect([begins, rollbacks, commits, attemptedWrites]).toEqual([1, 1, 0, 1]);
            expect(activeNativeTransaction).toBe(true); expect(provisionalRow).toBe(true); expect(capturedSignalAbortedAtStatementReturn).toBe(true);
            expect(events[0]).toMatchObject({ phase: "statement", rollbackProven: true, retryableCause: null });
          } else { expect([begins, rollbacks, commits, attemptedWrites]).toEqual([0, 0, 0, 0]); expect(activeNativeTransaction).toBe(false); expect(provisionalRow).toBe(false); expect(capturedSignalAbortedAtStatementReturn).toBeNull(); }
        }

      } finally { prototype.prepare = originalPrepare; prototype.exec = originalExec; console.error = originalLog; }
    }
  });
  test("PostgreSQL SQLSTATE 40001 retries the complete nested action once", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe("DROP TRIGGER IF EXISTS validation_retry_serialization ON \"entry\"");
    await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_serialization() CASCADE");
    await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_retry_serialization_once");
    await sql!.unsafe("CREATE SEQUENCE validation_retry_serialization_once");
    await sql!.unsafe(`CREATE FUNCTION validation_retry_serialization() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'serialization-once' AND nextval('validation_retry_serialization_once') = 1 THEN
          RAISE EXCEPTION 'injected serialization abort' USING ERRCODE = '40001';
        END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE TRIGGER validation_retry_serialization BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_retry_serialization()');
    const first = entry("serialization-outer-first", null);
    const second = entry("serialization-once", null);
    const originalRandom = Math.random;
    const originalDate = globalThis.Date;
    const refreshedAt = "2026-10-02T20:00:01.000Z";
    let clockReads = 0;
    class SteppedDate extends originalDate {
      toISOString() {
        return clockReads++ === 0 ? "2026-10-02T20:00:00.000Z" : refreshedAt;
      }
    }
    Math.random = () => 0;
    (globalThis as any).Date = SteppedDate;
    try {
      const response = await request("/pair", { first, second });
      expect(response.status, await response.clone().text()).toBe(200);
      expect(await response.json()).toEqual(second);
      expect(await stored(first.id)).toEqual([first]);
      expect(await stored(second.id)).toEqual([second]);
      const revisions = await rows(`SELECT entity_id, revision, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at FROM "__jadpo_changes" WHERE entity_id IN ($1, $2) ORDER BY entity_id`, [first.id, second.id]);
      expect(revisions.map((row) => ({ ...row, revision: Number(row.revision) }))).toEqual([
        { entity_id: first.id, revision: 1, created_at: refreshedAt },
        { entity_id: second.id, revision: 1, created_at: refreshedAt },
      ]);
      expect(clockReads).toBeGreaterThanOrEqual(2);
      const sequence = await sql!.unsafe("SELECT last_value::INTEGER AS attempts FROM validation_retry_serialization_once");
      expect(Number(sequence[0]?.attempts)).toBe(2);
    } finally {
      Math.random = originalRandom;
      (globalThis as any).Date = originalDate;
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_serialization ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_serialization() CASCADE");
      await sql!.unsafe("DROP SEQUENCE IF EXISTS validation_retry_serialization_once");
    }
  });
  test("PostgreSQL retry exhaustion returns a safe unavailable response", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_exhaustion ON "entry"');
    await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_exhaustion() CASCADE");
    await sql!.unsafe(`CREATE FUNCTION validation_retry_exhaustion() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'serialization-exhaustion' THEN
          RAISE EXCEPTION 'injected exhausted serialization failure' USING ERRCODE = '40001';
        END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE TRIGGER validation_retry_exhaustion BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_retry_exhaustion()');
    const value = entry("serialization-exhaustion", null);
    const originalRandom = Math.random;
    const originalConsoleError = console.error;
    const attempts: any[] = [];
    console.error = (...args: any[]) => {
      try {
        const event = JSON.parse(String(args[0]));
        if (event.eventName === "transaction.attempt") attempts.push(event);
      } catch {}
      return originalConsoleError.apply(console, args);
    };
    Math.random = () => 0;
    try {
      const response = await request("/entries", value);
      expect(response.status).toBe(503);
      expect((await response.json()).error.code).toBe("transaction_unavailable");
      expect(await stored(value.id)).toEqual([]);
      expect(attempts).toHaveLength(3);
      expect(attempts.map((event) => event.attempt)).toEqual([1, 2, 3]);
      expect(attempts.map((event) => event.retryScheduled)).toEqual([true, true, false]);
      expect(attempts.every((event) => event.outcome === "no_commit")).toBe(true);
    } finally {
      console.error = originalConsoleError;
      Math.random = originalRandom;
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_exhaustion ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_exhaustion() CASCADE");
    }
  });
  test("PostgreSQL deadlock across two app processes retries one outer transaction", async () => {
    if (!postgresUrl) return;
    await sql!.unsafe(`CREATE OR REPLACE FUNCTION validation_retry_deadlock() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.label = 'deadlock-a' OR NEW.label = 'deadlock-b' THEN
          PERFORM set_config('deadlock_timeout', '50ms', true);
          IF NEW.label = 'deadlock-a' THEN
            PERFORM pg_advisory_xact_lock(928001);
            PERFORM pg_sleep(0.2);
            PERFORM pg_advisory_xact_lock(928002);
          ELSE
            PERFORM pg_advisory_xact_lock(928002);
            PERFORM pg_sleep(0.2);
            PERFORM pg_advisory_xact_lock(928001);
          END IF;
        END IF;
        RETURN NEW;
      END $$`);
    await sql!.unsafe('CREATE TRIGGER validation_retry_deadlock BEFORE INSERT ON "entry" FOR EACH ROW EXECUTE FUNCTION validation_retry_deadlock()');
    const first = entry("deadlock-a", null);
    const second = entry("deadlock-b", null);
    const childScript = [
      'import { pathToFileURL } from "node:url";',
      'Math.random = () => 0;',
      'const app = await import(pathToFileURL(Bun.env.JADPO_VALIDATION_APP_MODULE!).href);',
      'console.log("READY");',
      'await new Promise(resolve => process.stdin.once("data", resolve));',
      'const input = JSON.parse(Bun.env.JADPO_VALIDATION_INPUT!);',
      'const response = await app.handleRequest(new Request("https://persistence.test/entries", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(input) }));',
      'console.log(JSON.stringify({ status: response.status, body: await response.json() }));',
      'process.exitCode = response.status === 200 ? 0 : 1;',
    ].join("\n");
    const values = [first, second];
    const children = values.map((input) => Bun.spawn(
      [process.execPath, "--no-install", "--env-file=/dev/null", "-e", childScript],
      {
        cwd: root,
        env: {
          ...Bun.env,
          JADPO_VALIDATION_APP_MODULE: join(root, "build/target/app.ts"),
          JADPO_VALIDATION_INPUT: JSON.stringify(input),
        },
        stdin: "pipe", stdout: "pipe", stderr: "pipe",
      },
    ));
    try {
      const ready = await Promise.all(children.map(async (child) => {
        const reader = child.stdout.getReader();
        let output = "";
        while (!output.includes("READY")) {
          const chunk = await reader.read();
          if (chunk.done) throw new Error(`PostgreSQL retry worker exited before readiness: ${output}`);
          output += new TextDecoder().decode(chunk.value);
        }
        return { child, reader, output };
      }));
      for (const worker of ready) { worker.child.stdin.write("go\n"); worker.child.stdin.end(); }
      const results = await Promise.all(ready.map(async (worker) => {
        let stdout = worker.output;
        while (true) {
          const chunk = await worker.reader.read();
          if (chunk.done) break;
          stdout += new TextDecoder().decode(chunk.value);
        }
        worker.reader.releaseLock();
        const [exitCode, stderr] = await Promise.all([
          worker.child.exited,
          new Response(worker.child.stderr).text(),
        ]);
        return { exitCode, stdout, stderr };
      }));
      for (const result of results) {
        expect(result.exitCode, result.stderr).toBe(0);
        const resultLine = result.stdout.trim().split("\n").at(-1)!;
        expect(JSON.parse(resultLine).status).toBe(200);
      }
      const attempts = results.flatMap(result => result.stderr.split("\n").flatMap((line) => {
        try {
          const event = JSON.parse(line);
          return event.eventName === "transaction.attempt" ? [event] : [];
        } catch { return []; }
      }));
      const deadlock = attempts.find(event => event.retryableCause === "postgres_deadlock" && event.retryScheduled);
      expect(deadlock).toBeDefined();
      expect(attempts).toContainEqual(expect.objectContaining({
        operationId: deadlock?.operationId, attempt: 2, outcome: "committed", commitAcknowledged: true,
      }));
      expect(await stored(first.id)).toEqual([first]);
      expect(await stored(second.id)).toEqual([second]);
      const changes = await rows('SELECT entity_id, revision FROM "__jadpo_changes" WHERE entity_id IN ($1, $2) ORDER BY entity_id', [first.id, second.id]);
      expect(changes).toEqual([{ entity_id: first.id, revision: "1" }, { entity_id: second.id, revision: "1" }]);
    } finally {
      for (const child of children) {
        if (child.exitCode === null) child.kill();
      }
      await sql!.unsafe('DROP TRIGGER IF EXISTS validation_retry_deadlock ON "entry"');
      await sql!.unsafe("DROP FUNCTION IF EXISTS validation_retry_deadlock()");
    }
  });
  test("checked reads classify structured availability causes without message matching or replay", async () => {
    const persistence = persistenceModule.persistence as any;
    const original = persistence.query_optional_Entry_by_id;
    const transforms = ["withDeadline", "withSignal", "withOperationTime", "withPolicy"] as const;
    const originals = new Map(transforms.map((method) => [method, persistence[method]]));
    let fault: Error;
    let reads = 0;
    const instrument = (client: any): any => {
      client.query_optional_Entry_by_id = async () => { reads++; throw fault; };
      for (const method of transforms) {
        const transform = client[method];
        if (typeof transform === "function") client[method] = (...args: any[]) => instrument(transform.apply(client, args));
      }
      return client;
    };
    instrument(persistence);
    const scenarios = [
      { code: "SQLITE_BUSY", kind: "driver", expected: "read_unavailable", status: 503 },
      { code: "SQLITE_LOCKED", kind: "driver", expected: "read_unavailable", status: 503 },
      { code: "ERR_POSTGRES_CONNECTION_CLOSED", kind: "driver", expected: "read_unavailable", status: 503 },
      { code: "42P01", kind: "driver", expected: "internal_fault", status: 500 },
      { code: "57014", kind: "driver", expected: "internal_fault", status: 500 },
      { code: undefined, kind: "driver", expected: "internal_fault", status: 500 },
      { code: "SQLITE_BUSY", kind: "cardinality", expected: "internal_fault", status: 500 },
      { code: "SQLITE_BUSY", kind: "unknown", expected: "outcome_unknown", status: 500 },
    ];
    try {
      for (const scenario of scenarios) {
        reads = 0;
        const cause = Object.assign(new Error("SECRET_SQLITE_BUSY SQL and credentials canary"), { code: scenario.code });
        fault = new persistenceModule.PersistenceFault("query.Entry.id", scenario.kind, cause);
        const response = await request("/lookup", { id: entry("read-probe").id });
        expect(response.status).toBe(scenario.status);
        const body = await response.json();
        expect(body.error.code).toBe(scenario.expected);
        expect(Object.keys(body.error).sort()).toEqual(["code", "message", "request_id"]);
        expect(body.error.request_id).toBe(response.headers.get("x-request-id"));
        expect(JSON.stringify(body)).not.toContain("canary");
        expect(response.headers.get("retry-after")).toBeNull();
        expect(reads).toBe(1);
      }
      reads = 0;
      fault = new persistenceModule.PersistenceFault("query.Entry.id", "driver", Object.assign(new Error("busy"), { code: "SQLITE_BUSY" }));
      const actionResponse = await request("/lookup-action", { id: entry("action-read").id });
      expect(actionResponse.status).toBe(500);
      expect((await actionResponse.json()).error.code).toBe("internal_fault");
      expect(reads).toBe(1);
      const inventory = JSON.parse(readFileSync(join(root, "build/inventory/routes.json"), "utf8"));
      const route = inventory.routes.find((r: any) => r.path === "/lookup");
      expect(route.operational_failures.find((f: any) => f.code === "read_unavailable"))
        .toMatchObject({ http_status: 503, automatic_retry: false });
      const audit = JSON.parse(readFileSync(join(root, "build/audit/failures.json"), "utf8"));
      // The global built-in registry includes native relationship conflict;
      // this read/Entry-only route has no checked foreign-key mutation edge.
      expect(route.operational_failures.some((failure: any) => failure.code === "relationship_conflict")).toBe(false);
      expect(audit.operational_failures).toEqual([...route.operational_failures, {
        code: "relationship_conflict", http_status: 409,
        message: "The operation conflicts with a relationship.",
        automatic_retry: false, condition: "checked_reachable_native_foreign_key_constraint",
      }]);
      const api = JSON.parse(readFileSync(join(root, "build/openapi/openapi.json"), "utf8"));
      expect(api.paths["/lookup"].post.responses["503"].content["application/json"].schema.properties.error.properties.code.enum)
        .toContain("read_unavailable");
    } finally {
      persistence.query_optional_Entry_by_id = original;
      for (const [method, transform] of originals) persistence[method] = transform;
    }
  });

  test("SQLite native exclusive lock maps a checked read to unavailable", async () => {
    if (postgresUrl) return;
    const value = entry("locked-read");
    await insert(value);
    sqlite!.exec("BEGIN EXCLUSIVE");
    try {
      const response = await request("/lookup", { id: value.id });
      expect(response.status).toBe(503);
      expect((await response.json()).error.code).toBe("read_unavailable");
      expect(response.headers.get("retry-after")).toBeNull();
    } finally { sqlite!.exec("ROLLBACK"); }
    expect(await stored(value.id)).toEqual([value]);
  });

  test("committed write with an injected lost acknowledgement returns uncertainty without replay", async () => {
    // Boundary fault injection, not network-phase classification evidence.
    // The original generated transaction really commits on each tested adapter.
    const persistence = persistenceModule.persistence as any;
    const original = persistence.withDeadline;
    let transactions = 0;
    const instrument = (client: any): any => {
      const transaction = client.transaction.bind(client);
      client.transaction = async (...args: any[]) => {
        transactions++;
        await transaction(...args);
        throw new persistenceModule.PersistenceFault(
          "transaction.commit", "unknown",
          new persistenceModule.OutcomeUnknownFault("transaction.commit"),
        );
      };
      for (const method of ["withDeadline", "withSignal", "withOperationTime", "withPolicy"] as const) {
        const transform = client[method];
        if (typeof transform === "function") {
          client[method] = (...args: any[]) => instrument(transform.apply(client, args));
        }
      }
      return client;
    };
    persistence.withDeadline = function (deadline: number | null) {
      return instrument(original.call(this, deadline));
    };
    const value = entry("committed-acknowledgement-lost");
    try {
      const response = await request("/entries", value);
      expect(response.status).toBe(500);
      expect(response.headers.get("retry-after")).toBeNull();
      expect(response.headers.get("x-request-id")).toMatch(/^req_/);
      const body = await response.json();
      expect(body).toEqual({ error: {
        code: "outcome_unknown", message: "The operation may have completed.",
        request_id: response.headers.get("x-request-id"),
      }});
      expect(JSON.stringify(body)).not.toContain("transaction.commit");
      expect(transactions).toBe(1);
      expect(await stored(value.id)).toEqual([value]);
      const inventory = JSON.parse(readFileSync(join(root,"build/inventory/routes.json"),"utf8"));
      const route = inventory.routes.find((r: any) => r.path === "/entries");
      expect(route.operational_boundary).toBe("bun_http");
      expect(route.operational_failures.find((f: any) => f.code === "outcome_unknown")).toMatchObject({http_status:500,semantic_http_default:null,automatic_retry:false});
      expect(route.operational_failures.find((f: any) => f.code === "transaction_unavailable")).toMatchObject({http_status:503,semantic_http_default:503,automatic_retry:false});
      const audit = JSON.parse(readFileSync(join(root,"build/audit/failures.json"),"utf8"));
      // The global built-in registry includes native relationship conflict;
      // this read/Entry-only route has no checked foreign-key mutation edge.
      expect(route.operational_failures.some((failure: any) => failure.code === "relationship_conflict")).toBe(false);
      expect(audit.operational_failures).toEqual([...route.operational_failures, {
        code: "relationship_conflict", http_status: 409,
        message: "The operation conflicts with a relationship.",
        automatic_retry: false, condition: "checked_reachable_native_foreign_key_constraint",
      }]);
      const api = JSON.parse(readFileSync(join(root,"build/openapi/openapi.json"),"utf8"));
      expect(api.paths["/entries"].post.responses["500"].content["application/json"].schema.properties.error.properties.code.enum).toContain("outcome_unknown");
      expect(api.paths["/entries"].post.responses["503"].content["application/json"].schema.properties.error.properties.code.enum).toContain("transaction_unavailable");
    } finally { persistence.withDeadline = original; }
  });
  test("propagated nested failure rolls back both entries including a nullable insert", async () => {
    const first = entry("propagated-first", null);
    const second = entry("propagated-second", null);
    const response = await request("/propagate", { first, second });
    expect(response.status).toBe(422);
    expect((await response.json()).error.code).toBe("deliberate_failure");
    expect(await stored(first.id)).toEqual([]);
    expect(await stored(second.id)).toEqual([]);
  });
  test("handled nested failure rolls back only its nullable insert and change record", async () => {
    const first = entry("recovered-first", null);
    const second = entry("recovered-second", null);
    const response = await request("/recover", { first, second });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(first);
    expect(await stored(first.id)).toEqual([first]);
    expect(await stored(second.id)).toEqual([]);
    const changes = await rows('SELECT entity_id FROM "__jadpo_changes" WHERE entity_id = $1 OR entity_id = $2 ORDER BY entity_id', [first.id, second.id]);
    expect(changes).toEqual([{ entity_id: first.id }]);
  });
  test("corrupt persisted semantic values are contained before entering trusted code", async () => {
    const value = entry("corrupt-read");
    await insert(value);
    await rows('UPDATE "entry" SET note = $1 WHERE id = $2 RETURNING id', ["invalid-stored-email", value.id]);
    await expectOperationalFailure(await request("/find", { id: value.id }));
    expect(await stored(value.id)).toEqual([{ ...value, note: "invalid-stored-email" }]);
  });
  test("parameterized text cannot change SQL shape", async () => {
    const value = entry(`a'); DELETE FROM entry; --`);
    await insert(value);
    const response = await request("/find", { id: value.id });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(value);
  });
});
