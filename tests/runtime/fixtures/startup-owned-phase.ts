// Test-owned native protocol observer. No compiler/runtime or driver result is replaced.
import { SQL } from "bun";
import { appendFileSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createConnection, createServer, type AddressInfo, type Socket } from "node:net";
const [phase, mode = "hold"] = Bun.argv.slice(2);
if (!["begin", "setup"].includes(phase) || !["hold", "healthy", "omit-hold", "preexecute"].includes(mode)) throw Error("Invalid fixture phase/mode");
const supplied = Bun.env.JADPO_STARTUP_OWNER_DATABASE_URL, app = Bun.env.JADPO_STARTUP_OWNER_APP;
if (!supplied || !app) throw Error("Requires owned native startup fixture");
const base = new URL(supplied);
if (!["localhost", "127.0.0.1"].includes(base.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(base.pathname)) throw Error("Requires disposable local database");
const database = `jadpo_auth_test_phase_${crypto.randomUUID().replaceAll("-", "")}`;
const admin = new SQL(supplied, { max: 1, prepare: false });
await admin.unsafe(`CREATE DATABASE "${database}"`);
const direct = new URL(supplied); direct.pathname = `/${database}`;
const control = new SQL(direct.href, { max: 1, prepare: false });
const fixture = mkdtempSync(join(tmpdir(), "jadpo-initial-phase-"));
const marker = join(fixture, "listener-starts"), pids = join(fixture, "native-pids"), preload = join(fixture, "observe-native.ts");
writeFileSync(preload, `import {appendFileSync} from "node:fs";import {Server} from "node:net";appendFileSync(${JSON.stringify(pids)},JSON.stringify({pid:process.pid,child:typeof process.send==="function"})+"\\n");const originalSpawn=Bun.spawn;Bun.spawn=((args:any,options:any)=>originalSpawn(Array.isArray(args)?[...args.slice(0,-1),"--preload",${JSON.stringify(preload)},args.at(-1)]:args,options))as typeof Bun.spawn;const serve=Bun.serve;Bun.serve=((...args:any[])=>{appendFileSync(${JSON.stringify(marker)},"Bun.serve\\n");return serve(...args);})as typeof Bun.serve;const listen=Server.prototype.listen;Server.prototype.listen=function(...args:any[]){appendFileSync(${JSON.stringify(marker)},"Node.listen\\n");return listen.apply(this,args as any);};`);
const sockets: Socket[] = [], backendOwners = new Set<number>();
let connections = 0, closedConnections = 0, closedUpstreams = 0, selectedPid: number | null = null;
let laterExecutions = 0, selectedForwarded = 0, selectedParsed = 0, selectedParseCompletions = 0, selectedSuppressed = 0;
let selectedFrontendTag: string | null = null, selectedCommand = false, selectedReadyT = false, selectedLimits: number[] = [];
let selectedDataRows = 0, holdOnsetMs: number | null = null, selectedSocketOpen = false, qualificationSettled = false;
let failReason: string | null = null, nativeStarted = 0;
let qualified!: () => void, intercepted!: () => void;
const qualifiedResult = new Promise<void>(resolve => { qualified = resolve; });
const preexecution = new Promise<void>(resolve => { intercepted = resolve; });
type Execution = { kind: string; target: boolean; command: boolean; limits: number[]; dataRows: number };
function classification(statement: string) {
  if (/^\s*BEGIN\b/iu.test(statement)) return "begin";
  if (statement.includes("set_config('statement_timeout'") && statement.includes("set_config('lock_timeout'")) return "setup";
  if (statement.includes("pg_advisory")) return "advisory";
  if (statement.includes("pg_catalog.pg_attribute")) return "catalog";
  if (/^\s*(CREATE|ALTER|DROP)\b/iu.test(statement)) return "schema";
  return "other";
}
function cstring(frame: Buffer, offset: number): [string, number] {
  const end = frame.indexOf(0, offset); if (end < 0) throw Error("Invalid protocol string");
  return [frame.subarray(offset, end).toString(), end + 1];
}
function limitMilliseconds(value: string) {
  const match = /^([0-9]+)(ms|s)?$/u.exec(value);
  if (!match) throw Error("Unexpected safe timeout result");
  const result = Number(match[1]) * (match[2] === "s" ? 1000 : 1);
  if (!Number.isSafeInteger(result) || result <= 0 || result > 10000) throw Error("Native timeout outside original profile");
  return result;
}
function limitsFromRow(frame: Buffer) {
  if (frame.readInt16BE(5) !== 2) throw Error("Unexpected native timeout column count");
  let offset = 7; const limits: number[] = [];
  for (let index = 0; index < 2; index++) {
    const size = frame.readInt32BE(offset); offset += 4;
    if (size <= 0 || size > 32 || offset + size > frame.length) throw Error("Unexpected native timeout value length");
    limits.push(limitMilliseconds(frame.subarray(offset, offset + size).toString())); offset += size;
  }
  if (offset !== frame.length || limits[0] !== limits[1]) throw Error("Unequal native timeout results");
  return limits;
}
const proxy = createServer(downstream => {
  connections++; const upstream = createConnection({ host: direct.hostname, port: Number(direct.port || 5432) }); sockets.push(downstream, upstream);
  let frontend = Buffer.alloc(0), backend = Buffer.alloc(0), started = false, connectionPid: number | null = null;
  let selectedOnConnection = false, stalledBeforeExecution = false, lastCompleted: Execution | null = null;
  const statements = new Map<string, string>(), portals = new Map<string, string>();
  const parseQueue: string[] = [], executions: Execution[] = [];
  const protocolFailure = () => { failReason = "bounded_protocol_observer_failed"; downstream.destroy(); upstream.destroy(); };
  downstream.on("data", chunk => {
    try {
      frontend = Buffer.concat([frontend, chunk]); if (frontend.length > 1048576) throw Error("Frontend bound");
      if (!started) {
        if (frontend.length < 4) return; const size = frontend.readUInt32BE(0);
        if (size < 8 || size > 65536) throw Error("Startup frame bound"); if (frontend.length < size) return;
        upstream.write(frontend.subarray(0, size)); frontend = frontend.subarray(size); started = true;
      }
      while (frontend.length >= 5) {
        const size = frontend.readUInt32BE(1) + 1; if (size < 5 || size > 1048576) throw Error("Frontend frame bound"); if (frontend.length < size) return;
        const frame = frontend.subarray(0, size); frontend = frontend.subarray(size); const tag = String.fromCharCode(frame[0]);
        let kind: string | null = null;
        if (tag === "P") { const [name, offset] = cstring(frame, 5); const [sql] = cstring(frame, offset); const parsed = classification(sql); statements.set(name, parsed); parseQueue.push(parsed); if (parsed === phase) selectedParsed++; }
        if (tag === "B") { const [portal, offset] = cstring(frame, 5); const [statement] = cstring(frame, offset); const parsed = statements.get(statement); if (parsed === undefined) throw Error("Unmapped Bind"); portals.set(portal, parsed); }
        if (tag === "E") { const [portal] = cstring(frame, 5); kind = portals.get(portal) ?? null; if (kind === null) throw Error("Unmapped Execute"); }
        if (tag === "Q") kind = classification(cstring(frame, 5)[0]);
        if (kind !== null) {
          if (selectedForwarded > 0) laterExecutions++;
          const target = kind === phase && selectedForwarded === 0 && selectedSuppressed === 0;
          if (target) {
            selectedFrontendTag = tag; selectedPid = connectionPid;
            if (mode === "preexecute") { selectedSuppressed++; stalledBeforeExecution = true; intercepted(); continue; }
            selectedForwarded++; selectedOnConnection = true; selectedSocketOpen = true; holdOnsetMs = performance.now() - nativeStarted;
          }
          executions.push({ kind, target, command: false, limits: [], dataRows: 0 });
        }
        // A pre-execution negative gates the genuine portal Execute/simple-Q;
        // pipelined Sync is not forwarded into a fabricated result cycle.
        if (!stalledBeforeExecution) upstream.write(frame);
      }
    } catch { protocolFailure(); }
  });
  upstream.on("data", chunk => {
    try {
      backend = Buffer.concat([backend, chunk]); if (backend.length > 1048576) throw Error("Backend bound");
      while (backend.length >= 5) {
        const size = backend.readUInt32BE(1) + 1; if (size < 5 || size > 1048576) throw Error("Backend frame bound"); if (backend.length < size) return;
        const frame = backend.subarray(0, size); backend = backend.subarray(size); const tag = String.fromCharCode(frame[0]);
        if (tag === "K") { connectionPid = frame.readInt32BE(5); backendOwners.add(connectionPid); }
        if (tag === "1") { const parsed = parseQueue.shift(); if (parsed === phase) selectedParseCompletions++; }
        const current = executions[0];
        if (tag === "D" && current?.target && phase === "setup") { current.limits = limitsFromRow(frame); current.dataRows++; selectedLimits = current.limits; selectedDataRows = current.dataRows; }
        if (tag === "C") {
          if (!current) throw Error("Unmapped CommandComplete"); const command = cstring(frame, 5)[0];
          current.command = current.kind === "begin" ? command === "BEGIN" : current.kind === "setup" ? command === "SELECT 1" : true;
          if (current.target) selectedCommand = current.command;
          lastCompleted = executions.shift()!;
        }
        if (tag === "Z" && lastCompleted?.target) {
          selectedReadyT = frame[5] === 84;
          if (selectedCommand && selectedReadyT && selectedPid !== null && (phase === "begin" || selectedDataRows === 1 && selectedLimits.length === 2)) { qualificationSettled = true; qualified(); }
        }
        // Observe complete real server frames but retain selected result/ready
        // on the live socket. No row/result or driver completion is invented.
        if (!(mode === "hold" && selectedOnConnection)) downstream.write(frame);
      }
    } catch { protocolFailure(); }
  });
  downstream.on("close", () => { closedConnections++; if (selectedOnConnection) selectedSocketOpen = false; upstream.end(); });
  upstream.on("close", () => { closedUpstreams++; downstream.end(); });
  downstream.on("error", () => upstream.end()); upstream.on("error", () => downstream.end());
});
await new Promise<void>(resolve => proxy.listen(0, "127.0.0.1", resolve));
const proxied = new URL(direct.href); proxied.port = String((proxy.address() as AddressInfo).port);
function reserve() { const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() }); const port = server.port; server.stop(true); return port; }
const appPort = reserve(); let deploymentPort = reserve(); while (deploymentPort === appPort) deploymentPort = reserve();
const observerPath = join(fixture, "network.ts");
writeFileSync(observerPath, `let polls=0,bindings=0,readyResponses=0;process.once("SIGTERM",()=>{console.log(JSON.stringify({polls,bindings,readyResponses}));process.exit(0)});while(true){for(const port of ${JSON.stringify([appPort, deploymentPort])}){polls++;try{const r=await fetch("http://127.0.0.1:"+port+"/health/ready",{signal:AbortSignal.timeout(100)});bindings++;if(r.status===200)readyResponses++}catch{}}await Bun.sleep(20)}`);
const observer = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", observerPath], { env: { PATH: Bun.env.PATH ?? "" }, stdout: "pipe", stderr: "pipe" });
const observerOutput = new Response(observer.stdout).text(), observerErrors = new Response(observer.stderr).text();
nativeStarted = performance.now();
const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", "--preload", preload, app], { env: { PATH: Bun.env.PATH ?? "", DATABASE_URL: proxied.href, PORT: String(appPort), JADPO_DEPLOYMENT_PORT: String(deploymentPort) }, stdout: "pipe", stderr: "pipe" });
async function boundedOutput(stream: ReadableStream<Uint8Array>) { const reader = stream.getReader(), decoder = new TextDecoder(); let text = ""; try { while (true) { const part = await reader.read(); if (part.done) break; if (text.length + part.value.byteLength > 65536) throw Error("Native output bound"); text += decoder.decode(part.value, { stream: true }); } return text; } finally { reader.releaseLock(); } }
const output = boundedOutput(child.stdout), errors = boundedOutput(child.stderr);
let expiryObserved = false;
let healthyStatuses: number[] | null = null;
const watchdog = setTimeout(() => { expiryObserved = true; child.kill("SIGKILL"); }, 15000);
async function schemaObjects() { return Number((await control`SELECT count(*) AS count FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p')`)[0].count); }
async function nativeBackendsRemaining() { const ids = [...backendOwners]; return ids.length === 0 ? 0 : Number((await control.unsafe(`SELECT count(*) AS count FROM pg_stat_activity WHERE pid IN (${ids.map((_, index) => `$${index + 1}`).join(",")})`, ids))[0].count); }
function nativeProcesses() { if (!existsSync(pids)) return []; const text = readFileSync(pids, "utf8"); if (text.length > 2048) throw Error("Native process trace bound"); return text.trim().split("\n").filter(Boolean).map(line => JSON.parse(line) as { pid: number; child: boolean }); }
function alive(pid: number) { try { process.kill(pid, 0); return true; } catch { return false; } }
try {
  if (mode === "preexecute") {
    await Promise.race([preexecution, child.exited.then(() => { throw Error("Preexecution interception absent"); })]); await Bun.sleep(50);
    console.log(JSON.stringify({ kind: "initial_phase_unqualified_preexecution", phase, selectedFrontendTag, selectedParsed, selectedParseCompletions, selectedSuppressed, selectedForwarded, selectedCommand, selectedDataRows, selectedReadyT, qualificationSettled, schemaObjects: await schemaObjects(), reason: "genuine_execute_or_Q_gated_before_server_execution" }));
    throw Error("Selected native phase was not executed; qualification correctly refused");
  }
  await Promise.race([qualifiedResult, child.exited.then(() => { throw Error("Native executed-result qualification absent"); })]);
  if (failReason !== null || selectedForwarded !== 1) throw Error("Native phase execution qualification failed");
  const activity = (await control`SELECT state, query FROM pg_stat_activity WHERE pid=${selectedPid}`)[0];
  const before = { stateIdleInTransaction: activity?.state === "idle in transaction", activityQueryClass: activity ? classification(activity.query) : null, schemaObjects: await schemaObjects(), selectedSocketOpen, topLevelStillOwned: child.exitCode === null };
  if (mode === "hold" && (!before.stateIdleInTransaction || before.activityQueryClass !== phase || before.schemaObjects !== 0 || !before.selectedSocketOpen || !before.topLevelStillOwned)) throw Error("Held native owner state failed");
  if (mode === "healthy" || mode === "omit-hold") {
    let ready = false; const end = performance.now() + 4000;
    while (performance.now() < end && !ready) { try { const [live, deployment] = await Promise.all([fetch(`http://127.0.0.1:${appPort}/health/live`, { signal: AbortSignal.timeout(200) }), fetch(`http://127.0.0.1:${deploymentPort}/health/ready`, { signal: AbortSignal.timeout(200) })]); healthyStatuses = [live.status, deployment.status]; ready = live.status === 200 && deployment.status === 200; } catch {} if (!ready) await Bun.sleep(10); }
    const requiredObjects = await control`SELECT relname FROM pg_catalog.pg_class WHERE relnamespace='public'::regnamespace AND relname IN ('account','customer') AND relkind='r'`;
    if (!ready || requiredObjects.length !== 2 || await schemaObjects() === 0) throw Error("Same-profile healthy control failed"); child.kill("SIGTERM");
  }
  const exit = await child.exited, elapsedMs = performance.now() - nativeStarted;
  for (let index = 0; index < 100 && (closedConnections !== connections || closedUpstreams !== connections || await nativeBackendsRemaining() !== 0); index++) await Bun.sleep(10);
  const allBackendsRemaining = await nativeBackendsRemaining(), schema = await schemaObjects();
  const processes = nativeProcesses(); const nativeProcessesRemaining = processes.filter(value => alive(value.pid)).length;
  const stdout = await output, stderr = await errors;
  const events = (stdout + stderr).split("\n").filter(Boolean).map(line => { if (line.length > 16384) throw Error("Native line bound"); return JSON.parse(line); });
  const startupFaults = events.filter(event => event.classification === "RUNTIME_STARTUP_FAILED");
  const safeStartupFaults = startupFaults.length;
  const closedStartupFaults = startupFaults.every(event => Object.keys(event).sort().join(",") === ["schemaVersion","kind","eventName","classification","requestId","traceId","semanticOperationId","sourceRevision","attributes"].sort().join(",") && event.schemaVersion === 1 && event.kind === "operational_log_event" && event.eventName === "operation.failed" && event.requestId === "startup" && event.traceId === null && event.semanticOperationId === "runtime:start" && typeof event.sourceRevision === "string" && event.sourceRevision.length <= 128 && event.attributes !== null && typeof event.attributes === "object" && !Array.isArray(event.attributes) && Object.keys(event.attributes).length === 0);
  if (!closedStartupFaults) throw Error("Startup diagnostic was not closed and safe");
  if ((stdout + stderr).includes(database) || (stdout + stderr).includes(proxied.href) || (stdout + stderr).includes("set_config") || events.some(event => event.stack || event.rawError)) throw Error("Unsafe native diagnostic");
  observer.kill("SIGTERM"); await observer.exited; const network = JSON.parse(await observerOutput); if (await observerErrors !== "") throw Error("Native observer failed");
  const listenerStarts = existsSync(marker) ? readFileSync(marker, "utf8").trim().split("\n").length : 0;
  const proof = { kind: "generated_initial_native_phase_owner", phase, mode, bun: Bun.version, policyMs: 10000, elapsedMs, holdOnsetMs, selectedFrontendTag, selectedParsed, selectedParseCompletions, selectedForwarded, selectedCommand, selectedReadyT, selectedDataRows, selectedLimits, qualifiedExecutedResult: qualificationSettled, laterExecutions, before, connections, closedConnections, closedUpstreams, backendOwners: backendOwners.size, backendPids: [...backendOwners], selectedBackendPid: selectedPid, allBackendsRemaining, healthyStatuses, nativeProcesses: processes, nativeProcessCount: processes.length, servingChildObserved: processes.some(value => value.child), nativeProcessesRemaining, schemaObjects: schema, network, listenerStarts, expiryObserved, terminationExit: exit, safeStartupFaults, closedStartupFaults, startupFaults, protocolFailure: failReason, publishedReady: events.some(event => event.eventName === "runtime.ready"), cleanupObservedBeforeFixtureTeardown: true };
  console.log(JSON.stringify(proof));
  if (failReason !== null || expiryObserved || connections === 0 || closedConnections !== connections || closedUpstreams !== connections || allBackendsRemaining !== 0 || nativeProcessesRemaining !== 0 || processes.length !== 2 || !proof.servingChildObserved || network.polls === 0) throw Error("Native resource ownership proof failed");
  if (mode === "healthy") { if (exit !== 143 || !proof.publishedReady || safeStartupFaults !== 0 || listenerStarts !== 2 || schema === 0) throw Error("Native healthy proof failed"); }
  else if (exit !== 1 || elapsedMs < 9500 || elapsedMs > 12000 || laterExecutions !== 0 || listenerStarts !== 0 || network.bindings !== 0 || proof.publishedReady || safeStartupFaults !== 1 || schema !== 0) throw Error("Native held result proof failed");
} finally {
  clearTimeout(watchdog);
  if (child.exitCode === null) { child.kill("SIGKILL"); await child.exited; }
  if (observer.exitCode === null) { observer.kill("SIGKILL"); await observer.exited; }
  await Promise.allSettled([output, errors, observerOutput, observerErrors]);
  for (const socket of sockets) socket.destroy(); proxy.close(); await control.close({ timeout: 0 });
  await admin.unsafe(`DROP DATABASE "${database}" WITH (FORCE)`); await admin.close({ timeout: 0 }); rmSync(fixture, { recursive: true, force: true });
}
