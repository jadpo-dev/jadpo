import {expect,test} from 'bun:test';
import {SQL} from 'bun';
import {copyFileSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';import {resolve,join} from 'node:path';import {tmpdir} from 'node:os';
const compiler=Bun.env.JADPO_BIN??'/private/tmp/jadpo-instant-many-product/build/validation/many-product/candidate-final-cli',pg=Boolean(Bun.env.JADPO_INSTANT_PAGE_DATABASE_URL),sha=(v:any)=>createHash('sha256').update(v).digest('hex');expect(sha(readFileSync(compiler))).toBe('160fc178defab1abefc27190738e864bafecaa9a907b2ffa9a1081dfecc89555');
const aligned=pg?'2026-10-25 00:30:00.123000+00':1792888200123,micro='2026-10-25 00:30:00.123456+00',range=pg?'10000-01-01 00:00:00+00':253402300800000;
const cases:any[]=[
 {label:'aligned three returned items',value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'limit2 returned selected cursor',limit:2,value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'continuation returns remaining item',limit:2,afterContinuation:true,value:aligned},
 {label:'reverse ties and selected cursor',desc:true,limit:2,value:aligned,output:'2026-10-25T00:30:00.123Z'},
 {label:'nullable actual null',field:'optional_instant',value:null,output:null},
 {label:'visible required range contradiction contained',value:range,fault:true},
 {label:'unreturned lookahead range corruption stays untrusted',index:2,limit:2,value:range},
 {label:'causal eager lookahead uses genuinely rejected range',index:2,limit:2,value:range,eager:true,negative:true,negativeExpected:'Expected: 200\nReceived: 500'},
 {label:'causal omitted genuine range mutation',value:range,fault:true,omit:true,negative:true},
 {label:'causal isolated successful projected field bypass',value:range,fault:true,bypass:true,negative:true},
 {label:'visible native TEXT contained',type:'text',value:'private-invalid-instant',bypassCheck:!pg,fault:true},
 {label:'hidden SQL scoped TEXT remains absent',type:'text',value:'private-invalid-instant',bypassCheck:!pg,hiddenOnly:true},
 {label:'mixed hidden range row excluded before limit',value:range,mixed:true},
 {label:'postread native membership deletion original SQL snapshot remains',value:aligned,revoke:true,output:'2026-10-25T00:30:00.123Z'},
 {label:'postread deletion does not invent field re-cut',value:range,revoke:true,fault:true},
 {label:'nonprojected noncursor Instant remains unused',ignoreInstant:true,value:range},
 {label:'projection-only Instant contained independently',projectionOnly:true,value:range,fault:true},
 {label:'empty matching bucket',missing:true,value:aligned},
 {label:'portable negative aligned',value:pg?'1969-12-31 23:59:59.999+00':-1,output:'1969-12-31T23:59:59.999Z'},
 {label:'portable native year1',value:pg?'0001-01-01 00:00:00+00':-62135596800000,output:'0001-01-01T00:00:00.000Z'},
 {label:'portable native year9999',value:pg?'9999-12-31 23:59:59.999+00':253402300799999,output:'9999-12-31T23:59:59.999Z'},
 ...(pg?[
  {label:'strict first selected projected microseconds',desc:true,value:micro,fault:true,strictRed:true},
  {label:'strict last selected projected microseconds',value:micro,fault:true,strictRed:true},
  {label:'strict signed selected microseconds',value:'1969-12-31 23:59:59.999999+00',fault:true,strictRed:true},
  {label:'strict cursor-only selected microseconds',cursorOnly:true,index:1,desc:true,limit:1,value:micro,fault:true,strictRed:true},
  {label:'strict nullable TEXT NULL kind',field:'optional_instant',type:'text',value:null,fault:true,strictRed:true},
  {label:'microseconds in unreturned extra row not decoded',index:2,limit:2,value:micro},
  {label:'mixed hidden microseconds stay absent',value:micro,mixed:true},
  {label:'hidden SQL microseconds remain absent',value:micro,hiddenOnly:true},
  {label:'native historical offset seconds literal',value:'1890-01-01 00:00:00+00',zone:'Europe/Berlin',output:'1890-01-01T00:00:00.000Z',literalDiagnostic:true},
  {label:'native offset rendered year10000 literal',value:'9999-12-31 23:59:59.999+00',zone:'Europe/Berlin',output:'9999-12-31T23:59:59.999Z',literalDiagnostic:true},
  {label:'native offset rendered BC literal',value:'0001-01-01 00:00:00+00',zone:'America/New_York',output:'0001-01-01T00:00:00.000Z',literalDiagnostic:true},
  {label:'selected connection alternate DateStyle literal',value:aligned,zone:'Europe/Amsterdam',style:'German, DMY',output:'2026-10-25T00:30:00.123Z',literalDiagnostic:true},
 ]:[])
];
for(const c of cases)test(`original native ${pg?'PG':'SQLite'} page: ${c.label}`,async()=>{
 const project=mkdtempSync(join(tmpdir(),'jadpo-instant-page-')),receipt=mkdtempSync(resolve('build/validation/rm504-instant-page.')),objects=resolve('build/validation/page-probe/objects');mkdirSync(objects,{recursive:true});const refs:any={};const store=(n:string,d:any)=>{const raw=Buffer.isBuffer(d)?d:Buffer.from(d),h=sha(raw);writeFileSync(join(objects,h),raw);refs[n]={sha256:h,bytes:raw.length};};
 try{
  let source=readFileSync(resolve('tests/runtime/fixtures/temporal-instant-sidecar.jadpo'),'utf8');
  const old='query find(input: TimedLookup) freshness: authoritative fails TimedMissing -> TimedRecord {\n  var record = attempt query required TimedRecord { where: bucket == input.bucket missing: TimedMissing }\n  return record\n }';expect(source.split(old)).toHaveLength(2);
  const fields=c.ignoreInstant||c.projectionOnly?'id':'instant, id',cursor=c.ignoreInstant||c.projectionOnly?'id':'instant, id',direction=c.desc?'desc':'asc';
  source=source.replace('input TimedLookup { bucket: TimedRecord.bucket }','input TimedLookup { bucket: TimedRecord.bucket after: TimedCursor optional page_size: Int }\nvalue TimedCursor { '+(c.ignoreInstant||c.projectionOnly?'':'instant: TimedRecord.instant ')+'id: TimedRecord.id }\noutput TimedView { id: TimedRecord.id scope_id: TimedRecord.scope_id bucket: TimedRecord.bucket '+(c.ignoreInstant||c.cursorOnly?'':'instant: TimedRecord.instant ')+'optional_instant: TimedRecord.optional_instant jadpo_probe_carrier_instant: Text }\noutput TimedPage { items: List<TimedView> next: TimedCursor? }');
  source=source.replace(old,`query find(input: TimedLookup) freshness: authoritative -> TimedPage { return attempt query page TimedRecord -> TimedPage { where: bucket == input.bucket order_by: ${fields.split(', ').map(f=>f+' '+direction).join(', ')} after: optional input.after limit: input.page_size project: TimedView cursor: TimedCursor(${cursor}) } }`).replace('output: TimedRecord run: TimedRecord.find','output: TimedPage run: TimedRecord.find');
  writeFileSync(join(project,'app.jadpo'),source);writeFileSync(join(receipt,'app.jadpo'),source);
  const built=Bun.spawnSync([compiler,'build',project],{stdout:'pipe',stderr:'pipe'});writeFileSync(join(receipt,'build.stdout'),built.stdout);writeFileSync(join(receipt,'build.stderr'),built.stderr);expect(built.exitCode).toBe(0);
  const emitted=join(project,'build'),pins:any={};const capture=(dir:string)=>{for(const item of readdirSync(dir,{withFileTypes:true})){const p=join(dir,item.name);if(item.isDirectory())capture(p);else{const n=p.slice(emitted.length+1),d=readFileSync(p);store('original-generated/'+n,d);pins[n]=sha(d);}}};capture(emitted);
  const appFile=join(emitted,'target/app.ts'),persistenceFile=join(emitted,'target/persistence.ts'),app=readFileSync(appFile,'utf8'),persistence=readFileSync(persistenceFile,'utf8');
  expect(app).not.toContain('decodePointInstant'); // page-only checked fixture stays outside existing entity opt-ins.
  const sqls=[...app.matchAll(/"SELECT (?:\\.|[^"\\])*"/gu)];expect(sqls).toHaveLength(4);let changedApp='import {stage as probeStage,options as probeOptions} from "./page-observer.ts";\n'+app;
  const originalPgSQL=JSON.parse(sqls[0][0]),originalAfterPgSQL=JSON.parse(sqls[2][0]);for(const i of [0,2]){const sql=JSON.parse(sqls[i][0]),withSettings=sql.replace(' FROM "timed_record" ',', current_setting(\'TimeZone\') AS "jadpo_probe_timezone", current_setting(\'DateStyle\') AS "jadpo_probe_datestyle", pg_backend_pid() AS "jadpo_probe_backend_pid" FROM "timed_record" ');expect(withSettings).not.toBe(sql);changedApp=changedApp.replace(sqls[i][0],JSON.stringify(withSettings));}
  const selected='const row = expectObject(value, `database.TimedPage.items[${index}]`);';expect(changedApp.split(selected)).toHaveLength(2);changedApp=changedApp.replace(selected,selected+' probeStage("selected_decode",[row]);');
  const next='const next = hasNext && last !== undefined ? validate_TimedCursor';expect(changedApp.split(next)).toHaveLength(2);changedApp=changedApp.replace(next,'if(hasNext && last !== undefined)probeStage("selected_cursor_decode",[last]); '+next);
  if(c.eager){const selectedRows='const selected = pageRows.slice(0, pageSize);';expect(changedApp.split(selectedRows)).toHaveLength(2);changedApp=changedApp.replace(selectedRows,selectedRows+' if(probeOptions().eager)for(const extra of pageRows.slice(pageSize))decodeDatabaseInstant(expectObject(extra,"database.TimedPage.extra")["instant"],"database.TimedPage.extra.instant");');}
  if(c.bypass){const needle='decodeDatabaseInstant(row["instant"], `database.TimedPage.items[${index}].instant`)';expect(changedApp.split(needle)).toHaveLength(2);changedApp=changedApp.replace(needle,'decodeDatabaseInstant(probeOptions().bypass ? '+(pg?'new Date("2026-10-25T00:30:00.123Z")':'1792888200123')+' : row["instant"], `database.TimedPage.items[${index}].instant`)');}
  const returned='    return policyReadRows(operation, entity, hasCursor ? postgresAfterSql : postgresFirstSql, hasCursor ? sqliteAfterSql : sqliteFirstSql, postgresValues, sqliteValues);';expect(persistence.split(returned)).toHaveLength(2);
  const changedPersistence=persistence.replace('import { SQL } from "bun";','import {SQL,stage as probeStage,afterRead as probeAfterRead} from "./page-observer.ts";').replace('import { Database } from "bun:sqlite";','import {Database} from "./page-observer.ts";').replace(returned,'    const rows = await policyReadRows(operation, entity, hasCursor ? postgresAfterSql : postgresFirstSql, hasCursor ? sqliteAfterSql : sqliteFirstSql, postgresValues, sqliteValues);\n    probeStage("native_before_strip",rows);\n    for(const row of rows)for(const key of ["jadpo_probe_timezone","jadpo_probe_datestyle","jadpo_probe_backend_pid"])delete (row as any)[key];\n    probeStage("raw_after_native",rows);\n    await probeAfterRead();\n    return rows;');
  writeFileSync(appFile,changedApp);writeFileSync(persistenceFile,changedPersistence);copyFileSync(resolve('tests/runtime/fixtures/temporal-instant-page-observer.ts'),join(emitted,'target/page-observer.ts'));
  // Both import paths must refer to the same observer singleton; host loads page-observer.
  for(const [n,h]of Object.entries(pins))if(!['target/app.ts','target/persistence.ts'].includes(n))expect(sha(readFileSync(join(emitted,n)))).toBe(h);
  store('instrumented-app.ts',changedApp);store('instrumented-persistence.ts',changedPersistence);store('observer.ts',readFileSync(resolve('tests/runtime/fixtures/temporal-instant-page-observer.ts')));store('host.ts',readFileSync(resolve('tests/runtime/fixtures/temporal-instant-page-host.ts')));writeFileSync(join(receipt,'artifact-references.json'),JSON.stringify(refs,null,2));writeFileSync(join(receipt,'instrumentation.json'),JSON.stringify({scope:'original generated page only; forwarding SQL/settings/object identity; no addedcarrier or policyFilterRows; named eager/bypass mutations only',compilerSHA:sha(readFileSync(compiler)),originalPgSQL,originalAfterPgSQL,originalAppSHA:sha(app),instrumentedAppSHA:sha(changedApp),originalPersistenceSHA:sha(persistence),instrumentedPersistenceSHA:sha(changedPersistence),eager:Boolean(c.eager),bypass:Boolean(c.bypass)},null,2));
  const env={...process.env};for(const n of ['DATABASE_URL','SQLITE_PATH','JADPO_DEBUG_TARGET_STACKS'])delete env[n];const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null',resolve('tests/runtime/fixtures/temporal-instant-page-host.ts'),project,JSON.stringify(c)],{env,stdout:'pipe',stderr:'pipe'});const [stdout,stderr,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);writeFileSync(join(receipt,'native.stdout'),stdout);writeFileSync(join(receipt,'native.stderr'),stderr);expect(stdout.trim()).not.toBe('');const ob=JSON.parse(stdout.trim().split('\n').at(-1)!);writeFileSync(join(receipt,'observations.json'),JSON.stringify(ob,null,2));console.log(JSON.stringify({kind:'original_page_instant_receipt',engine:pg?'postgres':'sqlite',receipt,case:c,exitCode:code}));
  expect(ob.observations.some((o:any)=>o.phase==='initialized')).toBe(true);expect(ob.observations.some((o:any)=>o.phase==='restored')).toBe(true);for(const port of Object.values(ob.ports)as number[]){let refused=false;try{await fetch(`http://127.0.0.1:${port}/health/live`,{signal:AbortSignal.timeout(1000)});}catch{refused=true;}expect(refused).toBe(true);}
  if(pg){expect(ob.observations.at(-1).phase).toBe('settings_restored');const db=new SQL({url:Bun.env.JADPO_INSTANT_PAGE_DATABASE_URL,prepare:false});try{expect(ob.ownedPostgresPids.length).toBeGreaterThan(0);expect(await db.unsafe(`SELECT pid FROM pg_stat_activity WHERE pid IN (${ob.ownedPostgresPids.map((_:any,i:number)=>'$'+(i+1)).join(',')})`,ob.ownedPostgresPids)).toHaveLength(0);}finally{await db.close();}}
  if(c.strictRed||c.negative){expect(code).toBe(1);expect(ob.failure.message).toContain(c.negativeExpected??'Expected: 500\nReceived: 200');const typed=ob.observations.find((o:any)=>o.phase==='typed_query');expect(typed.response.status).toBe(c.eager?500:200);if(!c.eager){expect(typed.faults).toEqual([]);expect(typed.events).toEqual([]);}}
  else if(c.literalDiagnostic&&code!==0){expect(ob.failure.message).toContain('Expected: 200\nReceived: 500');}
  else{expect(code).toBe(0);expect(ob.failure).toBeNull();}
 }finally{rmSync(project,{recursive:true,force:true});}
},20000);
