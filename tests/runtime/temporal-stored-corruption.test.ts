import {expect,test} from 'bun:test';
import {SQL} from 'bun';
import {copyFileSync,mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
const pg=Bun.env.JADPO_TEMPORAL_STORED_DATABASE_URL;
if(pg){const u=new URL(pg);if(!['postgres:','postgresql:'].includes(u.protocol)||!['127.0.0.1','localhost'].includes(u.hostname)||u.pathname!=='/jadpo_auth_test_runtime')throw Error('Disposable local stored-temporal database required');}
const compiler=Bun.env.JADPO_BIN??resolve('jadpo/target/debug/jadpo');
const source=readFileSync(resolve('tests/runtime/fixtures/temporal-roundtrip.jadpo'));
const cases:any[]=[
 {label:'unsafe positive Duration',field:'elapsed',value:'9007199254740992'},
 {label:'unsafe negative Duration',field:'elapsed',value:'-9007199254740992'},
 {label:'portable positive Duration boundary',field:'elapsed',value:'9007199254740991',positive:true},
 {label:'portable negative Duration boundary',field:'elapsed',value:'-9007199254740991',positive:true},
 {label:'Instant year10000',field:'instant',value:pg?'10000-01-01 00:00:00+00':253402300800000},
 {label:'impossible CalendarDate',field:'calendar_date',value:'2026-02-30',storageRefusal:!!pg},
 ...(pg?[{label:'CalendarDate year10000',field:'calendar_date',value:'10000-01-01'},{label:'diagnostic after-ready PostgreSQL six-digit precision',field:'instant',value:'2026-10-25 00:30:00.123456+00',mode:'precision-diagnostic'}]:[]),
 {label:'Time unknown zone',field:'resolved',value:JSON.stringify({instant:'2026-10-25T00:30:00.123Z',zone:'canary_private'})},
 {label:'Time extra field',field:'resolved',value:JSON.stringify({instant:'2026-10-25T00:30:00.123Z',zone:'Europe/London',canary_private:'private payload'})},
 {label:'Time zoneless instant',field:'resolved',value:JSON.stringify({instant:'2026-10-25T00:30:00.123',zone:'Europe/London'})},
 {label:'Time submillisecond instant',field:'resolved',value:JSON.stringify({instant:'2026-10-25T00:30:00.1234Z',zone:'Europe/London'})},
 {label:'Time missing instant',field:'resolved',value:JSON.stringify({zone:'Europe/London'})},
 {label:'causal omitted unsafe Duration mutation',field:'elapsed',value:'9007199254740992',omit:true,negative:true},
 {label:'causal isolated Duration decoder bypass',field:'elapsed',value:'9007199254740992',bypass:true,negative:true},
 ...(!pg?[{label:'stored Duration PT1S wire TEXT refused',field:'elapsed',value:'PT1S',mode:'kind-diagnostic',strict:true},{label:'stored Duration signed wire TEXT refused',field:'elapsed',value:'-PT1.234S',mode:'kind-diagnostic',strict:true},{label:'stored Duration overprecision wire TEXT refused',field:'elapsed',value:'PT1.0001S',mode:'kind-diagnostic',strict:true},{label:'native CHECK rejects Duration wire TEXT before storage',field:'elapsed',value:'PT1S',storageRefusal:true},{label:'causal old Duration fallback reintroduced',field:'elapsed',value:'PT1S',mode:'kind-diagnostic',strict:true,negative:true,reintroduceFallback:true}]:[]),
];
const env={...process.env};for(const k of ['DATABASE_URL','SQLITE_PATH','JADPO_DEBUG_TARGET_STACKS'])delete env[k];
for(const c of cases)test(`native stored temporal: ${c.label}`,async()=>{
 const root=mkdtempSync(join(tmpdir(),'jadpo-temporal-stored-')),receipt=mkdtempSync(resolve('build/validation/rm504-stored.'));mkdirSync(join(root,'application'));const project=join(root,'application');writeFileSync(join(project,'app.jadpo'),source);
 try{
 const built=Bun.spawnSync([compiler,'build',project],{stdout:'pipe',stderr:'pipe'});writeFileSync(join(receipt,'build.stdout'),built.stdout);writeFileSync(join(receipt,'build.stderr'),built.stderr);expect(built.exitCode).toBe(0);
 if(c.bypass){const f=join(project,'build/target/app.ts'),original=readFileSync(f,'utf8'),needle='elapsed: decodeDatabaseDuration(object["elapsed"], `${path}.elapsed`),';expect(original.split(needle)).toHaveLength(2);const changed=original.replace(needle,'elapsed: Number(object["elapsed"]) === 9007199254740992 ? "PT0S" : decodeDatabaseDuration(object["elapsed"], `${path}.elapsed`),');writeFileSync(join(receipt,'unaltered-app.ts'),original);writeFileSync(f,changed);writeFileSync(join(receipt,'decoder-bypass.json'),JSON.stringify({scope:'isolated actual negative only, never generated production correction',needle,originalSHA:createHash('sha256').update(original).digest('hex'),mutatedSHA:createHash('sha256').update(changed).digest('hex')},null,2));}
 if(c.reintroduceFallback){const f=join(project,'build/target/app.ts'),original=readFileSync(f,'utf8'),needle='return invalid(path, "stored Duration integer milliseconds");';expect(original.split(needle)).toHaveLength(2);const changed=original.replace(needle,'return validateDuration(value, path);');writeFileSync(join(receipt,'unaltered-app.ts'),original);writeFileSync(f,changed);writeFileSync(join(receipt,'old-fallback-reintroduction.json'),JSON.stringify({scope:'one private fallback restored in isolated negative only',needle,originalSHA:createHash('sha256').update(original).digest('hex'),mutatedSHA:createHash('sha256').update(changed).digest('hex')},null,2));}
 const pins:any={};for(const name of ['target/app.ts','target/persistence.ts','target/startup-owner.ts','sql/sqlite/schema.sql','sql/postgres/schema.sql','persistence/entities.json','approval/subject.json']){const b=readFileSync(join(project,'build',name));const dest=join(receipt,name);mkdirSync(resolve(dest,'..'),{recursive:true});writeFileSync(dest,b);pins[name]=createHash('sha256').update(b).digest('hex');}writeFileSync(join(receipt,'app.jadpo'),source);
 const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null',resolve('tests/runtime/fixtures/temporal-stored-host.ts'),project,JSON.stringify(c)],{env,stdout:'pipe',stderr:'pipe'});
 const [stdout,stderr,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);writeFileSync(join(receipt,'native.stdout'),stdout);writeFileSync(join(receipt,'native.stderr'),stderr);
 const observation=JSON.parse(stdout.trim().split('\n').at(-1)!);writeFileSync(join(receipt,'observations.json'),JSON.stringify(observation,null,2)+'\n');console.log(JSON.stringify({kind:'native_stored_temporal',case:c.label,backend:pg?'postgres':'sqlite',receipt,pins,observation,exitCode:code}));
 for(const port of Object.values(observation.ports) as number[]){expect(Number.isSafeInteger(port)).toBe(true);expect(port).toBeGreaterThan(0);let refused=false;try{await fetch(`http://127.0.0.1:${port}/health/live`,{signal:AbortSignal.timeout(1000)});}catch{refused=true;}expect(refused).toBe(true);}
 if(pg){expect(observation.ownedPostgresPids.length).toBeGreaterThan(0);const control=new SQL({url:pg,prepare:false});try{const pids=observation.ownedPostgresPids as number[];const rows=await control.unsafe(`SELECT pid FROM pg_stat_activity WHERE pid IN (${pids.map((_,i)=>'$'+(i+1)).join(',')})`,pids);expect(rows).toHaveLength(0);}finally{await control.close();}}
 if(c.negative){expect(code).toBe(1);expect(observation.failed).not.toBeNull();const query=observation.observations.find((r:any)=>r.phase==='typed_query');expect(query.response.status).toBe(200);expect(query.faults).toEqual([]);expect(query.events).toEqual([]);if(c.bypass)expect(query.response.body.elapsed).toBe('PT0S');else if(c.strict)expect(query.response.body.elapsed).toBe('PT1S');else expect(query.response.body.elapsed).toBe('-PT1.234S');}else{expect(code).toBe(0);expect(observation.failed).toBeNull();}expect(observation.observations.at(-1).phase).toBe('restored');
 }finally{rmSync(root,{recursive:true,force:true});}
},20000);
