import {expect} from "bun:test";
import {SQL} from "bun";
import {Database} from "bun:sqlite";
import {join} from "node:path";
import {readFileSync} from "node:fs";
import {pathToFileURL} from "node:url";
const [project,raw]=Bun.argv.slice(2),c=JSON.parse(raw),pg=Bun.env.JADPO_TEMPORAL_PAGE_PROBE_DATABASE_URL;
if(pg){Bun.env.DATABASE_URL=pg;delete Bun.env.SQLITE_PATH;}else{delete Bun.env.DATABASE_URL;Bun.env.SQLITE_PATH=join(project,"probe.sqlite");}
delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
const sourceRevision=readFileSync(join(project,"build/target/app.ts"),"utf8").match(/const sourceRevision = "([^"]+)";/u)?.[1];
expect(sourceRevision).toMatch(/^src_[0-9a-f]+$/u);
const events:any[]=[],faults:any[]=[],observations:any[]=[];
const originalError=console.error;
console.error=(...args:any[])=>{for(const x of args){if(typeof x==="string"){try{const j=JSON.parse(x);if(j.kind==="operational_log_event")events.push(j);}catch{originalError(x);}}else if(x instanceof Error)faults.push({name:x.name,constructorName:x.constructor.name,kind:(x as any).kind??null,operation:(x as any).operation??null,code:(x as any).code??null,causeCode:(x as any).cause?.code??null});}};
const app=await import(pathToFileURL(join(project,"build/target/app.ts")).href);
const {isPersistenceReady}=await import(pathToFileURL(join(project,"build/target/persistence.ts")).href);
const observer=await import(pathToFileURL(join(project,"build/target/temporal-page-sql-observer.ts")).href);
expect(isPersistenceReady()).toBe(true);
const inspector=pg?new SQL({url:pg,prepare:false}):new Database(Bun.env.SQLITE_PATH!);
const exec=async(sql:string,values:any[]=[])=>pg?await (inspector as SQL).unsafe(sql,values):(inspector as Database).prepare(sql).all(...values);
const server=Bun.serve({hostname:"127.0.0.1",port:0,fetch:app.handleRequest}),deployment=app.createDeploymentServer(0);
const ports={application:server.port,deployment:deployment.port};expect(ports.application).not.toBe(ports.deployment);
const rawForm=(v:any):any=>v instanceof Date?{native:"Date",iso:Number.isNaN(v.getTime())?"invalid":v.toISOString()}:Array.isArray(v)?v.map(rawForm):v&&typeof v==="object"?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,rawForm(x)])):v;
const request=async(path:string,input:any)=>{const r=await fetch(`http://127.0.0.1:${server.port}${path}`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(input),signal:AbortSignal.timeout(4000)});return {status:r.status,body:await r.json()};};
const ready=async()=>{const r=await fetch(`http://127.0.0.1:${deployment.port}/health/ready`,{signal:AbortSignal.timeout(4000)});expect(r.status).toBe(200);return {status:r.status,body:await r.json()};};
const ids=[crypto.randomUUID(),crypto.randomUUID()];
const rows=ids.map((id,i)=>({id,bucket:"probe",occurs_at:`2026-06-01T0${i+1}:00:00.000Z`,calendar_date:"2024-02-29"}));
const window={bucket:"probe",start:"2026-06-01T00:00:00.000Z",end:"2026-06-02T00:00:00.000Z",page_size:2};
const physical=async()=>{const r=await exec(pg?'SELECT "id", "calendar_date"::text AS "calendar_date", pg_typeof("calendar_date")::text AS "kind" FROM "day_event" ORDER BY "occurs_at", "id"':'SELECT "id", "calendar_date", typeof("calendar_date") AS "kind" FROM "day_event" ORDER BY "occurs_at", "id"');expect(r.map((x:any)=>x.id)).toEqual(ids);return r.map((x:any)=>({...x}));};
const native=async()=>rawForm(await exec('SELECT "id", "calendar_date" FROM "day_event" ORDER BY "occurs_at", "id"'));
const update=async(value:string)=>{const sql=`UPDATE "day_event" SET "calendar_date" = $1${pg?"::date":""} WHERE "id" = $2 RETURNING "id"`;expect(await exec(sql,[value,ids[0]])).toEqual([{id:ids[0]}]);observations.push({phase:"external_fixture_mutation",sql,parameters:[value,ids[0]]});};
let failed:any=null,baseline:any,baselineResponse:any,changed=false;const ownedPostgresPids:number[]=[];
try{
 observations.push({phase:"initialized",readiness:await ready()});
 // This dedicated disposable database is shared only by these sequential cases.
 // Clear prior fixture rows before real authored creates, outside query tracing.
 if(pg){await exec('DELETE FROM "day_event"');expect((await exec('SELECT COUNT(*)::integer AS count FROM "day_event"'))[0].count).toBe(0);observations.push({phase:"owned_fixture_reset",sql:'DELETE FROM "day_event"',empty:true});}
 for(const input of rows)expect(await request("/day/create",input)).toEqual({status:200,body:input});
 baselineResponse=await request("/day/page",window);expect(baselineResponse).toEqual({status:200,body:{items:rows,next:null}});
 baseline=await physical();if(pg)ownedPostgresPids.push(...(await exec("SELECT pid FROM pg_stat_activity WHERE datname=current_database()")).map((r:any)=>r.pid));observations.push({phase:"baseline",physical:baseline,native:await native(),response:baselineResponse});
 if(!c.omit&&!c.baselineOnly){await update(pg?"10000-01-01":"2026-02-30");changed=true;}
 const before=await physical();if(changed){expect(before[0]).toEqual({id:ids[0],calendar_date:pg?"10000-01-01":"2026-02-30",kind:pg?"date":"text"});expect(before[1]).toEqual(baseline[1]);}
 observations.push({phase:"physically_stored_before_page",physical:before,native:await native()});
 const e=events.length,f=faults.length,journal=observer.beginPageObservation();Bun.env.JADPO_DEBUG_TARGET_STACKS="1";
 const response=await request("/day/page",window);delete Bun.env.JADPO_DEBUG_TARGET_STACKS;observer.endPageObservation();
 const actualEvents=events.slice(e),actualFaults=faults.slice(f);observations.push({phase:"typed_page",response,journal,events:actualEvents,faults:actualFaults});
 expect(journal).toHaveLength(1);expect(journal[0].operation).toBe("SELECT");expect(journal[0].sql).toContain('"calendar_date"');expect(journal[0].sql).toContain('FROM "day_event"');expect(await physical()).toEqual(before);
 if(c.baselineOnly){expect(response).toEqual(baselineResponse);expect(actualFaults).toEqual([]);expect(actualEvents).toEqual([]);}
 else{
  expect(response.status).toBe(500);expect(Object.keys(response.body)).toEqual(["error"]);expect(Object.keys(response.body.error).sort()).toEqual(["code","message","request_id"]);expect(response.body.error).toMatchObject({code:"internal_fault",message:"An internal error occurred."});expect(response.body.error.request_id).toMatch(/^req_[0-9a-f-]{36}$/u);
  expect(actualEvents).toHaveLength(1);expect(actualEvents[0]).toMatchObject({eventName:"operation.failed",classification:"RUNTIME_UNHANDLED_FAULT",requestId:response.body.error.request_id,semanticOperationId:"route:POST:/day/page",sourceRevision,attributes:{}});
  const publicOutput=JSON.stringify({response,events:actualEvents});for(const text of ["SELECT","UPDATE","day_event","calendar_date","database.","stack"])expect(publicOutput).not.toContain(text);
  if(c.strict){expect(actualFaults).toHaveLength(1);expect(actualFaults[0]).toMatchObject({name:"PersistenceFault",constructorName:"PersistenceFault",kind:"data",operation:"decode.DayPage"});expect(typeof actualFaults[0].operation).toBe("string");}
 }
}catch(error){failed={name:(error as Error).name,message:(error as Error).message};}
finally{
 observer.endPageObservation();delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
 try{if(changed)await update("2024-02-29");if(baseline){expect(await physical()).toEqual(baseline);expect(await request("/day/page",window)).toEqual(baselineResponse);expect((await ready()).status).toBe(200);if(!pg)expect((await exec("PRAGMA ignore_check_constraints"))[0].ignore_check_constraints).toBe(0);observations.push({phase:"restored",physical:await physical(),response:await request("/day/page",window),readiness:await ready()});}}catch(error){failed??={name:"cleanup",message:(error as Error).message};}
 if(inspector instanceof Database)inspector.close();else await inspector.close();server.stop(true);deployment.stop(true);console.error=originalError;
}
console.log(JSON.stringify({scope:"instrumented_generated_copy_native_page",case:c,backend:pg?"postgres":"sqlite",sourceRevision,ids,ports,ownedPostgresPids,observations,failed}));
process.exit(failed?1:0);
