import {afterAll, expect, test} from "bun:test";
import {SQL} from "bun";
import {createHash} from "node:crypto";
import {Database} from "bun:sqlite";
import {cpSync,mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync} from "node:fs";
import {join,resolve} from "node:path";
import {tmpdir} from "node:os";

// Initial/recovery physical declarations only. Imported probes are not auth or
// main evidence; explicitly labelled actual native-main probes own that claim.
const pg=Bun.env.JADPO_TEMPORAL_SCHEMA_DATABASE_URL;
if(pg){const u=new URL(pg);if(!["postgres:","postgresql:"].includes(u.protocol)||!["127.0.0.1","localhost"].includes(u.hostname)||!/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(u.pathname))throw Error("Requires disposable local temporal schema database");}
const root=mkdtempSync(join(tmpdir(),"jadpo-temporal-schema-"));
const project=join(root,"source");mkdirSync(project);
const fixtureSource="type StoredInstant = Instant {}\ntype StoredElapsed = Duration {}\n"+readFileSync(resolve("tests/runtime/fixtures/temporal-roundtrip.jadpo"),"utf8").replace("instant: Instant\n","instant: StoredInstant\n").replace("elapsed: Duration\n","elapsed: StoredElapsed\n");
writeFileSync(join(project,"app.jadpo"),fixtureSource);
const built=Bun.spawnSync([Bun.env.JADPO_BIN??resolve("jadpo/target/debug/jadpo"),"build",project],{stdout:"pipe",stderr:"pipe"});
if(built.exitCode){rmSync(root,{recursive:true,force:true});throw Error("Schema fixture compiler failed");}
const receiptBase=resolve("build/validation");mkdirSync(receiptBase,{recursive:true});const receipt=mkdtempSync(join(receiptBase,"rm504-temporal-schema-runtime."));
const generatedPins:Record<string,string>={};
for(const name of ["target/app.ts","target/persistence.ts","target/startup-owner.ts","sql/sqlite/schema.sql","sql/postgres/schema.sql","persistence/entities.json","approval/subject.json"]){const bytes=readFileSync(join(project,"build",name));const destination=join(receipt,name);mkdirSync(resolve(destination,".."),{recursive:true});writeFileSync(destination,bytes);generatedPins[name]=createHash("sha256").update(bytes).digest("hex");}
writeFileSync(join(receipt,"app.jadpo"),fixtureSource);console.log(JSON.stringify({kind:"actual_temporal_schema_generated_receipt",backend:pg?"postgres":"sqlite",receipt,generatedPins}));
const sql=pg?new SQL({url:pg,prepare:false,max:1}):null;
afterAll(async()=>{await sql?.close();rmSync(root,{recursive:true,force:true});});
let serial=0;
function port(){const s=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});const p=s.port;s.stop(true);return String(p);}
async function setup(replace:(ddl:string)=>string){
 const target=join(root,String(++serial));cpSync(join(project,"build/target"),target,{recursive:true});
 const ddl=replace(readFileSync(join(project,"build/sql",pg?"postgres":"sqlite","schema.sql"),"utf8"));
 const env:Record<string,string>={PATH:Bun.env.PATH??"",PORT:port(),JADPO_DEPLOYMENT_PORT:port()};
 if(pg){env.DATABASE_URL=pg;await sql!.unsafe('DROP TABLE IF EXISTS "temporal_record" CASCADE');await sql!.unsafe(ddl);}
 else{env.SQLITE_PATH=join(target,"store.sqlite");const db=new Database(env.SQLITE_PATH);try{db.exec(ddl);}finally{db.close();}}
 return{target,env};
}
async function run(target:string,env:Record<string,string>,entry:string,preload?:string){
 const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",...(preload?["--preload",preload]:[]),entry],{env,stdout:"pipe",stderr:"pipe"});
 const out=new Response(child.stdout).text(),err=new Response(child.stderr).text();let watchdog=false;
 const timer=setTimeout(()=>{watchdog=true;child.kill("SIGKILL");},15000);
 try{const exit=await child.exited;return{exit,stdout:await out,stderr:await err,watchdog,pid:child.pid};}finally{clearTimeout(timer);if(child.exitCode===null){child.kill("SIGKILL");await child.exited;}}
}
function observer(target:string){
 const marker=join(target,"native.jsonl"),preload=join(target,"observe.ts");
 writeFileSync(preload,`import {appendFileSync} from "node:fs";import {SQL} from "bun";
import {createHash} from "node:crypto";import {Server} from "node:http";
const record=(phase:string)=>appendFileSync(${JSON.stringify(marker)},JSON.stringify({pid:process.pid,phase})+"\\n");record("process");
const spawn=Bun.spawn;Bun.spawn=((args:any,opts:any)=>spawn([...args.slice(0,-1),"--preload",${JSON.stringify(preload)},args.at(-1)],opts))as typeof Bun.spawn;
const serve=Bun.serve;Bun.serve=((opts:any)=>{record("bun_bind");return serve(opts)})as typeof Bun.serve;
const listen=Server.prototype.listen;Server.prototype.listen=function(...args:any[]){record("node_bind");return listen.apply(this,args as any)}as typeof listen;

`);
 return{marker,preload};
}
const cases:Array<[string,(ddl:string)=>string,boolean]>=[
 ["fresh physical declarations",s=>s,false],
 ["Duration TEXT",s=>s.replace('"elapsed" BIGINT','"elapsed" TEXT'),true],
 ["Duration missing",s=>s.replace(/^\s*"elapsed"[^\n]*\n/mu,""),true],
 ["Duration noninteger",s=>s.replace('"elapsed" BIGINT',pg?'"elapsed" NUMERIC':'"elapsed" REAL'),true],
 ["Instant wrong kind",s=>s.replace('"instant" '+(pg?'TIMESTAMPTZ(3)':'BIGINT'),'"instant" TEXT'),true],
 ["required Duration nullable",s=>s.replace('"elapsed" BIGINT NOT NULL','"elapsed" BIGINT'),true],
 ["optional Instant required",s=>s.replace('"optional_instant" '+(pg?'TIMESTAMPTZ(3)':'BIGINT'),'"optional_instant" '+(pg?'TIMESTAMPTZ(3)':'BIGINT')+' NOT NULL'),true],
 ...(pg?[
 ["Instant higher precision",(s:string)=>s.replace('"instant" TIMESTAMPTZ(3)','"instant" TIMESTAMPTZ(6)'),true],
 ["Instant unbounded precision",(s:string)=>s.replace('"instant" TIMESTAMPTZ(3)','"instant" TIMESTAMPTZ'),true],
 ["Instant zoneless",(s:string)=>s.replace('"instant" TIMESTAMPTZ(3)','"instant" TIMESTAMP(3)'),true],
 ]as Array<[string,(s:string)=>string,boolean]>:[
 ["SQLite INTEGER alias affinity",(s:string)=>s.replaceAll('BIGINT','INTEGER'),false],
 ]as Array<[string,(s:string)=>string,boolean]>),
];
for(const[name,replace,fatal]of cases)test(`actual imported initializer ${name}`,async()=>{
 const{target,env}=await setup(replace),probe=join(target,"probe.ts");
 writeFileSync(probe,'const app=await import("./app.ts");const p=await import("./persistence.ts");const fatal=p.isPersistenceInitializationFatal();let refusal=null;if(fatal){const r=await app.handleRequest(new Request("http://temporal.test/temporal/find",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({id:"00000000-0000-4000-8000-000000000001"})}));const body=await r.json();refusal={status:r.status,code:body.error.code,details:body.error.details??null};}console.log(JSON.stringify({fatal,ready:p.isPersistenceReady(),refusal}));');
 const result=await run(target,env,probe);expect(result.watchdog).toBe(false);expect(result.exit).toBe(0);
 const state=JSON.parse(result.stdout.trim());expect(state).toEqual({fatal,ready:!fatal,refusal:fatal?{status:503,code:"dependency_unavailable",details:null}:null});
 expect(result.stderr.includes("RUNTIME_STARTUP_FAILED")).toBe(fatal);
 expect(result.stderr).not.toContain('SELECT ');expect(result.stderr).not.toContain('temporal_record');
 console.log(JSON.stringify({kind:"initial_temporal_schema_declaration",backend:pg?"postgres":"sqlite",name,state,main:false}));
},20000);
test.skipIf(!pg)("same generated relation resolves active search_path rather than public namesake",async()=>{
 const{target,env}=await setup(s=>s),probe=join(target,"probe.ts");
 await sql!.unsafe('CREATE SCHEMA temporal_gate_shadow');
 const ddl=readFileSync(join(project,"build/sql/postgres/schema.sql"),"utf8").replaceAll('"temporal_record"','"temporal_gate_shadow"."temporal_record"').replace('"elapsed" BIGINT','"elapsed" TEXT');
 await sql!.unsafe(ddl);await sql!.unsafe('ALTER ROLE jadpo_auth_test SET search_path = temporal_gate_shadow, public');
 try{writeFileSync(probe,'const p=await import("./persistence.ts");console.log(JSON.stringify({fatal:p.isPersistenceInitializationFatal(),ready:p.isPersistenceReady()}));');
 const result=await run(target,env,probe);expect(result.exit).toBe(0);expect(JSON.parse(result.stdout.trim())).toEqual({fatal:true,ready:false});
 console.log(JSON.stringify({kind:"actual_catalog_search_path",public:"compatible",activeRelation:"incompatible",fatal:true}));
 }finally{await sql!.unsafe('ALTER ROLE jadpo_auth_test RESET search_path');await sql!.unsafe('DROP SCHEMA temporal_gate_shadow CASCADE');}
},20000);
test.skipIf(Boolean(pg))("recovery initialization checks declarations before publishing ready",async()=>{
 const{target,env}=await setup(s=>s),probe=join(target,"recovery.ts");
 env.SQLITE_PATH=join(target,"absent","store.sqlite");
 const ddl=readFileSync(join(project,"build/sql/sqlite/schema.sql"),"utf8").replace('"elapsed" BIGINT','"elapsed" TEXT');
 writeFileSync(probe,`import {mkdirSync} from "node:fs";import {Database} from "bun:sqlite";const p=await import("./persistence.ts");const before={fatal:p.isPersistenceInitializationFatal(),ready:p.isPersistenceReady()};mkdirSync(${JSON.stringify(join(target,"absent"))});const db=new Database(Bun.env.SQLITE_PATH!);db.exec(${JSON.stringify(ddl)});db.close();await p.refreshPersistenceReadiness();console.log(JSON.stringify({before,after:{fatal:p.isPersistenceInitializationFatal(),ready:p.isPersistenceReady()}}));`);
 const result=await run(target,env,probe);expect(result.exit).toBe(0);expect(JSON.parse(result.stdout.trim())).toEqual({before:{fatal:false,ready:false},after:{fatal:true,ready:false}});
 console.log(JSON.stringify({kind:"actual_recovery_temporal_schema_declaration",before:"unavailable",after:"incompatible_fatal",ready:false}));
},20000);
test("actual main schema contradiction exits before listeners and safe readiness",async()=>{
 const{target,env}=await setup(s=>s.replace('"elapsed" BIGINT','"elapsed" TEXT')),{marker,preload}=observer(target);
 const result=await run(target,env,join(target,"app.ts"),preload);expect(result.watchdog).toBe(false);expect(result.exit).toBe(1);
 expect(result.stdout).not.toContain("runtime.ready");const events=result.stderr.trim().split("\n").map(s=>JSON.parse(s));
 expect(events.length).toBe(2);for(const event of events){expect(event.classification).toBe("RUNTIME_STARTUP_FAILED");expect(event.attributes).toEqual({});}
 const records=readFileSync(marker,"utf8").trim().split("\n").map(s=>JSON.parse(s));expect(records.some(r=>r.phase.endsWith("_bind"))).toBe(false);
 const pids=[...new Set(records.map(r=>r.pid))];expect(pids).toHaveLength(2);for(const pid of pids)expect(()=>process.kill(pid,0)).toThrow();
 for(const p of [env.PORT,env.JADPO_DEPLOYMENT_PORT])await expect(fetch("http://127.0.0.1:"+p+"/health/live",{signal:AbortSignal.timeout(200)})).rejects.toThrow();
 if(sql)expect((await sql.unsafe("SELECT pid FROM pg_catalog.pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()"))).toHaveLength(0);
 console.log(JSON.stringify({kind:"actual_main_temporal_schema_refusal",backend:pg?"postgres":"sqlite",exit:result.exit,pidsGone:pids.length,binds:0,ready:false,watchdog:result.watchdog}));
},20000);
test.skipIf(!pg)("actual catalog SELECT server acknowledgement withheld retains original owner deadline",async()=>{
 const{target}=await setup(s=>s);
 const result=await run(target,{PATH:Bun.env.PATH??"",JADPO_TEMPORAL_SCHEMA_DATABASE_URL:pg!,JADPO_TEMPORAL_SCHEMA_APP:join(target,"app.ts")},resolve("tests/runtime/fixtures/startup-owned-temporal-catalog.ts"));
 if(result.exit!==0)throw Error(`Actual catalog fixture failed: ${result.stdout}\n${result.stderr}`);expect(result.watchdog).toBe(false);expect(result.exit).toBe(0);expect(result.stderr).toBe("");
 const proof=JSON.parse(result.stdout);expect(proof.expiryObserved).toBe(false);expect(proof.catalogForwarded).toBe(1);expect(proof.ackObserved).toBe(true);expect(proof.before.catalogExecuted).toBe(true);
 expect(proof.network.polls).toBeGreaterThan(0);expect(proof.network.bindings).toBe(0);expect(proof.listenerStarts).toBe(0);expect(proof.publishedReady).toBe(false);expect(proof.terminationExit).toBe(1);expect(proof.allBackendsRemaining).toBe(0);expect(proof.closedConnections).toBe(proof.connections);expect(proof.connections).toBeGreaterThan(0);
 console.log(result.stdout.trim());
},20000);
