import { afterAll, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Generated day_bounds API feeds an actual checked typed handler. No authored
// temporal intrinsic, main-entry, zone-rule byte attestation or auth claim.
const pg = Bun.env.JADPO_TEMPORAL_DAY_DATABASE_URL;
if (pg) {
  const u=new URL(pg);
  if(!["postgres:","postgresql:"].includes(u.protocol)||!["127.0.0.1","localhost"].includes(u.hostname)||!/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(u.pathname))throw Error("Disposable local day-page database required");
}
const root=mkdtempSync(join(tmpdir(),"jadpo-day-page-")), project=join(root,"app");mkdirSync(project);
copyFileSync(resolve("tests/runtime/fixtures/temporal-day-page.jadpo"),join(project,"app.jadpo"));
const compiler=Bun.env.JADPO_BIN??resolve("jadpo/target/debug/jadpo");
const built=Bun.spawnSync([compiler,"build",project],{stdout:"pipe",stderr:"pipe"});
if(built.exitCode){rmSync(root,{recursive:true,force:true});throw Error(`Checked day page build refused: ${built.stdout}\n${built.stderr}`);}
const receiptBase=resolve("build/validation");mkdirSync(receiptBase,{recursive:true});
const receipt=mkdtempSync(join(receiptBase,"rm504-day-page-runtime.")), pins:Record<string,string>={};
for(const name of ["target/app.ts","target/persistence.ts","target/startup-owner.ts","sql/sqlite/schema.sql","sql/postgres/schema.sql","persistence/entities.json","approval/subject.json"]){
 const from=join(project,"build",name),to=join(receipt,name);mkdirSync(resolve(to,".."),{recursive:true});copyFileSync(from,to);pins[name]=createHash("sha256").update(readFileSync(from)).digest("hex");
}
const old=Object.fromEntries(["DATABASE_URL","SQLITE_PATH"].map(n=>[n,Bun.env[n]]));
if(pg){Bun.env.DATABASE_URL=pg;delete Bun.env.SQLITE_PATH;}else{delete Bun.env.DATABASE_URL;Bun.env.SQLITE_PATH=join(root,"day.sqlite");}
const journal:Array<{sql:string,bindings:unknown[]}>=[],proto=Database.prototype as any,original=proto.prepare;
if(!pg)proto.prepare=function(sql:string,...options:unknown[]){
 const statement=original.call(this,sql,...options);
 return new Proxy(statement,{get(target,property){const member=Reflect.get(target,property,target);
  if(["all","get","run","values"].includes(String(property)))return(...values:unknown[])=>{journal.push({sql,bindings:values});return member.apply(target,values);};
  return typeof member==="function"?member.bind(target):member;
 }});
};
const app=await import(pathToFileURL(join(project,"build/target/app.ts")).href);
const observed:Record<string,unknown>[]=[];
afterAll(()=>{
 proto.prepare=original;
 const pgLog=Bun.env.JADPO_TEMPORAL_DAY_POSTGRES_LOG;
 if(pgLog){const bytes=readFileSync(pgLog);writeFileSync(join(receipt,"actual-postgres-statements.log"),bytes);}
 writeFileSync(join(receipt,"observations.json"),JSON.stringify({backend:pg?"postgres":"sqlite",observed,journal,generatedPins:pins,temporalProvenance:app.temporalProvenance},null,2)+"\n");
 for(const[n,v]of Object.entries(old)){if(v===undefined)delete Bun.env[n];else Bun.env[n]=v;}
 rmSync(root,{recursive:true,force:true});
 console.log(JSON.stringify({kind:"native_checked_half_open_day_pages",backend:pg?"postgres":"sqlite",receipt,observed,generatedPins:pins,temporalProvenance:app.temporalProvenance}));
});
async function request(path:string,value:unknown){const r=await app.handleRequest(new Request("http://day.test"+path,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(value)}));const body=await r.json();expect(r.status,JSON.stringify(body)).toBe(200);return body;}
const shift=(instant:string,ms:number)=>app.temporal.add_elapsed(instant,ms<0?`-PT${Math.abs(ms)/1000}S`:`PT${ms/1000}S`);
let serial=1;
function uuid(){return `00000000-0000-4000-8000-${String(serial++).padStart(12,"0")}`;}
describe("generated half-open Instant day-page predicates",()=>{
 for(const[date,hours]of[["2026-03-29",23],["2026-02-15",24],["2026-10-25",25]]as const){
  test(`London ${hours}-hour day preserves boundaries, ties and cursor repeat`,async()=>{
   const bounds=app.temporal.day_bounds(date,"Europe/London");
   expect(app.temporal.between(bounds.start,bounds.end)).toBe(`PT${hours}H`);
   const bucket=`day-${date}`,values=[shift(bounds.start,-1),bounds.start,bounds.start,shift(bounds.end,-1),bounds.end,shift(bounds.end,1)];
   const rows=[];
   for(const occurs_at of values){const v={id:uuid(),bucket,occurs_at};expect(await request("/day/create",v)).toEqual(v);rows.push(v);}
   const other={id:uuid(),bucket:`other-${date}`,occurs_at:bounds.start};expect(await request("/day/create",other)).toEqual(other);
   const input={bucket,start:bounds.start,end:bounds.end,page_size:2};
   const first=await request("/day/page",input);expect(first.items).toEqual(rows.slice(1,3));expect(first.next).toEqual({occurs_at:bounds.start,id:rows[2].id});
   expect(await request("/day/page",input)).toEqual(first);
   const second=await request("/day/page",{...input,after:first.next});expect(second).toEqual({items:[rows[3]],next:null});
   const reverse=await request("/day/reverse",input);expect(reverse.items).toEqual([rows[3],rows[2]]);expect(reverse.next).toEqual({occurs_at:bounds.start,id:rows[2].id});
   expect(await request("/day/reverse",{...input,after:reverse.next})).toEqual({items:[rows[1]],next:null});
   expect(await request("/day/page",{...input,end:bounds.start})).toEqual({items:[],next:null});
   expect(await request("/day/page",{...input,start:bounds.end,end:bounds.start})).toEqual({items:[],next:null});
   observed.push({date,hours,bounds,stored:rows,other,first,second,reverse});
  },20000);
 }
 test("missing, nullable and invalid temporal bounds fail before page selection",async()=>{
  const bounds=app.temporal.day_bounds("2026-03-29","Europe/London");
  const valid={bucket:"day-2026-03-29",start:bounds.start,end:bounds.end,page_size:2};
  const before=journal.length;
  const pgPageCount=()=>pg?(readFileSync(Bun.env.JADPO_TEMPORAL_DAY_POSTGRES_LOG!,"utf8").match(/FROM "day_event" WHERE/g)||[]).length:0;
  const pgBefore=pgPageCount();
  const variants=[{...valid,start:undefined},{...valid,start:null},{...valid,start:"2026-03-29T00:00:00"},{...valid,start:"2026-03-29T00:00:00.0001Z"},{...valid,after:null}];
  const refused=[];
  for(const value of variants){const response=await app.handleRequest(new Request("http://day.test/day/page",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(value)}));expect(response.status).toBe(422);const body=await response.json();expect(body.error.code).toBe("invalid_input");refused.push({status:response.status,code:body.error.code});}
  if(!pg)expect(journal.length).toBe(before);else expect(pgPageCount()).toBe(pgBefore);
  observed.push({case:"invalid_temporal_or_optional_bounds",refused});
 });
 test("physical SQL and generated plan retain both required predicates and typed parameters",async()=>{
  let catalog;
  if(pg){const sql=new SQL({url:pg,prepare:false});try{
   const columns=await sql.unsafe("SELECT column_name,data_type,datetime_precision FROM information_schema.columns WHERE table_schema='public' AND table_name='day_event' ORDER BY ordinal_position");
   expect(columns.map((r:any)=>({name:r.column_name,type:r.data_type,precision:r.datetime_precision}))).toEqual([{name:"id",type:"text",precision:null},{name:"bucket",type:"text",precision:null},{name:"occurs_at",type:"timestamp with time zone",precision:3}]);
   const indexes=await sql.unsafe("SELECT indexdef FROM pg_indexes WHERE schemaname='public' AND tablename='day_event' AND indexname='day_event_page_bucket_occurs_at_id_idx'");expect(indexes).toHaveLength(1);expect(indexes[0].indexdef).toContain("(bucket, occurs_at, id)");catalog={columns,indexes};
  }finally{await sql.close();}}
  else{const db=new Database(join(root,"day.sqlite"));try{const columns=db.prepare('PRAGMA table_info("day_event")').all()as any[];expect(columns.map(r=>({name:r.name,type:r.type}))).toEqual([{name:"id",type:"TEXT"},{name:"bucket",type:"TEXT"},{name:"occurs_at",type:"BIGINT"}]);const indexes=db.prepare('PRAGMA index_info("day_event_page_bucket_occurs_at_id_idx")').all()as any[];expect(indexes.map(r=>r.name)).toEqual(["bucket","occurs_at","id"]);catalog={columns,indexes};}finally{db.close();}}
  observed.push({case:"actual_catalog",catalog});
  const manifest=JSON.parse(readFileSync(join(project,"build/persistence/entities.json"),"utf8"));
  expect(manifest.query_plans).toHaveLength(2);
  for(const p of manifest.query_plans){expect(p.index).toBe("day_event_page_bucket_occurs_at_id_idx");for(const key of["postgres_first_sql","postgres_after_sql","sqlite_first_sql","sqlite_after_sql"]){expect(p[key]).toContain('"occurs_at" >=');expect(p[key]).toContain('"occurs_at" <');expect(p[key]).not.toMatch(/DATE\s*\(|IS NULL OR "occurs_at"/iu);}}
  if(pg){
   const log=readFileSync(Bun.env.JADPO_TEMPORAL_DAY_POSTGRES_LOG!,"utf8");expect(log).toContain('FROM "day_event" WHERE "bucket" = $1 AND "occurs_at" >= $2 AND "occurs_at" < $3');expect(log).toContain('FROM "day_event" WHERE "bucket" = $1 AND "occurs_at" < $2 AND "occurs_at" >= $3');expect(log).toContain("parameters: $1 = 'day-2026-03-29'");
  }else{
   const statements=journal.filter(r=>r.sql.startsWith('SELECT ')&&r.sql.includes('FROM "day_event"'));
   expect(statements.length).toBeGreaterThanOrEqual(21);
   expect(statements.every(r=>r.sql.includes('"occurs_at" >=')&&r.sql.includes('"occurs_at" <'))).toBe(true);
   expect(statements[0].bindings.slice(0,3)).toEqual(["day-2026-03-29",Date.parse(observed[0].bounds.start),Date.parse(observed[0].bounds.end)]);
  }
 });
});
