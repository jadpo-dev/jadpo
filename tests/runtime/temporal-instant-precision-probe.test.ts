import {expect,test} from 'bun:test';
import {SQL} from 'bun';
import {copyFileSync,mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
const url=Bun.env.JADPO_INSTANT_PRECISION_DATABASE_URL;if(!url)throw Error('Explicit isolated PostgreSQL precision probe required');
const compiler=Bun.env.JADPO_BIN??'/private/tmp/jadpo-root-audit-host-ordinary-98-cli';const sha=(b:any)=>createHash('sha256').update(b).digest('hex');expect(sha(readFileSync(compiler))).toBe('47b12e3b59cbfb0fdb10a75ca850347e5f66c78afcaff003ea55368d0147c07d');
const helper=String.raw`
// Experimental tests-only point-query adapter. Not a compiler product mapping.
function probeStoredInstant(value: unknown, type: unknown, nullable: boolean): string | null {
 const fail=():never=>{throw new PersistenceFault("probe.decode.Instant", "data" as any, new Error("stored Instant contradicts checked precision/type/range"));};
 if(type!=="timestamp with time zone")return fail();
 if(value===null)return nullable?null:fail();
 if(typeof value!=="string")return fail();
 const match=/^(-?)([0-9]+)(?:\.([0-9]{1,6}))?$/u.exec(value);if(match===null)return fail();
 const magnitude=BigInt(match[2])*1000000n+BigInt((match[3]??"").padEnd(6,"0"));const micros=match[1]? -magnitude:magnitude;
 if (micros % 1000n !== 0n) return fail();
 const milliseconds=micros/1000n;if(milliseconds < -62135596800000n || milliseconds > 253402300799999n)return fail();
 return new Date(Number(milliseconds)).toISOString();
}
`;
const cases:any[]=[
 {label:'original native six-digit Date loss retained',value:'2026-10-25 00:30:00.123456+00',contains:'.123456',originalLossy:true,output:'2026-10-25T00:30:00.123Z'},
 {label:'strict original lossy Date prerequisite red',value:'2026-10-25 00:30:00.123456+00',contains:'.123456',expected:'fault',negative:true},
 {label:'positive six-digit refusal',value:'2026-10-25 00:30:00.123456+00',contains:'.123456',expected:'fault',lossless:true},
 {label:'positive one-microsecond remainder refusal',value:'2026-10-25 00:30:00.123001+00',contains:'.123001',expected:'fault',lossless:true},
 {label:'negative fractional epoch refusal',value:'1969-12-31 23:59:59.999999+00',contains:'.999999',expected:'fault',lossless:true},
 {label:'aligned typmod6 offset-equivalent row only',value:'2026-10-25 01:30:00.123000+01',output:'2026-10-25T00:30:00.123Z',lossless:true},
 {label:'signed negative millisecond aligned',value:'1969-12-31 23:59:59.999+00',output:'1969-12-31T23:59:59.999Z',lossless:true},
 {label:'portable minimum year1',value:'0001-01-01 00:00:00+00',output:'0001-01-01T00:00:00.000Z',lossless:true},
 {label:'portable maximum year9999',value:'9999-12-31 23:59:59.999+00',output:'9999-12-31T23:59:59.999Z',lossless:true},
 {label:'year10000 refused',value:'10000-01-01 00:00:00+00',contains:'10000',expected:'fault',lossless:true},
 {label:'year0 native BC refused',value:'0001-01-01 00:00:00+00 BC',contains:'BC',expected:'fault',lossless:true},
 {label:'native infinity refused',value:'infinity',contains:'infinity',expected:'fault',lossless:true},
 {label:'nullable aligned field',field:'optional_instant',value:'2026-10-25 00:30:00.123+00',output:'2026-10-25T00:30:00.123Z',lossless:true},
 {label:'nullable microsecond remainder refusal',field:'optional_instant',value:'2026-10-25 00:30:00.123456+00',contains:'.123456',expected:'fault',lossless:true},
 {label:'nullable null retains native type',field:'optional_instant',value:null,lossless:true},
 {label:'zoneless kind refuses rather than UTC guess',type:'timestamp(6) without time zone',value:'2026-10-25 00:30:00.123456',contains:'.123456',expected:'fault',lossless:true},
 {label:'date kind refuses rather than epoch guess',type:'date',value:'2026-10-25',contains:'2026-10-25',expected:'fault',lossless:true},
 {label:'null with zoneless native kind still refuses',field:'optional_instant',type:'timestamp(6) without time zone',value:null,expected:'fault',lossless:true},
 {label:'causal omitted external mutation',value:'2026-10-25 00:30:00.123456+00',contains:'.123456',omit:true,expected:'fault',lossless:true,negative:true},
 {label:'causal lossless-to-lossy remainder guard removal',value:'2026-10-25 00:30:00.123456+00',contains:'.123456',expected:'fault',lossless:true,negative:true,lossy:true},
];
const oldSql='SELECT "id", "instant", "calendar_date", "elapsed", "resolved", "optional_instant" FROM "temporal_record" WHERE "id" = $1 LIMIT 2';
const selected=['instant','optional_instant'];const newSql=oldSql.replace('"instant",','EXTRACT(EPOCH FROM "instant")::text AS "instant", pg_typeof("instant")::text AS "__jadpo_probe_instant_type",').replace('"optional_instant" FROM','EXTRACT(EPOCH FROM "optional_instant")::text AS "optional_instant", pg_typeof("optional_instant")::text AS "__jadpo_probe_optional_instant_type" FROM');
for(const c of cases)test(`native point Instant probe: ${c.label}`,async()=>{
 const root=mkdtempSync(join(tmpdir(),'jadpo-instant-precision-')),project=join(root,'application'),receipt=mkdtempSync(resolve('build/validation/rm504-instant-precision.'));mkdirSync(project);copyFileSync(resolve('tests/runtime/fixtures/temporal-roundtrip.jadpo'),join(project,'app.jadpo'));
 try{
 const build=Bun.spawnSync([compiler,'build',project],{stdout:'pipe',stderr:'pipe'});writeFileSync(join(receipt,'build.stdout'),build.stdout);writeFileSync(join(receipt,'build.stderr'),build.stderr);expect(build.exitCode).toBe(0);
 const file=join(project,'build/target/persistence.ts'),original=readFileSync(file,'utf8'),start=original.indexOf('  async query_optional_TemporalRecord_by_id('),end=original.indexOf('  async query_required_TemporalRecord_by_id(',start);expect(start).toBeGreaterThan(0);expect(end).toBeGreaterThan(start);const method=original.slice(start,end);
 let changed=original;if(c.lossless){expect(method.split(JSON.stringify(oldSql))).toHaveLength(2);let replaced=method.replace(JSON.stringify(oldSql),JSON.stringify(newSql));const needle='    const visible = await policyFilterRows("TemporalRecord", rows, policy, postgres, sqlite);';expect(replaced.split(needle)).toHaveLength(2);const conversion='    for(const value of rows){const row=value as Record<string,unknown>; for(const field of ["instant","optional_instant"]){row[field]=probeStoredInstant(row[field],row["__jadpo_probe_"+field+"_type"],field==="optional_instant");delete row["__jadpo_probe_"+field+"_type"];}}\n';replaced=replaced.replace(needle,conversion+needle);changed=original.slice(0,start)+replaced+original.slice(end)+helper;
 if(c.lossy){const guard='if (micros % 1000n !== 0n) return fail();';expect(changed.split(guard)).toHaveLength(2);changed=changed.replace(guard,'// causal omitted remainder refusal: original lossy behavior deliberately restored');}
 }
 const importNeedle='import { SQL } from "bun";';expect(changed.split(importNeedle)).toHaveLength(2);changed=changed.replace(importNeedle,'import { SQL } from "./instant-precision-observer.ts";');copyFileSync(resolve('tests/runtime/fixtures/temporal-instant-precision-observer.ts'),join(project,'build/target/instant-precision-observer.ts'));writeFileSync(file,changed);writeFileSync(join(receipt,'original-persistence.ts'),original);writeFileSync(join(receipt,'instrumented-persistence.ts'),changed);writeFileSync(join(receipt,'mutation.json'),JSON.stringify({scope:'isolated generated adapter copy, no production mapping',sourceMethod:'query_optional_TemporalRecord_by_id',originalSHA:sha(original),instrumentedSHA:sha(changed),oldSql,newSql:c.lossless?newSql:oldSql,privateAliases:selected.map(s=>'__jadpo_probe_'+s+'_type'),lossyGuardOmitted:!!c.lossy},null,2));
 const pins:any={};for(const name of ['target/app.ts','target/persistence.ts','target/startup-owner.ts','sql/sqlite/schema.sql','sql/postgres/schema.sql','persistence/entities.json','approval/subject.json']){const b=readFileSync(join(project,'build',name)),dest=join(receipt,name);mkdirSync(resolve(dest,'..'),{recursive:true});writeFileSync(dest,b);pins[name]=sha(b);}copyFileSync(join(project,'app.jadpo'),join(receipt,'app.jadpo'));
 const env={...process.env};for(const k of ['DATABASE_URL','SQLITE_PATH','JADPO_DEBUG_TARGET_STACKS'])delete env[k];const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null',resolve('tests/runtime/fixtures/temporal-instant-precision-host.ts'),project,JSON.stringify({...c,expectedSql:(c.lossless?newSql:oldSql).replace(' WHERE "id" = $1 LIMIT 2',' WHERE "id" = $1 AND (1 = 1) LIMIT 2')})],{env,stdout:'pipe',stderr:'pipe'});const [stdout,stderr,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);writeFileSync(join(receipt,'native.stdout'),stdout);writeFileSync(join(receipt,'native.stderr'),stderr);const observation=JSON.parse(stdout.trim().split('\n').at(-1)!);writeFileSync(join(receipt,'observations.json'),JSON.stringify(observation,null,2)+'\n');console.log(JSON.stringify({kind:'point_instant_precision_receipt',receipt,case:c,pins,exitCode:code,observation}));
 expect(observation.observations.at(-1).phase).toBe('restored');for(const port of Object.values(observation.ports) as number[]){let refused=false;try{await fetch(`http://127.0.0.1:${port}/health/live`,{signal:AbortSignal.timeout(1000)});}catch{refused=true;}expect(refused).toBe(true);}const inspector=new SQL({url,prepare:false});try{const pids=observation.ownedPostgresPids;expect(pids.length).toBeGreaterThan(0);expect(await inspector.unsafe(`SELECT pid FROM pg_stat_activity WHERE pid IN (${pids.map((_:any,i:number)=>'$'+(i+1)).join(',')})`,pids)).toHaveLength(0);}finally{await inspector.close();}
 if(c.negative){expect(code).toBe(1);expect(observation.failed).not.toBeNull();expect(observation.failed.message).toContain('Expected: 500\nReceived: 200');const q=observation.observations.find((r:any)=>r.phase==='typed_query');expect(q.response.status).toBe(200);expect(q.faults).toEqual([]);expect(q.events).toEqual([]);}else{expect(code).toBe(0);expect(observation.failed).toBeNull();}
 }finally{rmSync(root,{recursive:true,force:true});}
},20000);
