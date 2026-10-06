import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

// Disposable component fixture only: no authored callback or public admission.
const seeds = [0x4a414450, 0x1badb002, 0x91e10da5, 0x00c0ffee];
const references = [[2932296559, 3799187970], [143896441, 2796163716], [2775147200, 1155626783], [712164981, 2160239952]];
const job = "validation_delivery", baselineJob = "validation_contention_baseline";
const ownerSeam = "\nexport function __contentionNativeOwners() { return Object.freeze({ postgres, sqlite, createPersistenceClient }); }\n";
type Wave = "success" | "expiry" | "cancel";
function check(value: unknown, code: string): asserts value { if (!value) throw new Error(`contention.${code}`); }
const canonical = (value: unknown) => JSON.stringify(value, (_key, item) => item && typeof item === "object" && !Array.isArray(item) ? Object.fromEntries(Object.keys(item).sort().map(key => [key, item[key]])) : item);
function equal(actual: unknown, expected: unknown, code: string) { check(canonical(actual) === canonical(expected), code); }
const bind = (target: any, property: PropertyKey) => { const value = Reflect.get(target, property, target); return typeof value === "function" ? value.bind(target) : value; };
async function query(owner: any, pg: boolean, statement: string, values: any[] = []) { return pg ? Array.from(await owner.unsafe(statement, values)) : owner.query(statement).all(...values); }
async function snapshot(owner: any, pg: boolean, id: string) {
  return {
    source: await query(owner, pg, 'SELECT * FROM "entry" WHERE id = $1', [id]),
    changes: await query(owner, pg, 'SELECT *, CAST(revision AS TEXT) AS revision FROM "__jadpo_changes" WHERE entity_id = $1 ORDER BY "__jadpo_changes".revision', [id]),
    intents: await query(owner, pg, 'SELECT *, CAST(key_sequence AS TEXT) AS key_sequence FROM "__jadpo_deliveries_v1" WHERE source_entity_id = $1 ORDER BY job, "__jadpo_deliveries_v1".key_sequence', [id]),
    keys: await query(owner, pg, 'SELECT *, CAST(next_sequence AS TEXT) AS next_sequence FROM "__jadpo_delivery_keys_v1" WHERE ordering_key = $1 ORDER BY job', [id]),
  };
}

async function childMain() {
  const config = JSON.parse(Bun.env.JADPO_CONTENTION_CHILD!);
  const pg = config.pg === true;
  check(Number.isInteger(config.worker) && config.worker >= 0 && config.worker < 4, "worker_identity");
  const module = await import(pathToFileURL(config.module).href);
  const owners = module.__contentionNativeOwners();
  check((owners.postgres !== null) === pg && (owners.sqlite !== null) === !pg, "original_owner");
  const original = pg ? owners.postgres : owners.sqlite;
  const initialBackend = pg ? Number((await original.unsafe("SELECT pg_backend_pid() AS pid"))[0].pid) : null;
  if (config.reader) {
    const state = await snapshot(original, pg, config.id);
    if (pg) await original.close(); else original.close();
    console.log(JSON.stringify({ state, ownerClosed: true, pid: process.pid, initialBackend }));
    return;
  }
  check(typeof process.send === "function", "ipc_missing");
  const send = (packet: any) => process.send!({ ...packet, worker: config.worker, pid: process.pid });
  const commands = new Map<string, (() => void)>();
  const received = new Set<string>();
  const waitCommand = (command: string) => cleanup || received.has(command) ? Promise.resolve() : new Promise<void>(resolve => commands.set(command, resolve));
  const controller = new AbortController();
  let cleanup = false;
  process.on("message", (packet: any) => {
    check(packet && packet.worker === config.worker && ["go", "release1", "release2", "abort", "cleanup"].includes(packet.command), "command");
    if (packet.command === "abort") { controller.abort(); send({ kind: "aborted", aborted: controller.signal.aborted }); }
    if (packet.command === "cleanup") { cleanup = true; controller.abort(); for (const release of commands.values()) release(); commands.clear(); }
    received.add(packet.command); const release = commands.get(packet.command); commands.delete(packet.command); release?.();
  });
  const originalRandom = Math.random, originalTimer = globalThis.setTimeout, originalLog = console.error, originalNow = performance.now;
  const phases: any[] = [], events: any[] = [], draws: number[] = [], waits: any[] = [], provisionals: any[] = [];
  let startedAt = 0, attemptCalls = 0, begins = 0, rollbacks = 0, commits = 0, callbacks = 0, activeOwner: any = null;
  let seed = config.omit === "seeds" ? seeds[0] : seeds[config.worker], selected: { index: number; fraction: number; drawAt: number } | null = null;
  let backend: number | null = null, expiryTimer: ReturnType<typeof setTimeout> | null = null;
  const native = new Proxy(original, { get(target, property) {
    if (pg && property === "begin") return async (work: (tx: any) => Promise<any>) => {
      attemptCalls++; phases.push({ phase: "begin_delegate", at: performance.now() - startedAt });
      try {
        const result = await target.begin(async (tx: any) => { begins++; activeOwner = tx; try { return await work(tx); } finally { activeOwner = null; } });
        commits++; phases.push({ phase: "commit_ack", at: performance.now() - startedAt }); return result;
      } catch (error) { rollbacks++; phases.push({ phase: "rollback_ack", at: performance.now() - startedAt }); throw error; }
    };
    if (!pg && property === "exec") return (statement: string, ...args: any[]) => {
      const command = statement.trim().toUpperCase();
      if (command === "BEGIN IMMEDIATE") { attemptCalls++; phases.push({ phase: "begin_call", at: performance.now() - startedAt }); }
      try {
        const result = target.exec(statement, ...args);
        if (command === "BEGIN IMMEDIATE") { begins++; activeOwner = target; }
        if (command === "ROLLBACK") { rollbacks++; activeOwner = null; phases.push({ phase: "rollback_ack", at: performance.now() - startedAt }); }
        if (command === "COMMIT") { commits++; activeOwner = null; phases.push({ phase: "commit_ack", at: performance.now() - startedAt }); }
        return result;
      } catch (error) {
        if (command === "BEGIN IMMEDIATE") { const code = (error as any).code; phases.push({ phase: "failed_begin", code, opened: target.inTransaction, at: performance.now() - startedAt }); send({ kind: "failed_begin", code, opened: target.inTransaction, attempt: attemptCalls }); }
        throw error;
      }
    };
    return bind(target, property);
  } });
  const client = owners.createPersistenceClient(pg ? native : null, pg ? null : native);
  let terminal: any = null, fault: any = null;
  send({ kind: "ready", pg, initialBackend });
  try {
    await waitCommand("go");
    Math.random = () => { check(selected === null && draws.length < 2, "draw_attribution"); seed = (Math.imul(1664525, seed) + 1013904223) >>> 0; draws.push(seed); selected = { index: draws.length, fraction: seed / 4294967296, drawAt: performance.now() }; return selected.fraction; };
    globalThis.setTimeout = ((callback: (...args: any[]) => void, delay?: number, ...args: any[]) => {
      if (selected === null) return originalTimer(callback, delay, ...args);
      const draw = selected; selected = null;
      check(typeof delay === "number" && delay > 0 && delay <= (draw.index === 1 ? 50 : 100), "attributed_timer");
      const entry = { index: draw.index, fraction: draw.fraction, delay, drawAt: draw.drawAt - startedAt, timerAt: performance.now() - startedAt, elapsedAt: null as number | null, releaseAt: null as number | null };
      waits.push(entry);
      return originalTimer(async () => {
        entry.elapsedAt = performance.now() - startedAt;
        send({ kind: "wait_elapsed", index: draw.index, rollbackAcknowledged: pg ? rollbacks === draw.index : begins === 0, elapsedAt: entry.elapsedAt });
        await waitCommand(`release${draw.index}`);
        entry.releaseAt = performance.now() - startedAt;
        callback(...args);
      }, delay);
    }) as typeof setTimeout;
    console.error = (...args: unknown[]) => {
      const event = JSON.parse(String(args[0]));
      check(event.eventName === "transaction.attempt", "unexpected_event"); events.push(event);
    };
    startedAt = performance.now();
    const options = Object.freeze({ replayable: true, operationId: `many-client-${config.wave}-${config.worker}-${config.id}`, operation: "validation.native_many_client", startedAt, signal: controller.signal });
    const expire = () => {
      if (cleanup) return;
      const remaining = startedAt + 1000 - performance.now();
      if (remaining > 0) { expiryTimer = originalTimer(expire, Math.ceil(remaining)); return; }
      send({ kind: "expired", elapsed: performance.now() - startedAt });
    };
    if (config.wave === "expiry" && config.omit !== "expiry") expiryTimer = originalTimer(expire, 1000);
    try {
      terminal = await client.withOperationTime("2026-10-04T08:10:00.000Z").transaction(async (tx: any) => {
        callbacks++;
        if (pg && callbacks === 1) {
          backend = Number((await activeOwner.unsafe("SELECT pg_backend_pid() AS pid"))[0].pid);
          send({ kind: "entering_gate", backend });
          await activeOwner.unsafe("SELECT pg_advisory_xact_lock($1)", [config.gate]);
        }
        const label = `many-${config.wave}-${config.worker}`;
        const changed = await tx.update_required_Entry_by_id_set_label(config.id, label);
        const history = await query(activeOwner, pg, 'SELECT CAST(revision AS TEXT) AS revision FROM "__jadpo_changes" WHERE entity_id=$1 ORDER BY "__jadpo_changes".revision', [config.id]);
        const revision = history.at(-1)!.revision;
        const spec = { job, sourceOperation: "Entry.validation_update", sourceEntity: "Entry", sourceEntityId: config.id, sourceRevision: revision, orderingKey: config.id, payloadVersion: "1", payload: { recipient: "person@example.test", entryId: config.id, label } };
        const enrolled = config.omit === "intent" && config.worker === 2 && (!pg || callbacks === 3) ? null : await tx.enqueue_delivery_intent(spec);
        const provisional = await snapshot(activeOwner, pg, config.id); provisionals.push(provisional);
        if (pg) {
          try { await activeOwner.unsafe(`SELECT validation_many_abort_${config.worker}()`); }
          catch (error) { check((error as any).errno === "40001", "actual_server_abort"); phases.push({ phase: "server_abort", code: (error as any).errno, at: performance.now() - startedAt }); throw new module.PersistenceFault("transaction.native_many_server_abort", "driver", error); }
        }
        if (enrolled !== null) { const duplicate = await tx.enqueue_delivery_intent(spec); equal(duplicate, enrolled, "duplicate_identity"); equal(await snapshot(activeOwner, pg, config.id), provisional, "duplicate_snapshot"); }
        return { changed, enrolled, revision };
      }, options);
    } catch (error) {
      check(error instanceof module.PersistenceFault, "terminal_fault_type");
      fault = { transaction: (error as any).transaction, deadlineExceeded: (error as any).deadlineExceeded === true };
    }
    check(options.startedAt === startedAt && options.signal === controller.signal && selected === null, "immutable_options");
    send({ kind: "result", wave: config.wave, originalStartedAt: startedAt, elapsed: performance.now() - startedAt, operationId: options.operationId, originalSignalAborted: controller.signal.aborted, monotonicClockUnmodified: performance.now === originalNow, draws, waits, phases, events, attemptCalls, begins, rollbacks, commits, callbacks, backend, provisionals, terminal, fault });
  } finally {
    if (expiryTimer !== null) clearTimeout(expiryTimer);
    Math.random = originalRandom; globalThis.setTimeout = originalTimer; console.error = originalLog;
    check(Math.random === originalRandom && globalThis.setTimeout === originalTimer && activeOwner === null, "restore");
    if (pg) await original.close(); else original.close();
    send({ kind: "closed", ownerClosed: true });
    process.disconnect?.();
  }
}

export async function runNativeContentionWave(input: { module: string; pristine: string; pg: boolean; observer: any; sourceId: string; wave: Wave; gate: number; baseline: any }) {
  const { pg, observer, sourceId: id, wave, gate, baseline } = input;
  const omit = Bun.env.JADPO_CONTENTION_OMIT ?? "";
  check(["", "gate", "seeds", "expiry", "abort", "intent"].includes(omit), "omission_selector");
  const modulePath = join(dirname(input.module), "persistence-contention.ts");
  writeFileSync(modulePath, input.pristine + ownerSeam);
  const configured = (worker: number, reader = false) => JSON.stringify({ worker, module: modulePath, pg, id, wave, gate, omit, reader });
  const packets: any[][] = [[], [], [], []], children: any[] = [], stderrTasks: Promise<string>[] = [], results: any[] = [];
  let failure: unknown = null, watchdog = false, blockerPID: number | null = null, overlapping: any[] = [], blockerReleased = false;
  let releaseBlocker: (() => void) | null = null, blockerTask: Promise<any> | null = null;
  const send = (worker: number, command: string) => children[worker].send({ worker, command });
  const release = async () => {
    if (blockerReleased) return; blockerReleased = true;
    if (pg) { releaseBlocker?.(); await blockerTask; }
    else if (observer.inTransaction) observer.exec("ROLLBACK");
  };
  const timeout = setTimeout(() => { watchdog = true; for (const child of children) if (child.exitCode === null) child.kill(); }, 10000);
  const wait = async (predicate: () => boolean, code: string) => {
    const begin = performance.now();
    while (!predicate()) {
      check(!watchdog && performance.now() - begin < 5000, `qualifier_${code}`);
      const invalid = packets.flat().find(packet => packet.kind === "failure"); check(!invalid, "worker_failure");
      await Bun.sleep(2);
    }
  };
  const all = (kind: string, index?: number) => packets.every(list => list.some(p => p.kind === kind && (index === undefined || p.index === index)));
  try {
    // Sequential initial imports only; actual four-worker operations start together.
    for (let worker = 0; worker < 4; worker++) {
      const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", import.meta.path], {
        env: { ...Bun.env, JADPO_CONTENTION_CHILD: configured(worker) }, stdout: "pipe", stderr: "pipe",
        ipc(packet: any) { check(packet?.worker === worker && packet.pid === child.pid && JSON.stringify(packet).length < 65536, "bounded_worker_packet"); packets[worker].push(packet); },
      });
      children.push(child); stderrTasks.push(new Response(child.stderr).text());
      await wait(() => packets[worker].some(p => p.kind === "ready"), "ready");
    }
    if (pg) {
      for (let worker = 0; worker < 4; worker++) {
        await observer.unsafe(`CREATE SEQUENCE validation_many_count_${worker}`);
        await observer.unsafe(`CREATE FUNCTION validation_many_abort_${worker}() RETURNS void LANGUAGE plpgsql AS $$ BEGIN IF nextval('validation_many_count_${worker}') <= 2 THEN RAISE EXCEPTION 'fixture server abort' USING ERRCODE='40001'; END IF; END $$`);
      }
      if (omit !== "gate") {
        let acquired!: () => void; const ready = new Promise<void>(resolve => acquired = resolve);
        blockerTask = observer.begin(async (tx: any) => {
          blockerPID = Number((await tx.unsafe("SELECT pg_backend_pid() AS pid"))[0].pid);
          await tx.unsafe("SELECT pg_advisory_xact_lock($1)", [gate]); acquired();
          await new Promise<void>(resolve => releaseBlocker = resolve);
        });
        await ready;
      }
    } else if (omit !== "gate") observer.exec("BEGIN IMMEDIATE");
    for (let worker = 0; worker < 4; worker++) send(worker, "go");
    if (pg) {
      await wait(() => all("entering_gate"), "four_entering");
      const pids = packets.map(list => list.find(p => p.kind === "entering_gate").backend);
      check(new Set(pids).size === 4, "four_distinct_backends");
      const onset = performance.now();
      while (true) {
        overlapping = (await query(observer, true, "SELECT pid, state, wait_event_type, wait_event, pg_blocking_pids(pid) AS blockers FROM pg_stat_activity WHERE pid IN ($1,$2,$3,$4) AND wait_event_type='Lock'", pids)).map(row => ({ ...row, blockers: Array.from(row.blockers) }));
        if (overlapping.length === 4 && overlapping.every(row => row.state === "active" && row.blockers.includes(blockerPID))) break;
        if (omit === "gate") check(false, "four_native_blocked_by_parent");
        check(performance.now() - onset < 1000 && !watchdog, "four_native_blocked_by_parent");
        await Bun.sleep(2);
      }
      await release();
    }
    await wait(() => all("wait_elapsed", 1) || packets.flat().some(p => p.kind === "result"), "first_waits");
    check(all("wait_elapsed", 1), "four_native_first_failures");
    if (!pg) check(packets.every(list => list.some(p => p.kind === "failed_begin" && p.attempt === 1 && p.code === "SQLITE_BUSY" && p.opened === false)), "four_native_first_failures");
    check(packets.every(list => list.find(p => p.kind === "wait_elapsed" && p.index === 1).rollbackAcknowledged === true), "native_rollback_before_wait");
    equal(await snapshot(observer, pg, id), baseline, "rollback_baseline_first_wait");
    const stopped = wave !== "success" && !(wave === "expiry" && omit === "expiry") && !(wave === "cancel" && omit === "abort");
    if (stopped) {
      await release();
      if (wave === "expiry") await wait(() => all("expired"), "original_window_expiry");
      else { for (let worker = 0; worker < 4; worker++) send(worker, "abort"); await wait(() => all("aborted"), "captured_signal_abort"); }
      for (let worker = 0; worker < 4; worker++) send(worker, "release1");
    } else {
      for (let worker = 0; worker < 4; worker++) send(worker, "release1");
      await wait(() => all("wait_elapsed", 2) || packets.flat().some(p => p.kind === "result"), "second_waits");
      check(all("wait_elapsed", 2), "four_second_failures");
      equal(await snapshot(observer, pg, id), baseline, "rollback_baseline_second_wait");
      await release();
      for (let worker = 0; worker < 4; worker++) {
        send(worker, "release2");
        await wait(() => packets[worker].some(p => p.kind === "result"), "terminal_turn");
      }
    }
    await wait(() => all("result") && all("closed"), "natural_owner_close");
    for (let worker = 0; worker < 4; worker++) {
      const child = children[worker]; check(await child.exited === 0, "natural_child_exit");
      check(await stderrTasks[worker] === "", "safe_child_stderr"); check(await new Response(child.stdout).text() === "", "closed_ipc_not_stdout_authority");
      results.push(packets[worker].find(p => p.kind === "result"));
    }
    const closedBackends = pg ? Array.from(new Set([...results.map(r => r.backend), ...packets.map(list => list.find(p => p.kind === "ready").initialBackend)])) : [];
    if (pg) {
      check(closedBackends.every(pid => Number.isInteger(pid) && pid > 0) && closedBackends.length <= 8, "observed_native_backends");
      const retained = await query(observer, true, `SELECT pid FROM pg_stat_activity WHERE pid IN (${closedBackends.map((_, index) => `$${index + 1}`).join(",")})`, closedBackends); equal(retained, [], "closed_worker_backends");
    }
    // Separate process observes durable native state after all four owners close.
    const reader = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", import.meta.path], { env: { ...Bun.env, JADPO_CONTENTION_CHILD: configured(0, true) }, stdout: "pipe", stderr: "pipe" });
    const [readerCode, readerText, readerError] = await Promise.all([reader.exited, new Response(reader.stdout).text(), new Response(reader.stderr).text()]);
    check(readerCode === 0 && readerError === "" && readerText.length < 65536, "fresh_reader_exit");
    const fresh = JSON.parse(readerText); check(fresh.ownerClosed === true && !children.some(child => child.pid === fresh.pid), "fresh_reader_original_owner");
    const durable = await snapshot(observer, pg, id); equal(fresh.state, durable, "fresh_process_durable");
    if (pg) equal(await query(observer, true, "SELECT pid FROM pg_stat_activity WHERE pid=$1", [fresh.initialBackend]), [], "closed_fresh_reader_backend");
    const report: any = { database: pg ? "postgres" : "sqlite", wave, omit, watchdog, sourceId: id, workerPIDs: children.map(child => child.pid), blockerPID, overlapping, baseline, results, durable, freshReaderPID: fresh.pid, freshReaderBackend: fresh.initialBackend, observedClosedBackends: closedBackends, nativeOwnersClosed: true, claim: "private_component_native_contention_real_elapsed_not_entropy_fairness_or_public_create" };
    const capture = Bun.env.JADPO_CONTENTION_CAPTURE;
    if (capture) { mkdirSync(capture, { recursive: true }); writeFileSync(join(capture, `${pg ? "postgres" : "sqlite"}-${wave}-${omit || "positive"}.json`), JSON.stringify(report, null, 2) + "\n"); writeFileSync(join(capture, "pristine-persistence.ts"), input.pristine); writeFileSync(join(capture, "executed-persistence-contention.ts"), input.pristine + ownerSeam); }
    check(!watchdog, "watchdog_not_product_evidence");
    // Omitted expiry/abort must fail on actual durable effects, after the
    // equivalent finite terminal protocol really commits, not an early wait.
    if (wave !== "success") equal(durable, baseline, "stopped_full_baseline");
    for (let worker = 0; worker < 4; worker++) {
      const r = results[worker]; const success = wave === "success";
      check(r.monotonicClockUnmodified === true, "original_native_monotonic_clock");
      equal(r.draws, references[worker].slice(0, success ? 2 : 1), "independent_reference_draws");
      check(r.waits.every((w: any, index: number) => w.fraction === references[worker][index] / 4294967296 && w.delay === w.fraction * (index === 0 ? 50 : 100) && w.releaseAt >= w.elapsedAt && w.elapsedAt >= w.timerAt), "actual_timer_budget_consumption");
      equal(r.events.map((e: any) => [e.operationId, e.attempt, e.outcome, e.selectedDelayMs, e.retryScheduled, e.cancelled]), success ? r.waits.map((w: any, i: number) => [r.operationId, i + 1, "no_commit", Math.round(w.delay), true, false]).concat([[r.operationId, 3, "committed", null, false, false]]) : [[r.operationId, 1, "no_commit", Math.round(r.waits[0].delay), false, wave === "cancel"]], "bounded_original_attempt_events");
      equal([r.attemptCalls, r.begins, r.rollbacks, r.commits, r.callbacks], success ? (pg ? [3,3,2,1,3] : [3,1,0,1,1]) : (pg ? [1,1,1,0,1] : [1,0,0,0,0]), "bounded_native_attempts_no_amplification");
      if (pg) {
        equal(r.phases.map((p: any) => p.phase), success ? ["begin_delegate","server_abort","rollback_ack","begin_delegate","server_abort","rollback_ack","begin_delegate","commit_ack"] : ["begin_delegate","server_abort","rollback_ack"], "actual_server_abort_and_native_rollback_order");
        check(r.phases.filter((p: any) => p.phase === "server_abort").every((p: any) => p.code === "40001"), "actual_server_abort_codes");
        check(r.provisionals.length === (success ? 3 : 1), "native_provisional_attempts");
        for (const state of r.provisionals.slice(0, success ? 2 : 1)) {
          equal(state.changes.slice(0, -1), baseline.changes, "native_provisional_preserves_baseline_history");
          check(state.source.length === 1 && state.source[0].label === `many-${wave}-${worker}` && state.changes.length === 2, "native_provisional_source_revision");
          equal(JSON.parse(state.changes.at(-1).payload), state.source[0], "native_provisional_change_payload");
          const provisionalIntent = state.intents.find((row: any) => row.job === job);
          check(state.intents.length === 2 && provisionalIntent.source_entity_id === id && provisionalIntent.source_revision === state.changes.at(-1).revision && provisionalIntent.state === "pending", "native_full_provisional_before_server_abort");
          equal(JSON.parse(provisionalIntent.payload), { recipient: "person@example.test", entryId: id, label: state.source[0].label }, "native_provisional_intent_payload");
        }
      } else check(r.phases.filter((p: any) => p.phase === "failed_begin").every((p: any) => p.code === "SQLITE_BUSY" && p.opened === false), "actual_native_busy_no_open_tx");
      check(r.originalSignalAborted === (wave === "cancel"), "original_signal_state");
      if (success) check(r.terminal !== null && r.fault === null && r.elapsed < 1000, "native_success_original_budget");
      else check(r.terminal === null && r.fault.transaction.outcome === "no_commit" && (wave === "cancel" || r.elapsed >= 1000), "stopped_original_budget");
    }
    equal(durable.intents.filter((r: any) => r.job === baselineJob), baseline.intents, "baseline_intent_preserved");
    equal(durable.keys.filter((r: any) => r.job === baselineJob), baseline.keys, "baseline_key_preserved");
    if (wave === "success") {
      equal(durable.changes.map((r: any) => r.revision), ["1","2","3","4","5"], "contiguous_native_history");
      equal(durable.changes[0], baseline.changes[0], "baseline_history_preserved");
      const intents = durable.intents.filter((r: any) => r.job === job);
      check(intents.length === 4, "four_durable_intents"); equal(intents.map((r: any) => r.key_sequence), ["1","2","3","4"], "contiguous_native_order");
      equal(durable.keys.filter((r: any) => r.job === job), [{ job, ordering_key: id, next_sequence: "4" }], "native_allocator_last_allocated");
      for (let worker = 0; worker < 4; worker++) {
        const r = results[worker]; const change = durable.changes.find((c: any) => c.revision === r.terminal.revision); const intent = intents.find((i: any) => i.intent_id === r.terminal.enrolled?.intentId);
        check(intent?.state === "pending" && intent.source_entity_id === id && intent.source_entity === "Entry" && intent.source_revision === change.revision && intent.ordering_key === id, "native_intent_source_binding");
        equal(JSON.parse(change.payload), r.terminal.changed, "native_change_payload"); equal(JSON.parse(intent.payload), { recipient: "person@example.test", entryId: id, label: r.terminal.changed.label }, "native_intent_payload");
      }
      equal(durable.source, [JSON.parse(durable.changes.at(-1).payload)], "native_final_source_revision");
    } else equal(durable, baseline, "stopped_full_baseline");
    return report;
  } catch (error) { failure = error; throw error; }
  finally {
    clearTimeout(timeout); await release();
    for (let worker = 0; worker < children.length; worker++) if (children[worker].exitCode === null) { try { send(worker, "cleanup"); } catch {} }
    let cleanupForcedKill = false;
    const cleanupTimer = setTimeout(() => { for (const child of children) if (child.exitCode === null) { cleanupForcedKill = true; child.kill(); } }, 2000);
    const exits = await Promise.allSettled(children.map(child => child.exited)); clearTimeout(cleanupTimer);
    await Promise.allSettled(stderrTasks);
    if (pg) for (let worker = 0; worker < 4; worker++) { await observer.unsafe(`DROP FUNCTION IF EXISTS validation_many_abort_${worker}()`); await observer.unsafe(`DROP SEQUENCE IF EXISTS validation_many_count_${worker}`); }
    if (failure && Bun.env.JADPO_CONTENTION_CAPTURE) {
      mkdirSync(Bun.env.JADPO_CONTENTION_CAPTURE, { recursive: true }); writeFileSync(join(Bun.env.JADPO_CONTENTION_CAPTURE, `${pg ? "postgres" : "sqlite"}-${wave}-${omit || "positive"}-failure.json`), JSON.stringify({ wave, omit, watchdog, code: (failure as Error).message.startsWith("contention.") ? (failure as Error).message : "unexpected_fixture_failure", packets, workerExitCodes: exits.map(exit => exit.status === "fulfilled" ? exit.value : "exit_wait_failed"), blockerReleased, cleanupForcedKill }, null, 2) + "\n");
    }
    check(!cleanupForcedKill, "failure_cleanup_forced_kill");
  }
}

export async function nativeContentionSnapshot(owner: any, pg: boolean, id: string) { return snapshot(owner, pg, id); }
if (import.meta.main) {
  try { await childMain(); }
  catch (error) { const code = error instanceof Error && error.message.startsWith("contention.") ? error.message : "contention.native_worker_failed"; process.send?.({ kind: "failure", worker: JSON.parse(Bun.env.JADPO_CONTENTION_CHILD!).worker, pid: process.pid, code }); process.disconnect?.(); process.exitCode = 1; }
}
