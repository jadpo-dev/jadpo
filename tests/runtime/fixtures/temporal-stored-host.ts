// Isolated native observation owner. Generated modules are untouched. Debug
// observation intercepts the actual contained Error and retains only kind and
// operation, never stack/cause; ordinary public/event envelopes stay closed.
import {expect} from 'bun:test';
import {Database} from 'bun:sqlite';
import {SQL} from 'bun';
import {join} from 'node:path';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const [project,raw]=Bun.argv.slice(2), c=JSON.parse(raw),pg=Bun.env.JADPO_TEMPORAL_STORED_DATABASE_URL;
if(pg){Bun.env.DATABASE_URL=pg;delete Bun.env.SQLITE_PATH;}else{delete Bun.env.DATABASE_URL;Bun.env.SQLITE_PATH=join(project,'fixture.sqlite');}
delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
const revision=readFileSync(join(project,'build/target/app.ts'),'utf8').match(/const sourceRevision = "([^"]+)";/u)?.[1];expect(revision).toMatch(/^src_[0-9a-f]+$/u);
const events:any[]=[], faults:any[]=[], observations:any[]=[];
const originalError=console.error;
console.error=(...args:any[])=>{for(const arg of args){if(typeof arg==='string'){try{const v=JSON.parse(arg);if(v.kind==='operational_log_event')events.push(v);}catch{originalError(arg);}}else if(arg instanceof Error)faults.push({name:arg.name,kind:(arg as any).kind,operation:(arg as any).operation});}};
const app=await import(pathToFileURL(join(project,'build/target/app.ts')).href);
// This no-config source initializes persistence at generated module import.
const {persistence,isPersistenceReady}=await import(pathToFileURL(join(project,'build/target/persistence.ts')).href);
expect(isPersistenceReady()).toBe(true);
const inspector=pg?new SQL({url:pg,prepare:false}):new Database(Bun.env.SQLITE_PATH!);
const exec=async(sql:string,values:any[]=[])=>pg?await (inspector as SQL).unsafe(sql,values):(inspector as Database).prepare(sql).all(...values);
const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch:app.handleRequest}),deployment=app.createDeploymentServer(0);
const ports={application:server.port,deployment:deployment.port};expect(ports.application).not.toBe(ports.deployment);
const id=crypto.randomUUID(),input={id,instant:'2026-10-25T00:30:00.123Z',calendar_date:'2024-02-29',elapsed:'-PT1.234S',resolved:{instant:'2026-10-25T00:30:00.123Z',zone:'Europe/London'},optional_instant:null};
const request=async(path:string,value:any)=>{const r=await fetch(`http://127.0.0.1:${server.port}${path}`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(value),signal:AbortSignal.timeout(4000)});return {status:r.status,body:await r.json()};};
const ready=async()=>{const r=await fetch(`http://127.0.0.1:${deployment.port}/health/ready`);expect(r.status).toBe(200);return {status:r.status,body:await r.json()};};
const column=c.field,types:any={instant:'timestamptz',calendar_date:'date',elapsed:'bigint',resolved:'jsonb',optional_instant:'timestamptz'};
expect(Object.hasOwn(types,column)).toBe(true);
const physical=async()=>{const fields=['instant','calendar_date','elapsed','resolved','optional_instant'];const columns=fields.map(f=>pg?`"${f}"::text AS "${f}", pg_typeof("${f}")::text AS "${f}_type"`:`"${f}", typeof("${f}") AS "${f}_type"`).join(', ');const rows=await exec(`SELECT "id", ${columns} FROM "temporal_record" WHERE "id" = $1`,[id]);expect(rows).toHaveLength(1);expect(rows[0].id).toBe(id);return {...rows[0]};};
const update=async(value:any)=>{const sql=`UPDATE "temporal_record" SET "${column}" = $1${pg?'::'+types[column]:''} WHERE "id" = $2 RETURNING "id"`;const bound=pg&&column==='resolved'&&typeof value==='string'?JSON.parse(value):value;const rows=await exec(sql,[bound,id]);expect(rows).toEqual([{id}]);observations.push({phase:'external_fixture_mutation',sql,parameters:[value,id]});};
const portableWire=(decimal:string)=>{let n=BigInt(decimal),sign=n<0n?'-':'';if(n<0n)n=-n;const h=n/3600000n;n%=3600000n;const m=n/60000n;n%=60000n;const sec=n/1000n,ms=n%1000n;const fraction=ms===0n?'':'.'+ms.toString().padStart(3,'0').replace(/0+$/u,'');const pieces=(h?h+'H':'')+(m?m+'M':'')+(sec||ms?sec+fraction+'S':'');return sign+'PT'+(pieces||'0S');};
const rawForm=(v:any):any=>v instanceof Date?{native:'Date',iso:Number.isNaN(v.getTime())?'invalid':v.toISOString()}:typeof v==='bigint'?{native:'bigint',decimal:v.toString()}:v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,rawForm(x)])):v;
let baseline:any, failed:any=null, changed=false, precisionChanged=false;const ownedPostgresPids:number[]=[];
const precision=async()=>{const rows=await exec("SELECT datetime_precision FROM information_schema.columns WHERE table_schema='public' AND table_name='temporal_record' AND column_name='instant'");expect(rows).toHaveLength(1);return rows[0].datetime_precision;};
try{
 observations.push({phase:'initialized',readiness:await ready()});
 const created=await request('/temporal/create',input);expect(created).toEqual({status:200,body:input});
 const found=await request('/temporal/find',{id});expect(found).toEqual({status:200,body:input});baseline=await physical();if(pg)ownedPostgresPids.push(...(await exec('SELECT pid FROM pg_stat_activity WHERE datname = current_database()')).map((r:any)=>r.pid));observations.push({phase:'baseline',response:found,physical:baseline});
 if(c.mode==='precision-fault'){expect(await precision()).toBe(3);await exec('ALTER TABLE \"temporal_record\" ALTER COLUMN \"instant\" TYPE timestamptz(6)');precisionChanged=true;expect(await precision()).toBe(6);}
 if(c.mode==='kind-diagnostic'&&!pg){await exec('PRAGMA ignore_check_constraints = ON');}
 if(!c.omit){try{await update(c.value);changed=true;}catch(error){if(!c.storageRefusal)throw error;observations.push({phase:'native_storage_refusal',code:(error as any).code,sqlState:(error as any).errno,physical:await physical()});expect(await physical()).toEqual(baseline);expect(await request('/temporal/find',{id})).toEqual({status:200,body:input});}}
 if(c.mode==='kind-diagnostic'&&!pg){await exec('PRAGMA ignore_check_constraints = OFF');expect((await exec('PRAGMA ignore_check_constraints'))[0].ignore_check_constraints).toBe(0);}
 if(!c.storageRefusal){
 const before=await physical();if(!c.omit){expect(before[column]).not.toEqual(baseline[column]);expect(before[column+'_type']).toBe(column==='elapsed'?(c.mode==='kind-diagnostic'?'text':pg?'bigint':'integer'):column==='resolved'?(pg?'jsonb':'text'):column==='calendar_date'?(pg?'date':'text'):(pg?'timestamp with time zone':'integer'));if(column==='resolved')expect(JSON.parse(before[column])).toEqual(JSON.parse(c.value));else if(column==='elapsed')expect(String(before.elapsed)).toBe(String(c.value));else if(column==='calendar_date')expect(before.calendar_date).toBe(c.value);else if(column==='instant'&&!pg)expect(before.instant).toBe(c.value);else if(c.mode==='precision-fault')expect(before.instant).toContain('.123456');else expect(before.instant).toContain('10000-01-01');}
 const native=rawForm(await persistence.query_optional_TemporalRecord_by_id(id));observations.push({phase:'physically_stored_before_typed_query',physical:before,native});
 const eventIndex=events.length,faultIndex=faults.length;Bun.env.JADPO_DEBUG_TARGET_STACKS='1';const response=await request('/temporal/find',{id});delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
 const actualEvents=events.slice(eventIndex),actualFaults=faults.slice(faultIndex);observations.push({phase:'typed_query',response,events:actualEvents,faults:actualFaults});
 expect(await physical()).toEqual(before);expect(await ready()).toMatchObject({status:200});
 if(c.mode==='precision-fault'){expect(await precision()).toBe(6);expect(native.instant).toEqual({native:'Date',iso:'2026-10-25T00:30:00.123Z'});observations.push({phase:'precision_refused_after_lossless_typed_decode',physicalPrecision:await precision(),physicalValue:before.instant,native:native.instant,typedStatus:response.status});}
 if(c.mode==='kind-diagnostic'&&!c.strict){expect(response.status).toBe(200);expect(before.elapsed_type).toBe('text');expect(response.body.elapsed).toBe('PT1S');}
 else if(c.positive){expect(response.status).toBe(200);expect(response.body.elapsed).toBe(portableWire(c.value));expect(actualEvents).toEqual([]);expect(actualFaults).toEqual([]);}
 else{expect(response.status).toBe(500);expect(Object.keys(response.body)).toEqual(['error']);expect(Object.keys(response.body.error).sort()).toEqual(['code','message','request_id']);expect(response.body.error).toMatchObject({code:'internal_fault',message:'An internal error occurred.'});expect(response.body.error.request_id).toMatch(/^req_[0-9a-f-]{36}$/u);expect(actualFaults).toEqual([{name:'PersistenceFault',kind:'data',operation:'decode.TemporalRecord'}]);expect(actualEvents).toHaveLength(1);expect(Object.keys(actualEvents[0]).sort()).toEqual(['attributes','classification','eventName','kind','requestId','schemaVersion','semanticOperationId','sourceRevision','traceId']);expect(actualEvents[0]).toMatchObject({kind:'operational_log_event',eventName:'operation.failed',classification:'RUNTIME_UNHANDLED_FAULT',requestId:response.body.error.request_id,semanticOperationId:'route:POST:/temporal/find',sourceRevision:revision,attributes:{}});const safe=JSON.stringify({response,events:actualEvents});for(const term of ['canary_private','SELECT','UPDATE','temporal_record','calendar_date','stack','database.'])expect(safe).not.toContain(term);}
 }
}catch(error){failed={name:(error as Error).name,message:(error as Error).message};}
finally{
 delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
 try{if(!pg){await exec('PRAGMA ignore_check_constraints = OFF');expect((await exec('PRAGMA ignore_check_constraints'))[0].ignore_check_constraints).toBe(0);}if(baseline&&changed)await update(baseline[column]);if(precisionChanged){await exec('ALTER TABLE \"temporal_record\" ALTER COLUMN \"instant\" TYPE timestamptz(3)');expect(await precision()).toBe(3);precisionChanged=false;}if(baseline){expect(await physical()).toEqual(baseline);expect(await request('/temporal/find',{id})).toEqual({status:200,body:input});observations.push({phase:'restored',physical:await physical(),readiness:await ready(),response:await request('/temporal/find',{id})});}}catch(error){failed??={name:'cleanup',message:(error as Error).message};}
 if(inspector instanceof Database)inspector.close();else await inspector.close();server.stop(true);deployment.stop(true);console.error=originalError;
}
await Bun.write(Bun.stdout,JSON.stringify({case:c,backend:pg?'postgres':'sqlite',id,ports,ownedPostgresPids,observations,failed})+'\n');
process.exit(failed?1:0);
