import { afterAll, expect, test } from "bun:test";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const root = mkdtempSync(join(tmpdir(), "jadpo-startup-owner-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const postgres = Bun.env.JADPO_STARTUP_OWNER_DATABASE_URL;
if (postgres) {
  const url = new URL(postgres);
  if (!["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Requires disposable local PostgreSQL");
}
const seed = readFileSync(resolve("examples/persistence-seed/app.jadpo"), "utf8");
function build(name: string, node = false) {
  const project = join(root, name); mkdirSync(project);
  writeFileSync(join(project, "app.jadpo"), seed + (node ? '\nroute GET /native-header { auth: none headers: { trace: Text from "X-Trace" } output: Text action: { return headers.trace } }\n' : ""));
  const result = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode) throw Error(`Startup fixture build failed: ${result.stdout}\n${result.stderr}`);
  return join(project, "build/target");
}
const generated = build("bun"), nodeGenerated = build("node", true);
function clone(name: string, source = generated) { const target = join(root, name); cpSync(source, target, { recursive: true }); for (const suffix of ["", "-wal", "-shm"]) rmSync(join(target, `native.sqlite${suffix}`), { force: true }); return target; }
function port() { const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() }); const selected = server.port; server.stop(true); return selected; }
function environment(target: string) {
  const selected: Record<string, string> = { PATH: Bun.env.PATH ?? "", SQLITE_PATH: join(target, "native.sqlite"), PORT: String(port()), JADPO_DEPLOYMENT_PORT: String(port()), JADPO_DEBUG_TARGET_STACKS: "1" };
  if (postgres) selected.DATABASE_URL = postgres;
  return selected;
}
function spawn(target: string, env = environment(target), entry = join(target, "app.ts"), extra: Record<string, any> = {}) {
  const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", entry], { env, stdout: "pipe", stderr: "pipe", ...extra });
  const state = { stdout: "", stderr: "" };
  const drain = async (stream: ReadableStream<Uint8Array>, key: "stdout" | "stderr") => { const reader = stream.getReader(), decoder = new TextDecoder(); try { while (true) { const next = await reader.read(); if (next.done) break; state[key] += decoder.decode(next.value); } } finally { reader.releaseLock(); } };
  const output = Promise.all([drain(child.stdout, "stdout"), drain(child.stderr, "stderr")]);
  return { child, state, env, output };
}
async function until(check: () => boolean, milliseconds = 5000) { const end = performance.now() + milliseconds; while (!check()) { if (performance.now() >= end) throw Error("Native startup observation timed out"); await Bun.sleep(5); } }
async function finish(instance: ReturnType<typeof spawn>, limit = 15000) {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try { return await Promise.race([instance.child.exited, new Promise<never>((_, reject) => { timeout = setTimeout(() => { instance.child.kill("SIGKILL"); reject(Error("Fixture watchdog terminated owner")); }, limit); })]); }
  finally { clearTimeout(timeout); await instance.output; }
}
function failed(state: { stdout: string; stderr: string }) {
  const events = (state.stdout + state.stderr).split("\n").filter(Boolean).map(line => JSON.parse(line));
  expect(events.filter(event => event.classification === "RUNTIME_STARTUP_FAILED")).toHaveLength(1);
  expect(events.find(event => event.classification === "RUNTIME_STARTUP_FAILED").attributes).toEqual({});
  expect(state.stdout).not.toContain("runtime.ready");
}
for (const [name, target, node] of [["Bun", generated, false], ["Node", nodeGenerated, true]] as const) {
  test(`actual ${name} native child acknowledges both listeners before ready and parent owns normal shutdown`, async () => {
    const instance = spawn(target);
    try {
      await until(() => instance.state.stdout.includes("runtime.ready") || instance.child.exitCode !== null);
      expect(instance.child.exitCode).toBeNull();
      const live = await fetch(`http://127.0.0.1:${instance.env.PORT}/health/live`); expect(live.status).toBe(200);
      const ready = await fetch(`http://127.0.0.1:${instance.env.JADPO_DEPLOYMENT_PORT}/health/ready`); expect(ready.status).toBe(200);
      expect((await fetch(`http://127.0.0.1:${instance.env.PORT}/health/ready`)).status).toBe(404);
      if (node) expect(await (await fetch(`http://127.0.0.1:${instance.env.PORT}/native-header`, { headers: { "X-Trace": "owned" } })).json()).toBe("owned");
      instance.child.kill("SIGTERM"); expect(await finish(instance)).toBe(143);
      expect(instance.state.stderr).not.toContain("RUNTIME_STARTUP_FAILED");
      for (const selected of [instance.env.PORT, instance.env.JADPO_DEPLOYMENT_PORT]) await expect(fetch(`http://127.0.0.1:${selected}/health/live`)).rejects.toThrow();
    } finally { if (instance.child.exitCode === null) { instance.child.kill("SIGKILL"); await finish(instance); } }
  }, 15000);
}
function handoffProbe(target: string, mode: string) {
  const marker = join(target, "handoff.jsonl"), preload = join(target, "handoff-preload.ts");
  // Actual native transport and listener observers. Neither source IPC packets
  // nor the generated budget are fabricated for the delayed-message controls.
  writeFileSync(preload, `import {appendFileSync,readFileSync} from "node:fs";
import {Server} from "node:http";
const marker=${JSON.stringify(marker)},mode=${JSON.stringify(mode)};
const record=(phase:string)=>appendFileSync(marker,JSON.stringify({pid:process.pid,phase})+"\\n");record("preload");
const originalSpawn=Bun.spawn;
Bun.spawn=((command:any,options:any)=>originalSpawn([...command.slice(0,-1),"--preload",${JSON.stringify(preload)},command.at(-1)],options)) as typeof Bun.spawn;
const originalServe=Bun.serve;Bun.serve=((options:any)=>{record("bun_bind");return originalServe(options)}) as typeof Bun.serve;
const originalListen=Server.prototype.listen;Server.prototype.listen=function(...args:any[]){record("node_bind");return originalListen.apply(this,args as any)} as typeof originalListen;
const originalOn=process.on;process.on=function(name:any,listener:any){return originalOn.call(this,name,name==="message"?((message:any)=>{
  if(message?.kind==="complete"&&mode==="held-complete"){
    record("complete_held");
    const sourceRevision=readFileSync(new URL("./app.ts",import.meta.url),"utf8").match(/src_[0-9a-f]{16}/)![0];
    console.log(JSON.stringify({schemaVersion:1,kind:"operational_log_event",eventName:"runtime.ready",classification:"ready",requestId:"startup",traceId:null,semanticOperationId:"runtime:start",sourceRevision,attributes:{port:12345,readiness:"ready"}}));
    setTimeout(()=>listener(message),11000);
  }else listener(message);
}):listener)} as typeof process.on;
if(typeof process.send==="function"){
const originalSend=process.send.bind(process);process.send=((message:any,...args:any[])=>{
  if(message?.kind==="ready"){
    record("ready_send");
    if(mode==="held-ready"){setTimeout(()=>originalSend(message,...args),11000);return true;}
    if(mode==="wrong-ready-source")message={...message,sourceRevision:"src_wrong"};
    if(mode==="extra-ready-field")message={...message,extra:true};
    if(mode==="stale-ready-owner")message={...message,owner:crypto.randomUUID()};
  }
  return originalSend(message,...args);
}) as typeof process.send;
}
`);
  return { marker, preload };
}
for (const mode of ["held-complete", "held-complete-node", "held-ready", "wrong-ready-source", "extra-ready-field", "stale-ready-owner"] as const) {
  test(`closed final handoff ${mode} retains original budget and cannot publish readiness`, async () => {
    const target = clone(mode, mode === "held-complete-node" ? nodeGenerated : generated), {marker,preload} = handoffProbe(target,mode === "held-complete-node" ? "held-complete" : mode), env = environment(target);
    const child = Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null","--preload",preload,join(target,"app.ts")], {env,stdout:"pipe",stderr:"pipe"});
    const state={stdout:"",stderr:""};
    const drain=async(stream:ReadableStream<Uint8Array>,key:"stdout"|"stderr")=>{state[key]=await new Response(stream).text()};
    const instance={child,state,env,output:Promise.all([drain(child.stdout,"stdout"),drain(child.stderr,"stderr")])};
    // The network observer has its own acknowledged native lifetime. Pending
    // fixture fetches cannot hold test cleanup after serving-child termination.
    const observerFile=join(target,"network-observer.ts");
    writeFileSync(observerFile,`let polls=0,bindings=0;process.once("SIGTERM",()=>{console.log(JSON.stringify({polls,bindings}));process.exit(0)});while(true){for(const port of ${JSON.stringify([env.PORT,env.JADPO_DEPLOYMENT_PORT])}){polls++;try{await fetch("http://127.0.0.1:"+port+"/health/live",{signal:AbortSignal.timeout(100)});bindings++}catch{}}await Bun.sleep(20)}`);
    const observer=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",observerFile],{env:{PATH:Bun.env.PATH??""},stdout:"pipe",stderr:"pipe"});
    const observerOutput=new Response(observer.stdout).text(),observerErrors=new Response(observer.stderr).text();
    const started=performance.now();let observationsComplete=false;
    try {
      expect(await finish(instance)).toBe(1);const elapsed=performance.now()-started;failed(state);
      const observations=readFileSync(marker,"utf8").trim().split("\n").map(line=>JSON.parse(line));
      const pids=[...new Set(observations.map(row=>row.pid))];expect(pids).toHaveLength(2);
      for(const pid of pids)expect(()=>process.kill(pid,0)).toThrow();
      observer.kill("SIGTERM");expect(await observer.exited).toBe(0);expect(await observerErrors).toBe("");
      const {polls,bindings}=JSON.parse(await observerOutput);observationsComplete=true;
      expect(polls).toBeGreaterThan(0);
      if(mode.startsWith("held-complete")){
        expect(observations.some(row=>row.phase==="complete_held")).toBe(true);
        expect(observations.some(row=>row.phase.endsWith("_bind"))).toBe(false);expect(bindings).toBe(0);
      }else expect(observations.some(row=>row.phase==="ready_send")).toBe(true);
      if(mode.startsWith("held-")){expect(elapsed).toBeGreaterThanOrEqual(9500);expect(elapsed).toBeLessThan(12000)}
      for(const selected of [env.PORT,env.JADPO_DEPLOYMENT_PORT])await expect(fetch(`http://127.0.0.1:${selected}/health/live`)).rejects.toThrow();
      console.log(JSON.stringify({kind:"owned_final_handoff",mode,elapsedMs:elapsed,polls,bindings,observations,exitAcknowledged:true}));
    }finally{if(!observationsComplete){observer.kill("SIGKILL");await observer.exited}if(child.exitCode===null){child.kill("SIGKILL");await finish(instance)}}
  },16000);
}
test("imported application preserves library exports and does not spawn or bind; symlink native entry stays supervised", async () => {
  const target = clone("library"), importer = join(target, "library.ts");
  writeFileSync(importer, 'const app = await import("./app.ts"); const owner = await import("./startup-owner.ts"); console.log(JSON.stringify({handle:typeof app.handleRequest,deadline:owner.initialNativeStartupDeadline(),main:import.meta.main}));');
  const library = spawn(target, environment(target), importer); expect(await finish(library)).toBe(0);
  expect(JSON.parse(library.state.stdout)).toEqual({ handle: "function", deadline: null, main: true }); expect(library.state.stderr).toBe("");
  const alias = join(target, "alias.ts"); symlinkSync(join(target, "app.ts"), alias);
  const native = spawn(target, environment(target), alias);
  try { await until(() => native.state.stdout.includes("runtime.ready")); native.child.kill("SIGTERM"); expect(await finish(native)).toBe(143); expect(native.state.stderr).toBe(""); }
  finally { if (native.child.exitCode === null) { native.child.kill("SIGKILL"); await finish(native); } }
}, 15000);
test("first ESM dependency holds parent before actual persistence evaluation; only the acknowledged child evaluates it", async () => {
  const target = clone("evaluation-order"), marker = join(target, "evaluated.jsonl"), file = join(target, "persistence.ts");
  // Observer-only fixture injection preserves the generated execution body.
  writeFileSync(file, `import {appendFileSync as recordOwnedEvaluation} from "node:fs";recordOwnedEvaluation(${JSON.stringify(marker)},JSON.stringify({pid:process.pid})+"\\n");\n` + readFileSync(file, "utf8"));
  const instance = spawn(target);
  try {
    await until(() => instance.state.stdout.includes("runtime.ready"));
    const owners = readFileSync(marker, "utf8").trim().split("\n").map(line => JSON.parse(line).pid);
    expect(owners).toHaveLength(1); expect(owners[0]).not.toBe(instance.child.pid);
    instance.child.kill("SIGTERM"); expect(await finish(instance)).toBe(143);
    expect(() => process.kill(owners[0], 0)).toThrow(); expect(instance.state.stderr).toBe("");
  } finally { if (instance.child.exitCode === null) { instance.child.kill("SIGKILL"); await finish(instance); } }
}, 15000);
test("bounded native stdio preserves existing typed attempt and maintenance audits after startup, excluding raw text and extra fields", async () => {
  const target = clone("safe-audits"), file = join(target, "app.ts"), original = readFileSync(file, "utf8");
  const revision = original.match(/src_[0-9a-f]{16}/u)![0], operationId = crypto.randomUUID();
  const records = [
    { schemaVersion: 1, kind: "operational_log_event", eventName: "service.attempt", operationId, attempt: 1, elapsedMs: 1, outcome: "pre_dispatch_failure", retryScheduled: false, retryDelayMs: null },
    { schemaVersion: 1, kind: "operational_log_event", eventName: "transaction.attempt", operationId, operation: "Account.create", attempt: 1, database: "postgres", phase: "begin", outcome: "no_commit", retryableCause: "postgres_connection", rollbackProven: false, commitAcknowledged: false, elapsedMs: 1, selectedDelayMs: null, retryScheduled: false, cancelled: false },
    { schemaVersion: 1, kind: "maintenance_audit", eventName: "retention.purge.batch", operation: "retention_purge(Account)", entity: "Account", clause: "Account.lifecycle.purge", sourceRevision: revision, retentionBinding: "config.retention", attemptNow: "2026-10-06T15:00:00.000Z", selectedCount: 1, committedCount: 1, rowIdentityTokens: ["0123456789abcdef"], bound: 500, outcome: "committed" },
  ];
  // Transport controls only; these fixture records do not claim real provider
  // attempts or lifecycle deletion. Ordinary generated execution is untouched.
  writeFileSync(file, original + `\nif(import.meta.main){for(const record of ${JSON.stringify(records)})console.error(JSON.stringify(record));console.error("PRIVATE_RAW_TEXT");console.error(JSON.stringify(${JSON.stringify({ ...records[0], privateValue: "PRIVATE_EXTRA_FIELD" })}));}\n`);
  const instance = spawn(target);
  try {
    await until(() => instance.state.stderr.includes("retention.purge.batch"));
    instance.child.kill("SIGTERM"); expect(await finish(instance)).toBe(143);
    expect(instance.state.stderr.trim().split("\n").map(line => JSON.parse(line))).toEqual(records);
    expect(instance.state.stderr).not.toContain("PRIVATE_");
  } finally { if (instance.child.exitCode === null) { instance.child.kill("SIGKILL"); await finish(instance); } }
}, 15000);
test("closed maintenance schedule failure is relayed before final startup and after ready, without raw or extra fields", async () => {
  const target=clone("schedule-failure-relay"),file=join(target,"persistence.ts"),original=readFileSync(file,"utf8");
  const record={schemaVersion:1,kind:"maintenance_audit",eventName:"retention.schedule.failure",outcome:"failed"};
  const rejected=[{...record,privateError:"PRIVATE_SCHEDULER_ERROR"},{...record,outcome:"succeeded"},{...record,eventName:"retention.schedule.other"}];
  // Transport-only controls inside the actual supervised generated child.
  // Actual compiler scheduler production is qualified in the integration packet.
  writeFileSync(file,`console.error(JSON.stringify(${JSON.stringify(record)}));for(const event of ${JSON.stringify(rejected)})console.error(JSON.stringify(event));console.error("PRIVATE_SCHEDULER_RAW");\n`+original);
  const app=join(target,"app.ts");writeFileSync(app,readFileSync(app,"utf8")+`\nif(import.meta.main)console.error(JSON.stringify(${JSON.stringify(record)}));`);
  const instance=spawn(target);
  try{
    await until(()=>instance.state.stdout.includes("runtime.ready")&&instance.state.stderr.trim().split("\n").length===2);
    expect(instance.state.stderr.trim().split("\n").map(line=>JSON.parse(line))).toEqual([record,record]);
    expect(instance.state.stdout+instance.state.stderr).not.toContain("PRIVATE_");
    instance.child.kill("SIGTERM");expect(await finish(instance)).toBe(143);
    expect(instance.state.stderr).not.toContain("RUNTIME_STARTUP_FAILED");
  }finally{if(instance.child.exitCode===null){instance.child.kill("SIGKILL");await finish(instance)}}
},15000);
test("static dependency failure is suppressed before app catch, while parent emits one safe fault", async () => {
  const target = clone("import-failure"), canary = "PRIVATE_STATIC_DATABASE_PROVIDER_CREDENTIAL";
  const file = join(target, "persistence.ts"); writeFileSync(file, `throw Error(${JSON.stringify(canary)});\n` + readFileSync(file, "utf8"));
  const instance = spawn(target); expect(await finish(instance)).toBe(1); failed(instance.state);
  expect(instance.state.stdout + instance.state.stderr).not.toContain(canary); expect(existsSync(instance.env.SQLITE_PATH)).toBe(false);
});
test("original parent budget terminates a synchronously blocked static dependency with native exit acknowledgement", async () => {
  const target = clone("blocked-import"), file = join(target, "persistence.ts");
  // Deliberate native blocking injection in generated dependency, not a claim
  // that a particular SQLite constructor currently takes twenty seconds.
  writeFileSync(file, 'Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,20000);\n' + readFileSync(file, "utf8"));
  const started = performance.now(), instance = spawn(target); expect(await finish(instance)).toBe(1);
  const elapsed = performance.now() - started; expect(elapsed).toBeGreaterThanOrEqual(9500); expect(elapsed).toBeLessThan(12000); failed(instance.state);
  expect(existsSync(instance.env.SQLITE_PATH)).toBe(false);
  console.log(JSON.stringify({ kind: "owned_blocked_import", elapsedMs: elapsed, exitAcknowledged: true }));
}, 16000);
test("bounded startup pipe overflow terminates the owner without disclosing raw dependency output", async () => {
  const target = clone("overflow"), file = join(target, "persistence.ts");
  writeFileSync(file, 'console.error("PRIVATE_OUTPUT_".repeat(10000));Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,20000);\n' + readFileSync(file, "utf8"));
  const instance = spawn(target); expect(await finish(instance, 5000)).toBe(1); failed(instance.state);
  expect(instance.state.stdout + instance.state.stderr).not.toContain("PRIVATE_OUTPUT"); expect(existsSync(instance.env.SQLITE_PATH)).toBe(false);
});
for (const mode of ["wrong-source", "extra-field", "channel-loss"] as const) {
  test(`actual native IPC ${mode} refuses database import and listeners`, async () => {
    const target = clone(mode); let boot = false;
    const instance = spawn(target, environment(target), join(target, "app.ts"), { ipc(message: any, child: any) {
      boot = true;
      if (mode === "channel-loss") child.disconnect();
      else child.send({ kind: "owned", owner: crypto.randomUUID(), sourceRevision: mode === "wrong-source" ? "src_wrong" : message.sourceRevision, remainingMs: 1000, ...(mode === "extra-field" ? { extra: true } : {}) });
    } });
    expect(await finish(instance, 5000)).toBe(1); expect(boot).toBe(true); expect(existsSync(instance.env.SQLITE_PATH)).toBe(false); expect(instance.state.stdout).not.toContain("runtime.ready");
  });
}
for (const mode of ["stale-owner", "replayed-owned"] as const) {
  test(`actual native IPC ${mode} cannot authorize either listener`, async () => {
    const target = clone(mode), owner = crypto.randomUUID(); let boot = false;
    const instance = spawn(target, environment(target), join(target, "app.ts"), { ipc(message: any, child: any) {
      if (message.kind === "boot") {
        boot = true; const owned = { kind: "owned", owner, sourceRevision: message.sourceRevision, remainingMs: 2000 }; child.send(owned);
        if (mode === "replayed-owned") child.send(owned);
      }
      if (message.kind === "validated" && mode === "stale-owner") child.send({ kind: "continue", owner: crypto.randomUUID(), sourceRevision: message.sourceRevision });
    } });
    expect(await finish(instance, 5000)).toBe(1); expect(boot).toBe(true); expect(instance.state.stdout).not.toContain("runtime.ready");
    for (const selected of [instance.env.PORT, instance.env.JADPO_DEPLOYMENT_PORT]) await expect(fetch(`http://127.0.0.1:${selected}/health/live`)).rejects.toThrow();
  });
}
test("valid late CONTINUE cannot replenish child budget or bind either listener", async () => {
  const target = clone("late-continue"); let owner = crypto.randomUUID(), boot = false, validated = false;
  const instance = spawn(target, environment(target), join(target, "app.ts"), { ipc(message: any, child: any) {
    if (message.kind === "boot") { boot = true; child.send({ kind: "owned", owner, sourceRevision: message.sourceRevision, remainingMs: 250 }); }
    if (message.kind === "validated") { validated = true; setTimeout(() => child.send({ kind: "continue", owner, sourceRevision: message.sourceRevision }), 350); }
  } });
  expect(await finish(instance, 5000)).toBe(1); expect(boot).toBe(true); expect(validated).toBe(true); expect(instance.state.stdout).not.toContain("runtime.ready");
  for (const selected of [instance.env.PORT, instance.env.JADPO_DEPLOYMENT_PORT]) await expect(fetch(`http://127.0.0.1:${selected}/health/live`)).rejects.toThrow();
});
test.skipIf(!postgres)("actual initial PostgreSQL COMMIT acknowledgement stall is ended by emitted owner, preserving unknown committed schema", async () => {
  const result = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", resolve("tests/runtime/fixtures/startup-owned-commit.ts")], {
    env: { PATH: Bun.env.PATH ?? "", JADPO_STARTUP_OWNER_DATABASE_URL: postgres!, JADPO_STARTUP_OWNER_APP: join(generated, "app.ts") }, stdout: "pipe", stderr: "pipe",
  });
  const [stdout, stderr, exit] = await Promise.all([new Response(result.stdout).text(), new Response(result.stderr).text(), result.exited]);
  if(exit !== 0) throw Error(`Native owned COMMIT fixture failed: ${stdout}\n${stderr}`);
  expect(exit).toBe(0); expect(stderr).toBe(""); const proof = JSON.parse(stdout); expect(proof.expiryObserved).toBe(false); expect(proof.backendRemaining).toBe(0); expect(proof.clientCloseObserved).toBe(true); expect(proof.terminationExit).toBe(1); expect(proof.startupFaultReported).toBe(true); expect(proof.connections).toBeGreaterThan(0); expect(proof.closedConnections).toBe(proof.connections); expect(proof.allBackendsRemaining).toBe(0); console.log(stdout.trim());
}, 20000);
