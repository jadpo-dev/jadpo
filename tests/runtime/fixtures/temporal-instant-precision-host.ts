import {expect} from 'bun:test';
import {SQL} from 'bun';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const [project,raw]=Bun.argv.slice(2),c=JSON.parse(raw),url=Bun.env.JADPO_INSTANT_PRECISION_DATABASE_URL!;
const {beginObservation,endObservation,nativeForm}=await import(pathToFileURL(join(project,"build/target/instant-precision-observer.ts")).href);
const u=new URL(url);if(!['postgres:','postgresql:'].includes(u.protocol)||u.hostname!=='127.0.0.1'||u.pathname!=='/jadpo_auth_test_runtime')throw Error('Disposable local precision database required');
Bun.env.DATABASE_URL=url;delete Bun.env.SQLITE_PATH;delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
const events:any[]=[],faults:any[]=[],obs:any[]=[];const oldError=console.error;
console.error=(...args:any[])=>{for(const arg of args)if(arg instanceof Error)faults.push({name:arg.name,constructorName:arg.constructor.name,kind:(arg as any).kind,operation:(arg as any).operation});else if(typeof arg==='string'){try{const v=JSON.parse(arg);if(v.kind==='operational_log_event')events.push(v);}catch{oldError(arg);}}};
const app=await import(pathToFileURL(join(project,'build/target/app.ts')).href),store=await import(pathToFileURL(join(project,'build/target/persistence.ts')).href);expect(store.isPersistenceReady()).toBe(true);
const inspector=new SQL({url,prepare:false});const exec=(s:string,p:any[]=[])=>inspector.unsafe(s,p);
const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch:app.handleRequest}),deployment=app.createDeploymentServer(0),ports={application:server.port,deployment:deployment.port};
const request=async(path:string,body:any)=>{const r=await fetch(`http://127.0.0.1:${server.port}${path}`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(5000)});return{status:r.status,body:await r.json()};};
const ready=async()=>{const r=await fetch(`http://127.0.0.1:${deployment.port}/health/ready`);expect(r.status).toBe(200);return{status:r.status,body:await r.json()};};
const id=crypto.randomUUID(),input={id,instant:'2026-10-25T00:30:00.123Z',calendar_date:'2024-02-29',elapsed:'-PT1.234S',resolved:{instant:'2026-10-25T00:30:00.123Z',zone:'Europe/London'},optional_instant:null};
const field=c.field??'instant';expect(['instant','optional_instant']).toContain(field);
const physical=async()=>{const rows=await exec(`SELECT "id", "instant"::text AS "instant", pg_typeof("instant")::text AS "instant_type", "optional_instant"::text AS "optional_instant", pg_typeof("optional_instant")::text AS "optional_instant_type", "calendar_date"::text AS "calendar_date", "elapsed"::text AS "elapsed", "resolved" FROM "temporal_record" ORDER BY "id"`);return nativeForm(rows);};
const column=async()=>nativeForm(await exec("SELECT a.attname AS name,a.atttypid::integer AS oid,a.atttypmod AS precision,a.attnotnull AS required FROM pg_catalog.pg_attribute a WHERE a.attrelid='temporal_record'::regclass AND a.attname=$1",[field]));
let baseline:any=null,failed:any=null,changed=false,pids:number[]=[];
try{
 obs.push({phase:'initialized',readiness:await ready(),versions:{bun:Bun.version,icu:process.versions.icu,tz:process.versions.tz??null,postgres:(await exec('SHOW server_version'))[0].server_version}});
 expect(await request('/temporal/create',input)).toEqual({status:200,body:input});expect(await request('/temporal/find',{id})).toEqual({status:200,body:input});baseline=await physical();expect(baseline).toHaveLength(1);const baselineColumn=await column();obs.push({phase:'baseline',physical:baseline,column:baselineColumn,response:await request('/temporal/find',{id})});
 pids=(await exec('SELECT pid FROM pg_stat_activity WHERE datname=current_database()')).map((r:any)=>r.pid);
 if(!c.omit){const type=c.type??'timestamptz(6)';expect(['timestamptz(6)','timestamp(6) without time zone','date']).toContain(type);await exec(`ALTER TABLE "temporal_record" ALTER COLUMN "${field}" TYPE ${type} USING "${field}"::${type}`);changed=true;const cast=c.type==='date'?'date':c.type?'timestamp without time zone':'timestamptz';await exec(`UPDATE "temporal_record" SET "${field}"=$1::${cast} WHERE "id"=$2`,[c.value,id]);}
 const before=await physical(),actualColumn=await column();expect(before).toHaveLength(1);expect(before[0].id).toBe(id);if(!c.omit){expect(actualColumn[0].oid).toBe(c.type==='date'?1082:c.type?1114:1184);expect(actualColumn[0].precision).toBe(c.type==='date'?-1:6);if(c.value===null)expect(before[0][field]).toBeNull();else if(c.contains)expect(before[0][field]).toContain(c.contains);}obs.push({phase:'physical_mutation',physical:before,column:actualColumn,fixtureMutation:!c.omit,value:c.value});
 const ei=events.length,fi=faults.length,trace=beginObservation();Bun.env.JADPO_DEBUG_TARGET_STACKS='1';let response:any;try{response=await request('/temporal/find',{id});}finally{delete Bun.env.JADPO_DEBUG_TARGET_STACKS;endObservation();}
 const actualEvents=events.slice(ei),actualFaults=faults.slice(fi);obs.push({phase:'typed_query',response,trace,events:actualEvents,faults:actualFaults});
 expect(trace).toHaveLength(1);expect(trace[0].sql).toMatch(/^SELECT /u);expect(trace[0].sql).toBe(c.expectedSql);expect(trace[0].parameters).toEqual([id]);expect(await physical()).toEqual(before);expect(await ready()).toMatchObject({status:200});
 if(c.expected==='fault'){
  expect(response.status).toBe(500);expect(Object.keys(response.body)).toEqual(['error']);expect(Object.keys(response.body.error).sort()).toEqual(['code','message','request_id']);expect(response.body.error).toMatchObject({code:'internal_fault',message:'An internal error occurred.'});expect(response.body.error.request_id).toMatch(/^req_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/u);
  expect(actualFaults).toEqual([{name:'PersistenceFault',constructorName:'PersistenceFault',kind:'data',operation:'probe.decode.Instant'}]);expect(actualEvents).toHaveLength(1);expect(actualEvents[0]).toMatchObject({requestId:response.body.error.request_id,eventName:'operation.failed',classification:'RUNTIME_UNHANDLED_FAULT'});expect(JSON.stringify(response.body)+JSON.stringify(actualEvents)).not.toContain('__jadpo_probe');if(c.contains)expect(JSON.stringify(response.body)+JSON.stringify(actualEvents)).not.toContain(c.contains);
 }else{expect(response.status).toBe(200);expect(actualFaults).toEqual([]);expect(actualEvents).toEqual([]);expect(Object.keys(response.body).sort()).toEqual(Object.keys(input).sort());expect(response.body[field]).toBe(c.output??input[field]);if(c.originalLossy){expect(trace[0].rows[0][field]).toEqual({native:'Date',iso:c.output});}if(c.lossless){expect(trace[0].rows[0]).toHaveProperty('__jadpo_probe_'+field+'_type','timestamp with time zone');expect(JSON.stringify(response.body)).not.toContain('__jadpo_probe');}}
}catch(error){failed={name:(error as any).name,message:(error as any).message};}
finally{
 try{if(baseline){if(changed){await exec(`ALTER TABLE "temporal_record" ALTER COLUMN "${field}" TYPE timestamptz(3) USING "${field}"::timestamptz`);await exec(`UPDATE "temporal_record" SET "${field}"=$1::timestamptz WHERE "id"=$2`,[baseline[0][field],id]);}expect((await column())[0]).toMatchObject({oid:1184,precision:3});expect(await physical()).toEqual(baseline);expect(await request('/temporal/find',{id})).toEqual({status:200,body:input});obs.push({phase:'restored',physical:await physical(),column:await column(),readiness:await ready(),response:await request('/temporal/find',{id})});}}
 catch(error){failed??={name:'cleanup',message:(error as any).message};}
 if(baseline)await exec('DELETE FROM \"temporal_record\" WHERE \"id\"=$1',[id]);server.stop(true);deployment.stop(true);await inspector.close();console.error=oldError;
}
await Bun.write(Bun.stdout,JSON.stringify({kind:'native_point_instant_precision',case:c,observations:obs,failed,ports,ownedPostgresPids:pids,sourceRevision:readFileSync(join(project,'build/target/app.ts'),'utf8').match(/const sourceRevision = "([^"]+)";/u)?.[1]})+'\n');process.exit(failed?1:0);
