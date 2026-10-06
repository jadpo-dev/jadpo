import {expect,test} from "bun:test";
import {SQL} from "bun";
import {copyFileSync,mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync} from "node:fs";
import {createHash} from "node:crypto";
import {join,resolve} from "node:path";
import {tmpdir} from "node:os";
const pg=Bun.env.JADPO_TEMPORAL_PAGE_PROBE_DATABASE_URL;
if(pg){const u=new URL(pg);if(!["postgres:","postgresql:"].includes(u.protocol)||!["127.0.0.1","localhost"].includes(u.hostname)||u.pathname!=="/jadpo_auth_test_runtime")throw Error("Disposable local projection probe database required");}
const compiler=Bun.env.JADPO_BIN??resolve("jadpo/target/debug/jadpo");
const cases=[{label:"nontrivial authored Int limit preserves native read-fetch-read evaluation sequence",baselineOnly:true}];
for(const c of cases)test(c.label,async()=>{
 const root=mkdtempSync(join(tmpdir(),"jadpo-page-containment-")),project=join(root,"app"),receipt=mkdtempSync(resolve("build/validation/rm504-page-case."));mkdirSync(project);
 try{
 const source=readFileSync(resolve("tests/runtime/fixtures/temporal-page-limit-expression.jadpo"));writeFileSync(join(project,"app.jadpo"),source);const built=Bun.spawnSync([compiler,"build",project],{stdout:"pipe",stderr:"pipe"});writeFileSync(join(receipt,"build.stdout"),built.stdout);writeFileSync(join(receipt,"build.stderr"),built.stderr);expect(built.exitCode).toBe(0);
 const persistence=join(project,"build/target/persistence.ts"),original=readFileSync(persistence,"utf8");expect(original.split('import { SQL } from "bun";')).toHaveLength(2);expect(original.split('import { Database } from "bun:sqlite";')).toHaveLength(2);writeFileSync(join(receipt,"original-persistence.ts"),original);writeFileSync(persistence,original.replace('import { SQL } from "bun";','import { SQL } from "./temporal-page-sql-observer.ts";').replace('import { Database } from "bun:sqlite";','import { Database } from "./temporal-page-sql-observer.ts";'));copyFileSync(resolve("tests/runtime/fixtures/temporal-page-sql-observer.ts"),join(project,"build/target/temporal-page-sql-observer.ts"));
 const pins:any={};for(const name of ["target/app.ts","target/persistence.ts","target/temporal-page-sql-observer.ts","sql/sqlite/schema.sql","sql/postgres/schema.sql","approval/subject.json"]){const data=readFileSync(join(project,"build",name)),dest=join(receipt,name);mkdirSync(resolve(dest,".."),{recursive:true});writeFileSync(dest,data);pins[name]=createHash("sha256").update(data).digest("hex");}writeFileSync(join(receipt,"app.jadpo"),source);
 const env={...process.env};delete env.DATABASE_URL;delete env.SQLITE_PATH;delete env.JADPO_DEBUG_TARGET_STACKS;
 const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",resolve("tests/runtime/fixtures/temporal-page-limit-host.ts"),project,JSON.stringify(c)],{env,stdout:"pipe",stderr:"pipe"});const [stdout,stderr,exit]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);writeFileSync(join(receipt,"native.stdout"),stdout);writeFileSync(join(receipt,"native.stderr"),stderr);const observation=JSON.parse(stdout.trim().split("\n").at(-1)!);writeFileSync(join(receipt,"observations.json"),JSON.stringify(observation,null,2)+"\n");console.log(JSON.stringify({kind:"native_page_containment_probe",receipt,backend:pg?"postgres":"sqlite",case:c.label,exit,pins,observation}));
 for(const port of Object.values(observation.ports) as number[]){let refused=false;try{await fetch(`http://127.0.0.1:${port}/health/live`,{signal:AbortSignal.timeout(1000)});}catch{refused=true;}expect(refused).toBe(true);}
 if(pg){expect(observation.ownedPostgresPids.length).toBeGreaterThan(0);const db=new SQL({url:pg,prepare:false});try{const pids=observation.ownedPostgresPids,rows=await db.unsafe(`SELECT pid FROM pg_stat_activity WHERE pid IN (${pids.map((_:any,i:number)=>"$"+(i+1)).join(",")})`,pids);expect(rows).toHaveLength(0);}finally{await db.close();}}
 expect(observation.observations.at(-1).phase).toBe("restored");
 expect(exit).toBe(0);expect(observation.failed).toBeNull();
 }finally{rmSync(root,{recursive:true,force:true});}
},20000);
