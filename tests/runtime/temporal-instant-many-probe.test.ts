import {expect,test} from 'bun:test';
import {SQL} from 'bun';
import {copyFileSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';

const pg=Boolean(Bun.env.JADPO_INSTANT_MANY_DATABASE_URL), compiler=Bun.env.JADPO_BIN??resolve('jadpo/target/debug/jadpo');
const sha=(v:any)=>createHash('sha256').update(v).digest('hex');
if(Bun.env.JADPO_MANY_COMPILER_SHA)expect(sha(readFileSync(compiler))).toBe(Bun.env.JADPO_MANY_COMPILER_SHA);
const aligned=pg?'2026-10-25 00:30:00.123000+00':1792888200123, micro='2026-10-25 00:30:00.123456+00';
const cases:any[]=[
 {label:'three aligned visible rows',value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'negative aligned row in list',value:pg?'1969-12-31 23:59:59.999+00':-1,output:'1969-12-31T23:59:59.999Z'},
 {label:'portable year1',value:pg?'0001-01-01 00:00:00+00':-62135596800000,output:'0001-01-01T00:00:00.000Z'},
 {label:'portable year9999',value:pg?'9999-12-31 23:59:59.999+00':253402300799999,output:'9999-12-31T23:59:59.999Z'},
 {label:'nullable actual null',field:'optional_instant',value:null,output:null},
 {label:'last row aligned',index:2,value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'visible wrong native TEXT contained',type:'text',value:'private-invalid-instant',bypassCheck:!pg,fault:true},
 ...(!pg?[{label:'mixed hidden per-row TEXT stays absent',value:'private-invalid-instant',bypassCheck:true,mixed:true}]:[]),
 {label:'hidden-only wrong native TEXT stays empty',type:'text',value:'private-invalid-instant',bypassCheck:!pg,hiddenOnly:true},
 {label:'postread membership revocation precedes TEXT decode',type:'text',value:'private-invalid-instant',bypassCheck:!pg,revoke:true},
 {label:'literal descending limit2 offset1 selected rows',desc:true,limit:2,offset:1,value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'unbounded ordinary many',unbounded:true,value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'empty selected bucket',missing:true,value:aligned},
 {label:'causal omitted physical TEXT mutation',type:'text',value:'private-invalid-instant',bypassCheck:!pg,omit:true,fault:true,negative:true},
 {label:'causal single validated native decoder argument bypass',type:'text',value:'private-invalid-instant',bypassCheck:!pg,fault:true,decoderBypass:true,negative:true},
 ...(pg?[
  {label:'strict precision first visible row diagnostic',value:micro,fault:true,strictRed:true},
  {label:'strict precision last visible row diagnostic',index:2,value:micro,fault:true,strictRed:true},
  {label:'strict signed microsecond diagnostic',value:'1969-12-31 23:59:59.999999+00',fault:true,strictRed:true},
  {label:'nullable physical TEXT null strict kind diagnostic',field:'optional_instant',type:'text',value:null,fault:true,strictRed:true},
  {label:'mixed hidden microseconds',value:micro,mixed:true},
  {label:'hidden-only microseconds',value:micro,hiddenOnly:true},
  {label:'postread revocation microseconds remains empty',value:micro,revoke:true},
 ]:[{label:'visible integer range contradiction contained',value:253402300800000,fault:true}]),
];

for(const c of cases)test(`original native ${pg?'PG':'SQLite'} many: ${c.label}`,async()=>{
 const project=mkdtempSync(join(tmpdir(),'jadpo-many-instant-')),receipt=mkdtempSync(resolve('build/validation/rm504-instant-many.')),objectDir=resolve('build/validation/many-instant/objects');mkdirSync(objectDir,{recursive:true});const refs:any={};
 const store=(name:string,bytes:Buffer|string)=>{const data=Buffer.isBuffer(bytes)?bytes:Buffer.from(bytes),digest=sha(data);writeFileSync(join(objectDir,digest),data);refs[name]={sha256:digest,bytes:data.length};};
 try{
  const originalFixture=readFileSync(resolve('tests/runtime/fixtures/temporal-instant-sidecar.jadpo'),'utf8');
  const old='query find(input: TimedLookup) freshness: authoritative fails TimedMissing -> TimedRecord {\n  var record = attempt query required TimedRecord { where: bucket == input.bucket missing: TimedMissing }\n  return record\n }';expect(originalFixture.split(old)).toHaveLength(2);
  const pagination=c.unbounded?'':` limit: ${c.limit??10} offset: ${c.offset??0}`;
  const source=originalFixture.replace(old,`query find(input: TimedLookup) freshness: authoritative -> List<TimedRecord> { return attempt query many TimedRecord { where: bucket == input.bucket order_by: id ${c.desc?'desc':'asc'}${pagination} } }`).replace('output: TimedRecord run: TimedRecord.find','output: List<TimedRecord> run: TimedRecord.find');
  writeFileSync(join(project,'app.jadpo'),source);writeFileSync(join(receipt,'app.jadpo'),source);
  const build=Bun.spawnSync([compiler,'build',project],{stdout:'pipe',stderr:'pipe'});writeFileSync(join(receipt,'build.stdout'),build.stdout);writeFileSync(join(receipt,'build.stderr'),build.stderr);expect(build.exitCode).toBe(0);
  const emitted=join(project,'build'),originalFiles:any={};const capture=(directory:string)=>{for(const file of readdirSync(directory,{withFileTypes:true})){const path=join(directory,file.name);if(file.isDirectory())capture(path);else{const name=path.slice(emitted.length+1),raw=readFileSync(path);store('original-generated/'+name,raw);originalFiles[name]=sha(raw);}}};capture(emitted);
  const persistence=join(emitted,'target/persistence.ts'),appFile=join(emitted,'target/app.ts'),original=readFileSync(persistence,'utf8'),originalApp=readFileSync(appFile,'utf8');
  const name=`query_many_TimedRecord_by_bucket_order_by_id_${c.desc?'desc':'asc'}${c.unbounded?'':'_paginated'}`,start=original.indexOf('  async '+name+'('),end=original.indexOf('\n  },',start)+7;expect(start).toBeGreaterThan(0);const method=original.slice(start,end),sqls=[...method.matchAll(/"SELECT (?:\\.|[^"\\])*"/gu)];expect(sqls).toHaveLength(2);const originalPgSQL=JSON.parse(sqls[0][0]),originalSQLiteSQL=JSON.parse(sqls[1][0]);expect(originalPgSQL).not.toContain('EXTRACT');
  const readbackPgSQL=originalPgSQL.replace(' FROM "timed_record" ',', current_setting(\'TimeZone\') AS "jadpo_probe_timezone", current_setting(\'DateStyle\') AS "jadpo_probe_datestyle", pg_backend_pid() AS "jadpo_probe_backend_pid" FROM "timed_record" ');expect(readbackPgSQL).not.toBe(originalPgSQL);
  const filter='    return policyFilterRows("TimedRecord", rows, policy, postgres, sqlite);';expect(method.split(filter)).toHaveLength(2);
  const instrumentedMethod=method.replace(sqls[0][0],JSON.stringify(readbackPgSQL)).replace(filter,'    probeStage("native_before_strip", rows);\n    for(const row of rows) for(const key of ["jadpo_probe_timezone","jadpo_probe_datestyle","jadpo_probe_backend_pid"]) delete (row as any)[key];\n    probeStage("raw_before_policy", rows);\n    await probeAfterRead();\n    const visible = await policyFilterRows("TimedRecord", rows, policy, postgres, sqlite);\n    probeStage("visible_after_policy", visible);\n    return visible;');
  let changed=original.slice(0,start)+instrumentedMethod+original.slice(end);
  changed=changed.replace('import { SQL } from "bun";','import { SQL,stage as probeStage,afterRead as probeAfterRead } from "./many-observer.ts";').replace('import { Database } from "bun:sqlite";','import { Database } from "./many-observer.ts";');
  const declaration='function decodeDatabase_TimedRecord(value: unknown, path: string): unknown {';expect(originalApp.split(declaration)).toHaveLength(2);
  let changedApp='import {stage as probeStage,options as probeOptions} from "./many-observer.ts";\n'+originalApp.replace(declaration,declaration+'\n  probeStage("selected_decode",[value]);');
  if(c.decoderBypass){const from=changedApp.indexOf(declaration),to=changedApp.indexOf('\nfunction validate_TimedRecord(',from),body=changedApp.slice(from,to),field='decodeDatabaseInstant(object["instant"], `${path}.instant`)';expect(body.split(field)).toHaveLength(2);changedApp=changedApp.slice(0,from)+body.replace(field,'decodeDatabaseInstant(probeOptions().decoderBypass ? '+(pg?'new Date("2026-10-25T00:30:00.123Z")':'1792888200123')+' : object["instant"], `${path}.instant`)')+changedApp.slice(to);}
  writeFileSync(persistence,changed);writeFileSync(appFile,changedApp);copyFileSync(resolve('tests/runtime/fixtures/temporal-instant-many-observer.ts'),join(emitted,'target/many-observer.ts'));
  for(const [file,digest]of Object.entries(originalFiles))if(!['target/app.ts','target/persistence.ts'].includes(file))expect(sha(readFileSync(join(emitted,file)))).toBe(digest);
  store('instrumented-persistence.ts',changed);store('instrumented-app.ts',changedApp);store('observer.ts',readFileSync(join(emitted,'target/many-observer.ts')));store('host.ts',readFileSync(resolve('tests/runtime/fixtures/temporal-instant-many-host.ts')));writeFileSync(join(receipt,'artifact-references.json'),JSON.stringify(refs,null,2));
  writeFileSync(join(receipt,'instrumentation.json'),JSON.stringify({scope:'original native ordinary many helper/primitive decoder; explicit forwarding observer, selected native settings readback and identity stages; only named causal decoder argument bypass mutates decoding',compilerSHA:sha(readFileSync(compiler)),originalPgSQL,readbackPgSQL,originalSQLiteSQL,originalPersistenceSHA:sha(original),instrumentedPersistenceSHA:sha(changed),originalAppSHA:sha(originalApp),instrumentedAppSHA:sha(changedApp),mutant:c.decoderBypass?'one native decoder argument replaces corrupt field with valid fixture baseline value after visibility':null},null,2));
  const env={...process.env};for(const key of ['DATABASE_URL','SQLITE_PATH','JADPO_DEBUG_TARGET_STACKS'])delete env[key];const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null',resolve('tests/runtime/fixtures/temporal-instant-many-host.ts'),project,JSON.stringify(c)],{env,stdout:'pipe',stderr:'pipe'});const [stdout,stderr,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);writeFileSync(join(receipt,'native.stdout'),stdout);writeFileSync(join(receipt,'native.stderr'),stderr);expect(stdout.trim()).not.toBe('');const observation=JSON.parse(stdout.trim().split('\n').at(-1)!);writeFileSync(join(receipt,'observations.json'),JSON.stringify(observation,null,2));console.log(JSON.stringify({kind:'native_original_many_receipt',engine:pg?'postgres':'sqlite',receipt,case:c,exitCode:code}));
  expect(observation.observations.some((x:any)=>x.phase==='initialized')).toBe(true);expect(observation.observations.some((x:any)=>x.phase==='restored')).toBe(true);if(pg)expect(observation.observations.at(-1).phase).toBe('settings_restored');
  for(const port of Object.values(observation.ports)as number[]){let refused=false;try{await fetch(`http://127.0.0.1:${port}/health/live`,{signal:AbortSignal.timeout(1000)});}catch{refused=true;}expect(refused).toBe(true);}
  if(pg){const inspector=new SQL({url:Bun.env.JADPO_INSTANT_MANY_DATABASE_URL,prepare:false});try{expect(observation.ownedPostgresPids.length).toBeGreaterThan(0);expect(await inspector.unsafe(`SELECT pid FROM pg_stat_activity WHERE pid IN (${observation.ownedPostgresPids.map((_:any,i:number)=>'$'+(i+1)).join(',')})`,observation.ownedPostgresPids)).toHaveLength(0);}finally{await inspector.close();}}
  if(c.strictRed||c.negative){expect(code).toBe(1);expect(observation.failure.message).toContain('Expected: 500\nReceived: 200');const typed=observation.observations.find((x:any)=>x.phase==='typed_query');expect(typed.response.status).toBe(200);expect(typed.events).toEqual([]);expect(typed.faults).toEqual([]);if(c.strictRed){const physical=observation.observations.find((x:any)=>x.phase==='physical_mutation'),row=physical.before[c.index??0],native=typed.journal.trace.find((x:any)=>x.sql.includes('FROM \"timed_record\"')).rows.find((x:any)=>x.id===row.id);if(c.field==='optional_instant'){expect(row.optional_type).toBe('text');expect(native.optional_instant).toBeNull();}else{expect(row.instant_type).toBe('timestamp with time zone');expect(row.instant).toContain(c.value.match(/\.\d{4,6}/u)[0]);expect(native.instant.native).toBe('Date');expect(typed.response.body.find((x:any)=>x.id===row.id).instant).toBe(native.instant.iso);}}}else{expect(code).toBe(0);expect(observation.failure).toBeNull();}
 }finally{rmSync(project,{recursive:true,force:true});}
},20000);
