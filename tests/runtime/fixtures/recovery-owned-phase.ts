// Actual post-start native recovery phase observation. No generated state/evaluator replacement.
import { SQL } from "bun";
import { createConnection, createServer, type AddressInfo, type Socket } from "node:net";
import { appendFileSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const phase = Bun.argv[2];
if (!["begin", "setup"].includes(phase)) throw Error("probe.invalid_phase");
const mode = Bun.argv[3] ?? "hold";
if (!["hold", "healthy", "omit-hold", "preexecute", "keep-hold"].includes(mode)) throw Error("probe.invalid_mode");
const supplied = Bun.env.JADPO_READINESS_TEST_DATABASE_URL, app = Bun.env.JADPO_READINESS_APPLICATION;
if (!supplied || !app) throw Error("probe.native_input_required");
const base = new URL(supplied);
if (!["127.0.0.1", "localhost"].includes(base.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(base.pathname)) throw Error("probe.disposable_database_required");
const database = `jadpo_auth_test_probe_${crypto.randomUUID().replaceAll("-", "")}`;
const admin = new SQL(supplied, { max: 1, prepare: false });
await admin.unsafe(`CREATE DATABASE "${database}"`);
const direct = new URL(supplied); direct.pathname = `/${database}`;
const control = new SQL(direct.href, { max: 1, prepare: false });
const directory = mkdtempSync(join(tmpdir(), "jadpo-native-recovery-phase-"));
const pidPath = join(directory, "pids"), listenerPath = join(directory, "listeners"), preload = join(directory, "observe.ts");
writeFileSync(preload, `import {appendFileSync} from "node:fs";import {Server} from "node:net";appendFileSync(${JSON.stringify(pidPath)},JSON.stringify({pid:process.pid,child:typeof process.send==="function"})+"\\n");const spawn=Bun.spawn;Bun.spawn=((a:any,o:any)=>spawn(Array.isArray(a)?[...a.slice(0,-1),"--preload",${JSON.stringify(preload)},a.at(-1)]:a,o))as typeof Bun.spawn;const serve=Bun.serve;Bun.serve=((...a:any[])=>{appendFileSync(${JSON.stringify(listenerPath)},"Bun.serve\\n");return serve(...a);})as typeof Bun.serve;const listen=Server.prototype.listen;Server.prototype.listen=function(...a:any[]){appendFileSync(${JSON.stringify(listenerPath)},"Node.listen\\n");return listen.apply(this,a as any);};`);
const check = (value: unknown, code: string) => { if (!value) throw Error(`probe.${code}`); };
const sockets: Socket[] = [], owners = new Set<number>(), executionTrace: any[] = [], cancellations: any[] = [], parsedStatements:any[]=[];let parseAcknowledgements=0;
let connections = 0, downstreamClosed = 0, upstreamClosed = 0, protocolFailure: string | null = null;
let invalidating=false; const nativeInvalidations:any[]=[];
let armed = false, selected: any = null, delivered = false, selectedOpen = false;
let selectedForwarded = 0, selectedSuppressed = 0, qualified = false, selectedRows = 0, selectedCommand = false, selectedInTransaction = false;
let selectedError: string | null = null, heldBytes = 0, releasedAt: number | null = null;
const held: Array<{ socket: Socket; frame: Buffer }> = [];
let qualify!: () => void, intercept!: () => void;
const qualification = new Promise<void>(resolve => qualify = resolve), intercepted = new Promise<void>(resolve => intercept = resolve);
function string(frame: Buffer, offset: number): [string, number] { const end = frame.indexOf(0, offset); check(end >= offset, "protocol_string"); return [frame.subarray(offset, end).toString(), end + 1]; }
function kind(sql: string) {
  if (/^\s*SELECT\s+1\s*;?\s*$/iu.test(sql)) return "ping";
  if (/^\s*BEGIN\b/iu.test(sql)) return "begin";
  if (/^\s*COMMIT\b/iu.test(sql)) return "commit";
  if (/^\s*ROLLBACK\b/iu.test(sql)) return "rollback";
  if (sql.includes("set_config('statement_timeout'")) return "setup";
  if (/^\s*(CREATE|ALTER|DROP)\b/iu.test(sql)) return "schema";
  if (sql.includes("pg_catalog.pg_attribute")) return "catalog";
  return "other";
}
function rowIsOne(frame: Buffer) {
  if (frame.readInt16BE(5) !== 1) return false;
  const size = frame.readInt32BE(7); if (11 + size !== frame.length) return false;
  return size === 1 && frame[11] === 49 || size === 4 && frame.readInt32BE(11) === 1;
}
function nativeTimeouts(frame: Buffer) {
  check(frame.readInt16BE(5) === 2, "timeout_column_count"); let offset = 7; const values: number[] = [];
  for (let i = 0; i < 2; i++) { const size = frame.readInt32BE(offset); offset += 4; check(size > 0 && size <= 32 && offset + size <= frame.length, "timeout_value_bound"); const value = frame.subarray(offset, offset + size).toString(); offset += size; const match = /^([0-9]+)(ms|s)?$/u.exec(value); check(match !== null, "native_timeout_unit"); const number = Number(match![1]) * (match![2] === "s" ? 1000 : 1); check(Number.isSafeInteger(number) && number > 0 && number <= 10000, "native_timeout_profile"); values.push(number); }
  check(offset === frame.length && values[0] === values[1], "native_timeout_row"); return values;
}
function errorCode(frame: Buffer) { let offset = 5; while (offset < frame.length && frame[offset] !== 0) { const field = String.fromCharCode(frame[offset++]); const [value, next] = string(frame, offset); offset = next; if (field === "C") return /^[0-9A-Z]{5}$/u.test(value) ? value : "invalid"; } return null; }
const proxy = createServer(downstream => {
  const index = ++connections, upstream = createConnection({ host: direct.hostname, port: Number(direct.port || 5432) }); sockets.push(downstream, upstream); check(connections <= 64, "connection_observation_bound");
  let front = Buffer.alloc(0), back = Buffer.alloc(0), started = false, sslResponse = false, pid: number | null = null, stalled = false;
  let selectedConnection = false, lastCompleted: any = null;
  const statements = new Map<string, string>(), portals = new Map<string, string>(), executions: any[] = [];
  const fail = () => { protocolFailure = "bounded_native_protocol_refused"; downstream.destroy(); upstream.destroy(); };
  downstream.on("data", chunk => { try {
    front = Buffer.concat([front, chunk]); check(front.length <= 1048576, "front_bound");
    if (!started) {
      if (front.length < 4) return; const size = front.readUInt32BE(0); check(size >= 8 && size <= 65536, "startup_bound"); if (front.length < size) return;
      const packet = front.subarray(0, size); front = front.subarray(size); const code = packet.readUInt32BE(4);
      if (code === 80877103) { check(size === 8, "ssl_frame"); sslResponse = true; upstream.write(packet); return; }
      if (code === 80877102) { check(size === 16, "cancel_frame"); cancellations.push({ connection: index, targetPID: packet.readInt32BE(8), at: performance.now(), selectedPID: selected?.pid ?? null }); }
      else check(code === 196608, "startup_version");
      upstream.write(packet); started = true;
    }
    while (front.length >= 5) {
      const size = front.readUInt32BE(1) + 1; check(size >= 5 && size <= 1048576, "front_frame"); if (front.length < size) return;
      const frame = front.subarray(0, size); front = front.subarray(size); const tag = String.fromCharCode(frame[0]); let sqlKind: string | null = null;
      if (tag === "P") { const [name, offset] = string(frame, 5); const [sql] = string(frame, offset); statements.set(name, kind(sql));parsedStatements.push({connection:index,pid,name,kind:kind(sql),at:performance.now()}); check(statements.size <= 1024, "statement_map_bound"); }
      if (tag === "B") { const [portal, offset] = string(frame, 5); const [statement] = string(frame, offset); check(statements.has(statement), "bind_mapping"); portals.set(portal, statements.get(statement)!); check(portals.size <= 1024, "portal_map_bound"); }
      if (tag === "E") { const [portal] = string(frame, 5); check(portals.has(portal), "execute_mapping"); sqlKind = portals.get(portal)!; }
      if (tag === "Q") sqlKind = kind(string(frame, 5)[0]);
      if (sqlKind !== null) {
        const target = armed && selected === null && sqlKind === phase;
        if (invalidating && sqlKind === "ping") { invalidating=false; check(pid !== null,"invalidation_native_owner"); const owner=pid; stalled=true; nativeInvalidations.push({pid:owner,frontendTag:tag,at:performance.now(),queryForwarded:false,terminationAcknowledged:false}); void control.unsafe("SELECT pg_terminate_backend($1) AS terminated",[owner]).then(rows=>{check(rows.length===1&&rows[0].terminated===true,"invalidation_actual_termination");nativeInvalidations[0].terminationAcknowledged=true;},()=>{protocolFailure="invalidation_native_termination_failed";downstream.destroy();upstream.destroy();}); executions.push({kind:"ping",pid:owner,nativeCommandAcknowledged:false}); continue; }
        const execution = { index: executionTrace.length + 1, connection: index, pid, kind: sqlKind, frontendTag: tag, target, forwarded: true, at: performance.now(), dataRows: 0, nativeCommandAcknowledged: false, commandCompletedAt: null as number | null, readyStatus: null as string | null, readyAt: null as number | null, timeoutMilliseconds: null as number[] | null };
        if (target) {
          selected = execution; selectedConnection = true; selectedOpen = true;
          if (mode === "preexecute") { execution.forwarded = false; selectedSuppressed++; stalled = true; executionTrace.push(execution); intercept(); continue; }
          selectedForwarded++;
        }
        executionTrace.push(execution); executions.push(execution); check(executionTrace.length <= 2048 && executions.length <= 1024, "execution_map_bound");
      }
      if (!stalled || tag === "X") upstream.write(frame);
    }
  } catch { fail(); } });
  upstream.on("data", chunk => { try {
    back = Buffer.concat([back, chunk]); check(back.length <= 1048576, "back_bound");
    if (sslResponse) { if (!back.length) return; check(back[0] === 78, "encrypted_profile_unobserved"); downstream.write(back.subarray(0, 1)); back = back.subarray(1); sslResponse = false; }
    while (back.length >= 5) {
      const size = back.readUInt32BE(1) + 1; check(size >= 5 && size <= 1048576, "back_frame"); if (back.length < size) return;
      const frame = back.subarray(0, size); back = back.subarray(size); const tag = String.fromCharCode(frame[0]), current = executions[0];
      if(tag === "1")parseAcknowledgements++;
      if (tag === "K") { check(size === 13, "backend_key_shape"); pid = frame.readInt32BE(5); check(pid > 0, "backend_pid"); owners.add(pid); }
      if (tag === "D" && current) { current.dataRows++; if (current.kind === "setup") current.timeoutMilliseconds = nativeTimeouts(frame); if (current.target) { check(phase === "setup" && current.timeoutMilliseconds?.length === 2, "actual_setup_row"); selectedRows++; } }
      if (tag === "C") { check(current !== undefined, "command_mapping"); const command = string(frame, 5)[0]; current.nativeCommandAcknowledged = current.kind === "begin" ? command === "BEGIN" : current.kind === "commit" ? command === "COMMIT" : current.kind === "rollback" ? command === "ROLLBACK" : current.kind === "ping" || current.kind === "setup" ? command === "SELECT 1" : true; current.commandCompletedAt = performance.now(); if (current.target) selectedCommand = command === (phase === "begin" ? "BEGIN" : "SELECT 1"); lastCompleted = executions.shift(); }
      if (tag === "E" && current) { if (current.target) selectedError = errorCode(frame); lastCompleted = executions.shift(); }
      if (tag === "Z" && lastCompleted) { lastCompleted.readyStatus = String.fromCharCode(frame[5]); lastCompleted.readyAt = performance.now(); if (lastCompleted.target) { selectedInTransaction = frame[5] === 84; if (selectedCommand && selectedRows === (phase === "begin" ? 0 : 1) && selectedInTransaction && selected?.pid !== null) { qualified = true; qualify(); } } }
      if (selectedConnection && !delivered && (mode === "hold" || mode === "keep-hold")) { heldBytes += frame.length; check(heldBytes <= 1048576, "held_bound"); held.push({ socket: downstream, frame: Buffer.from(frame) }); }
      else { downstream.write(frame); if (tag === "Z" && lastCompleted?.target) delivered = true; }
    }
  } catch { fail(); } });
  downstream.on("close", () => { downstreamClosed++; if (selectedConnection) selectedOpen = false; upstream.end(); });
  upstream.on("close", () => { upstreamClosed++; downstream.end(); });
  downstream.on("error", () => upstream.end()); upstream.on("error", () => downstream.end());
});
await new Promise<void>(resolve => proxy.listen(0, "127.0.0.1", resolve));
const proxied = new URL(direct.href); proxied.port = String((proxy.address() as AddressInfo).port);
function reserve() { const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() }); const port = server.port; server.stop(true); return port; }
const applicationPort = reserve(); let deploymentPort = reserve(); while (deploymentPort === applicationPort) deploymentPort = reserve();
async function boundedText(stream: ReadableStream<Uint8Array>) { const reader = stream.getReader(); let text = ""; const decoder = new TextDecoder(); try { while (true) { const p = await reader.read(); if (p.done) break; check(text.length + p.value.byteLength <= 65536, "native_output_bound"); text += decoder.decode(p.value, { stream: true }); } return text; } finally { reader.releaseLock(); } }
const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "--preload", preload, app], { env: { PATH: Bun.env.PATH ?? "", DATABASE_URL: proxied.href, PORT: String(applicationPort), JADPO_DEPLOYMENT_PORT: String(deploymentPort) }, stdout: "pipe", stderr: "pipe" });
const stdout = boundedText(child.stdout), stderr = boundedText(child.stderr);
let watchdog = false, failure: string | null = null, proof: any = null;
const watchdogTimer = setTimeout(() => { watchdog = true; child.kill("SIGKILL"); }, 15000);
const observations: any[] = [], pending: Promise<any>[] = [];
async function request(port: number, path: string) { const start = performance.now(); const response = await fetch(`http://127.0.0.1:${port}${path}`, { signal: AbortSignal.timeout(4000) }); const body = await response.json(); const safe = JSON.stringify(body); check(!safe.includes(database) && !safe.includes(proxied.href) && !safe.includes("set_config") && !safe.includes("SELECT") && !safe.includes("stack"), "public_database_leak"); const requestID = response.headers.get("x-request-id"); check(/^req_[0-9a-f-]{36}$/u.test(requestID ?? ""), "native_request_id"); if (port === deploymentPort) { check(Object.keys(body).sort().join(",") === "advisories,checks,status", "closed_readiness_body"); check(Object.keys(body.checks).join(",") === "database" && ["available", "unavailable"].includes(body.checks.database) && ["ready", "not_ready"].includes(body.status), "closed_readiness_fields"); } const completedAt = performance.now(); return { status: response.status, body, requestID, elapsedMs: completedAt - start, completedAt }; }
async function ready() { return request(deploymentPort, "/health/ready"); }
async function live() { return request(applicationPort, "/health/live"); }
async function until(predicate: () => boolean, milliseconds: number, code: string) { const stop = performance.now() + milliseconds; while (!predicate()) { check(!watchdog && protocolFailure === null && child.exitCode === null && performance.now() < stop, code); await Bun.sleep(5); } }
async function remaining() { const ids = [...owners]; return ids.length ? Number((await control.unsafe(`SELECT count(*) AS count FROM pg_stat_activity WHERE pid IN (${ids.map((_, i) => "$" + (i + 1)).join(",")})`, ids))[0].count) : 0; }
async function durableState() {
 const objects=await control.unsafe("SELECT c.relname,c.relkind FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' ORDER BY c.relname,c.relkind");
 const tables=objects.filter(row=>row.relkind==="r"),counts:any[]=[];
 for(const row of tables){check(/^[a-z_][a-z0-9_]*$/u.test(row.relname),"fixture_table_identity");const count=await control.unsafe(`SELECT count(*)::integer AS count FROM "${row.relname}"`);counts.push({table:row.relname,count:count[0].count});}
 return {objects,counts};
}
let originalDurable:any=null;
function release() { delivered = true; releasedAt = performance.now(); for (const { socket, frame } of held) { check(!socket.destroyed, "late_result_transport_closed"); socket.write(frame); } held.length = 0; }
try {
  let baseline: any = null; const stop = performance.now() + 5000;
  while (performance.now() < stop) { try { const r = await ready(); if (r.status === 200) { baseline = r; break; } } catch {} await Bun.sleep(20); }
  check(baseline?.status === 200 && (await live()).status === 200, "healthy_main_baseline");
  check(executionTrace.some(r => r.kind === "ping" && r.forwarded), "baseline_native_ping");
  originalDurable=await durableState(); check(originalDurable.counts.every((row:any)=>row.count===0),"pristine_native_fixture"); observations.push({ durable:originalDurable, phase: "healthy_baseline", readiness: baseline, live: await live(), baselineNativePings: executionTrace.filter(r => r.kind === "ping").length });
  await Bun.sleep(1050); invalidating=true; const invalidated=await ready(); check(invalidated.status===503 && nativeInvalidations.length===1 && nativeInvalidations[0].terminationAcknowledged,"actual_dependency_invalidation"); check((await live()).status===200,"invalidation_keeps_liveness"); observations.push({phase:"genuine_native_dependency_failure",response:invalidated,nativeInvalidations}); await Bun.sleep(350); armed = true; const began = performance.now();
  const requests = Array.from({ length: 32 }, () => ready()); pending.push(...requests);for(const request of requests)void request.catch(()=>{});
  if (mode === "preexecute") { await Promise.race([intercepted, child.exited.then(() => { throw Error("probe.preexecute_absent"); })]); await Bun.sleep(50); check(!qualified && selectedForwarded === 0 && selectedSuppressed === 1 && selectedRows === 0 && !selectedCommand && !selectedInTransaction, "preexecute_unqualified"); observations.push({ phase: "genuine_execute_not_forwarded", selected }); throw Error("probe.selected_native_result_unqualified"); }
  await Promise.race([qualification, child.exited.then(() => { throw Error("probe.result_unqualified"); })]);
  check(qualified && selectedForwarded === 1 && protocolFailure === null, "native_executed_result");
  const activity = (await control`SELECT state, query FROM pg_stat_activity WHERE pid=${selected.pid}`)[0];
  observations.push({ phase: "native_completed_result", selected, actualSelectedDataRows: selectedRows, actualPhaseCommand: selectedCommand, actualReadyInTransaction: selectedInTransaction, backendState: activity?.state, backendQueryClass: activity ? kind(activity.query) : null, selectedSocketOpen: selectedOpen });
  if (mode === "hold" || mode === "keep-hold") check(activity?.state === "idle in transaction" && kind(activity.query) === phase, "native_completed_transaction_owner");
  const responses = await Promise.all(requests); observations.push({ phase: "first_http_joiners", elapsedMs: performance.now() - began, responses, live: await live(), cancellations: [...cancellations], selectedSocketOpen: selectedOpen });
  if (mode === "healthy") { check(responses.every(r => r.status === 200), "unarmed_acknowledged_healthy"); }
  else {
    check(responses.every(r => r.status === 503), "held_original_response_503"); check(performance.now() - began >= 900, "original_real_response_budget");
    // Exclude mere backoff masking: later joiners are sent after its original250ms cutoff.
    await Bun.sleep(350);
    const joined = Array.from({ length: 128 }, () => ready()); pending.push(...joined);for(const request of joined)void request.catch(()=>{}); const joinedResponses = await Promise.all(joined);
    const after = executionTrace.filter(r => r.index > selected.index && r.forwarded);
    observations.push({ phase: "later_http_joiners_after_backoff", responses: joinedResponses, live: await live(), executionsAfterSelected: after, selectedSocketOpen: selectedOpen, actualCancelRequests: cancellations, heldResultDelivered: delivered });
    check(JSON.stringify(await durableState())===JSON.stringify(originalDurable),"held_durable_state_conserved"); check(joinedResponses.every(r => r.status === 503), "joined_response_503"); check(after.length === 0, "native_slot_retained_after_original_expiry"); check(selectedOpen && !delivered, "held_native_transport_owned");
    if (mode === "keep-hold") { await Bun.sleep(1500); observations.push({ phase: "manual_shutdown_with_result_still_held", stillHeld: !delivered, nativeTransportOpen: selectedOpen, live: await live(), lifetimeClaim: "response_bounded_native_ack_still_owned_no_automatic_preemption" }); }
    else {
      release(); await until(()=>executionTrace.some(r=>r.index>selected.index&&r.kind==="rollback"&&r.nativeCommandAcknowledged&&r.readyStatus==="I"),1000,"old_native_rollback_acknowledged"); const settled=executionTrace.find(r=>r.index>selected.index&&r.kind==="rollback"&&r.nativeCommandAcknowledged&&r.readyStatus==="I"); observations.push({phase:"original_recovery_native_rollback_ack",settled,durable:await durableState()});check(JSON.stringify(await durableState())===JSON.stringify(originalDurable),"rollback_durable_state_conserved"); const recoveryStart = executionTrace.length;
      let recovered=await ready();const settlementResponses=[recovered];const freshLimit=performance.now()+1500;while(recovered.status!==200&&performance.now()<freshLimit){await Bun.sleep(10);recovered=await ready();settlementResponses.push(recovered);} const recovery = executionTrace.slice(recoveryStart);
      observations.push({ phase: "fresh_recovery", releasedAt, readiness: recovered, settlementResponses, live: await live(), actualNativeRecovery: recovery });
      check(JSON.stringify(await durableState())===JSON.stringify(originalDurable),"fresh_durable_state_conserved"); const begin = recovery.filter(r => r.kind === "begin"), commit = recovery.filter(r => r.kind === "commit"), setup = recovery.filter(r => r.kind === "setup");
      check(recovered.status === 200 && begin.length === 1 && commit.length === 1, "fresh_initializer_not_cached_late_publication");
      check(begin[0].nativeCommandAcknowledged && begin[0].readyStatus === "T" && commit[0].nativeCommandAcknowledged && commit[0].readyStatus === "I" && commit[0].readyAt <= recovered.completedAt && begin[0].pid === commit[0].pid && setup.length > 0 && setup.every((r, i) => r.pid === begin[0].pid && r.nativeCommandAcknowledged && r.readyStatus === "T" && r.timeoutMilliseconds?.every((n: number) => n <= 1000) && (i === 0 || r.timeoutMilliseconds[0] <= setup[i - 1].timeoutMilliseconds[0])), "fresh_actual_commit_ack_and_original_setup");
    }
  }
} catch (error) { failure = error instanceof Error && error.message.startsWith("probe.") ? error.message : "probe.unexpected_fixture_failure"; }
finally {
  if (child.exitCode === null) child.kill("SIGTERM"); const exit = await child.exited;
  await Promise.allSettled(pending);
  for (let i = 0; i < 100 && (downstreamClosed !== connections || upstreamClosed !== connections || await remaining() !== 0); i++) await Bun.sleep(10);
  const backendRemaining = await remaining(); const nativeProcesses = existsSync(pidPath) ? readFileSync(pidPath, "utf8").trim().split("\n").filter(Boolean).map(line => JSON.parse(line)) : [];
  const alive = (pid: number) => { try { process.kill(pid, 0); return true; } catch { return false; } };
  const text = await stdout, errors = await stderr; const events = (text + errors).split("\n").filter(Boolean).map(line => JSON.parse(line));
  const formerPorts = []; for (const port of [applicationPort, deploymentPort]) { let refused = false; try { await fetch(`http://127.0.0.1:${port}/health/live`, { signal: AbortSignal.timeout(300) }); } catch { refused = true; } formerPorts.push({ port, refused }); }
  let finalDurable:any=null;try{finalDurable=await durableState();if(originalDurable&&JSON.stringify(finalDurable)!==JSON.stringify(originalDurable))failure??="probe.cleanup_durable_state_changed";}catch{failure??="probe.cleanup_durable_snapshot_failed";}
  const clean = !watchdog && exit === 143 && connections > 0 && downstreamClosed === connections && upstreamClosed === connections && backendRemaining === 0 && nativeProcesses.length === 2 && nativeProcesses.some(p => p.child) && nativeProcesses.every(p => !alive(p.pid)) && formerPorts.every(p => p.refused);
  const safe = !(text + errors).includes(database) && !(text + errors).includes(proxied.href) && !events.some(event => event.stack || event.rawError);
  if (!clean || !safe || protocolFailure !== null) failure ??= "probe.native_normal_shutdown_or_safety_failed";
  proof = { kind: "native_recovery_initializer_phase", phase, mode, nativeInvalidations, originalDurable, finalDurable, bun: Bun.version, originalResponseBudgetMs: 1000, originalCacheMs: 1000, selected, selectedForwarded, selectedSuppressed, selectedRows, selectedCommand, selectedInTransaction, selectedError, qualifiedNativeExecution: qualified, selectedResultDelivered: delivered, selectedSocketOpen: selectedOpen, heldBytes, executionTrace, parsedStatements, parseAcknowledgements, cancellations, observations, failure, watchdog, nativeExit: exit, nativeProcesses, connections, downstreamClosed, upstreamClosed, backendPids: [...owners], backendRemaining, formerPorts, cleanupBeforeFixtureTeardown: true, safeNativeOutput: safe, nativeEvents: events, protocolFailure, claim: "actual_poststart_recovery_phase_response_and_transport_not_hard_native_lifetime" };
  clearTimeout(watchdogTimer); for (const socket of sockets) socket.destroy(); proxy.close(); await control.close({ timeout: 0 }); await admin.unsafe(`DROP DATABASE "${database}" WITH (FORCE)`); await admin.close({ timeout: 0 }); rmSync(directory, { recursive: true, force: true });
}
console.log(JSON.stringify(proof)); process.exit(failure ? 1 : 0);
