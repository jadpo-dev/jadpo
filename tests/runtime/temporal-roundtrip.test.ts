import { afterAll, describe, expect, test } from "bun:test";
import { SQL } from "bun";
import { Database } from "bun:sqlite";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// This fixture exercises supported generated CRUD/equality queries. It does not
// qualify the separately rejected half-open predicate, authentication or main.
const postgresUrl = Bun.env.JADPO_TEMPORAL_ROUNDTRIP_DATABASE_URL;
if (postgresUrl) {
  const url = new URL(postgresUrl);
  if (!["postgres:", "postgresql:"].includes(url.protocol) || !["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Requires a disposable local temporal database");
}
const root = mkdtempSync(join(tmpdir(), "jadpo-temporal-roundtrip-"));
const project = join(root, "application"); mkdirSync(project);
copyFileSync(resolve("tests/runtime/fixtures/temporal-roundtrip.jadpo"), join(project, "app.jadpo"));
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const built = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (built.exitCode) { rmSync(root, {recursive:true,force:true}); throw Error(`Temporal fixture build refused: ${built.stdout}\n${built.stderr}`); }
const old = Object.fromEntries(["DATABASE_URL", "SQLITE_PATH"].map(name => [name, Bun.env[name]]));
if (postgresUrl) { Bun.env.DATABASE_URL = postgresUrl; delete Bun.env.SQLITE_PATH; }
else { delete Bun.env.DATABASE_URL; Bun.env.SQLITE_PATH = join(root, "temporal.sqlite"); }
const observations: Record<string, unknown>[] = [];
const receiptBase = resolve("build/validation"); mkdirSync(receiptBase, { recursive: true });
const receipt = mkdtempSync(join(receiptBase, "rm504-temporal-roundtrip-runtime."));
const generatedPins: Record<string,string> = {};
for (const name of ["target/app.ts", "target/persistence.ts", "target/startup-owner.ts", "sql/sqlite/schema.sql", "sql/postgres/schema.sql", "persistence/entities.json", "approval/subject.json"]) {
  const bytes = readFileSync(join(project, "build", name));
  const destination = join(receipt, name); mkdirSync(resolve(destination, ".."), {recursive:true});
  copyFileSync(join(project,"build",name),destination);
  generatedPins[name] = createHash("sha256").update(bytes).digest("hex");
}
let inspection: SQL | Database | null = null;
let schemaKinds: unknown = null;
afterAll(async () => {
  if (inspection instanceof Database) inspection.close(); else await inspection?.close();
  for (const [name,value] of Object.entries(old)) { if(value===undefined) delete Bun.env[name];else Bun.env[name]=value; }
  rmSync(root, {recursive:true,force:true});
  expect(observations).toHaveLength(4);
  console.log(JSON.stringify({kind:"native_generated_temporal_roundtrip",backend:postgresUrl?"postgres":"sqlite",surface:"imported_generated_persistence_and_direct_public_handler",authentication:"not_exercised",mainEntry:"not_exercised",observations,schemaKinds,provenance:app.temporalProvenance,generatedPins,receipt}));
});
const app = await import(pathToFileURL(join(project,"build/target/app.ts")).href);
const {persistence} = await import(pathToFileURL(join(project,"build/target/persistence.ts")).href);
if (postgresUrl) {
  const sql = new SQL({url:postgresUrl,prepare:false}); inspection = sql;
  const columns = await sql.unsafe("SELECT column_name, data_type, datetime_precision FROM information_schema.columns WHERE table_schema='public' AND table_name='temporal_record' ORDER BY ordinal_position");
  schemaKinds = columns.map(({column_name,data_type,datetime_precision}:Record<string,unknown>)=>({name:column_name,type:data_type,precision:datetime_precision}));
  expect(schemaKinds).toEqual([
    {name:"id",type:"text",precision:null},{name:"instant",type:"timestamp with time zone",precision:3},{name:"calendar_date",type:"date",precision:0},
    {name:"elapsed",type:"bigint",precision:null},{name:"resolved",type:"jsonb",precision:null},{name:"optional_instant",type:"timestamp with time zone",precision:3},
  ]);
} else {
  const db = new Database(join(root,"temporal.sqlite")); inspection = db;
  const columns = db.prepare('PRAGMA table_info("temporal_record")').all() as Array<{name:string,type:string}>;
  schemaKinds = columns.map(({name,type})=>({name,type}));
  expect(schemaKinds).toEqual([{name:"id",type:"TEXT"},{name:"instant",type:"BIGINT"},{name:"calendar_date",type:"TEXT"},{name:"elapsed",type:"BIGINT"},{name:"resolved",type:"TEXT"},{name:"optional_instant",type:"BIGINT"}]);
}

async function request(path:string,value:unknown) {
  return app.handleRequest(new Request("http://temporal.test"+path,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(value)}));
}
const input = {
  id:"00000000-0000-4000-8000-000000000001",
  instant:"2026-10-25T01:30:00.123+01:00",
  calendar_date:"2024-02-29",
  elapsed:"-PT1.234S",
  resolved:{instant:"2026-10-25T01:30:00.123+01:00",zone:"Europe/London"},
  optional_instant:"2026-03-29T02:00:00.001+01:00",
};
const canonical = {...input,instant:"2026-10-25T00:30:00.123Z",resolved:{instant:"2026-10-25T00:30:00.123Z",zone:"Europe/London"},optional_instant:"2026-03-29T01:00:00.001Z"};
function noHostDate(value:unknown):boolean {
  if(value instanceof Date)return false;
  if(Array.isArray(value))return value.every(noHostDate);
  return value===null || typeof value!=="object" || Object.values(value).every(noHostDate);
}

describe("generated native typed temporal roundtrips",()=>{
  test("canonicalizes scalar and resolved temporal values through stored create/read",async()=>{
    const created=await request("/temporal/create",input);expect(created.status).toBe(200);expect(await created.json()).toEqual(canonical);
    const fetched=await request("/temporal/find",{id:input.id});expect(fetched.status).toBe(200);const native=await fetched.json();expect(native).toEqual(canonical);expect(noHostDate(native)).toBe(true);
    observations.push({case:"scalar_resolved_roundtrip",value:native,noHostDate:true});
  });
  test("preserves portable year boundaries, signed duration normalization and nullable instant",async()=>{
    const value={...canonical,id:"00000000-0000-4000-8000-000000000002",instant:"9999-12-31T23:59:59.999Z",calendar_date:"0001-01-01",elapsed:"PT90S",resolved:{instant:"2026-03-08T07:30:00.000Z",zone:"America/New_York"},optional_instant:null};
    const expected={...value,elapsed:"PT1M30S"};
    const created=await request("/temporal/create",value);expect(created.status).toBe(200);expect(await created.json()).toEqual(expected);
    const fetched=await request("/temporal/find",{id:value.id});expect(fetched.status).toBe(200);const native=await fetched.json();expect(native).toEqual(expected);expect(noHostDate(native)).toBe(true);
    observations.push({case:"portable_nullable_roundtrip",value:native,noHostDate:true});
  });
  test("normalizes a typed Instant equality parameter before actual stored lookup",async()=>{
    const response=await request("/temporal/equal",{instant:input.instant});expect(response.status).toBe(200);const rows=await response.json();expect(rows).toEqual([canonical]);
    const absent=await request("/temporal/equal",{instant:"2026-10-25T00:30:00.124Z"});expect(absent.status).toBe(200);expect(await absent.json()).toEqual([]);
    observations.push({case:"typed_equality_lookup",rows,adjacentMillisecondAbsent:true});
  });
  test("refuses invalid temporal wire values before any stored row",async()=>{
    const invalid=[
      {label:"submillisecond",patch:{instant:"2026-10-25T00:30:00.1234Z"}},
      {label:"zoneless_instant",patch:{instant:"2026-10-25T00:30:00.123"}},
      {label:"invalid_calendar_date",patch:{calendar_date:"2026-02-30"}},
      {label:"unknown_time_zone",patch:{resolved:{...canonical.resolved,zone:"Jadpo/Unknown"}}},
      {label:"extra_time_field",patch:{resolved:{...canonical.resolved,authority:"must-not-exist"}}},
      {label:"duration_precision",patch:{elapsed:"PT1.0001S"}},
      {label:"numeric_duration_wire",patch:{elapsed:1234}},
      {label:"decimal_string_duration_wire",patch:{elapsed:"1234"}},
      {label:"duration_range",patch:{elapsed:"PT9007199254740992S"}},
      {label:"instant_range",patch:{instant:"10000-01-01T00:00:00.000Z"}},
    ];
    const results=[];
    for(const [index,item]of invalid.entries()){
      const id=`00000000-0000-4000-8000-${String(index+10).padStart(12,"0")}`;
      const response=await request("/temporal/create",{...canonical,id,...item.patch});expect(response.status).toBe(422);const body=await response.json();expect(body.error.code).toBe("invalid_input");
      expect(await persistence.query_optional_TemporalRecord_by_id(id)).toBeNull();
      results.push({case:item.label,status:response.status,code:body.error.code,storedRowAbsent:true});
    }
    expect(app.temporalProvenance.runtimeRuleDataVersion).toBe(process.versions.tz?.trim()?process.versions.tz:"unreported");expect(app.temporalProvenance.runtimeRuleDataByteDigest).toBeNull();expect(app.temporalProvenance.tzdbScope).toBe("compiler_zone_enumeration_only");
    observations.push({case:"strict_ingress_refusal",results});
  });
});
