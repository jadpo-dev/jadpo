import { afterAll, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { copyFileSync,mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join,resolve } from "node:path";
mkdirSync(resolve("build/validation"),{recursive:true});
const compiler=Bun.env.JADPO_BIN??resolve("jadpo/target/debug/jadpo");
const base=readFileSync(resolve("examples/first-party-authentication/app.jadpo"),"utf8");
const pg=Bun.env.JADPO_FINITE_WITNESS_DATABASE_URL;
if(pg){const u=new URL(pg);if(!["postgres:","postgresql:"].includes(u.protocol)||!["localhost","127.0.0.1"].includes(u.hostname)||u.pathname!=="/jadpo_auth_test_runtime")throw Error("Disposable local witness PostgreSQL required");}
afterAll(()=>{const log=Bun.env.JADPO_FINITE_WITNESS_POSTGRES_LOG;if(log)copyFileSync(log,resolve("build/validation/rm601-finite-witness-postgres-statements.log"));});
const literal='route GET /witness { output: Text action: { return "literal witness" } }';
const shadow='route GET /{slot} { auth: none path: { slot: Text } output: Text action: { return "shadow response" } }';
const cases=[
 {name:"unshadowed and genuine storage outage",prior:"",later:"",closed:true,outage:true},
 {name:"earlier wildcard",prior:shadow,later:"",closed:false},
 {name:"earlier mismatched parameter type",prior:shadow.replace("slot: Text","slot: Int"),later:"",closed:false,invalid:true},
 {name:"earlier imported file wildcard",prior:shadow,later:"",closed:false,cross:"early"},
 {name:"later imported file wildcard",prior:"",later:shadow,closed:true,cross:"late"},
 {name:"earlier disjoint static route",prior:'route GET /different { auth: none output: Text action: { return "other" } }',later:"",closed:true},
 {name:"earlier different-length template",prior:'route GET /other/{slot} { auth: none path: { slot: Text } output: Text action: { return "other" } }',later:"",closed:true},
 {name:"earlier different method",prior:shadow.replace("route GET","route POST"),later:"",closed:true},
 {name:"later same-file overlap",prior:"",later:shadow,closed:true},
] as const;
for(const fixture of cases){
 test(`native conditional witness: ${fixture.name}`,async()=>{
  const root=mkdtempSync(join(tmpdir(),"jadpo-finite-witness-"));
  const receipt=mkdtempSync(resolve("build/validation")+"/rm601-finite-witness.");
  const old=Object.fromEntries(["DATABASE_URL","SQLITE_PATH"].map(n=>[n,Bun.env[n]]));
  const observations:any[]=[],pins:Record<string,string>={};
  try{
   for(const version of ["before","after"]){
    const project=join(root,version);mkdirSync(project);const route=version==="after"?literal.replace("{ output:","{ auth: none output:"):literal;
    const cross="cross" in fixture?fixture.cross:null;
    const files=cross?{[cross==="early"?"a-shadow.jadpo":"z-shadow.jadpo"]:shadow,[cross==="early"?"z-app.jadpo":"a-app.jadpo"]:`${base}\n${route}\n`}:{"app.jadpo":`${base}\n${fixture.prior}\n${route}\n${fixture.later}\n`};
    for(const[n,s]of Object.entries(files)){writeFileSync(join(project,n),s);mkdirSync(join(receipt,version),{recursive:true});writeFileSync(join(receipt,version,n),s);}
    const build=Bun.spawnSync([compiler,"build",project],{stdout:"pipe",stderr:"pipe"});expect(build.exitCode,build.stderr.toString()).toBe(0);
    const subject=JSON.parse(readFileSync(join(project,"build/approval/subject.json"),"utf8"));
    for(const[n,f]of Object.entries(subject.canonical.after.provenance.generated_artifacts.target.files)as[string,any][]){
     const bytes=readFileSync(join(project,"build",n)),hash=createHash("sha256").update(bytes).digest("hex");expect(`sha256:${hash}`).toBe(f.byte_digest);pins[`${version}/${n}`]=hash;
     mkdirSync(join(receipt,version,n,".."),{recursive:true});writeFileSync(join(receipt,version,n),bytes);
    }
    const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",resolve("tests/runtime/fixtures/finite-witness-host.ts"),project,version,JSON.stringify(fixture)],{stdout:"pipe",stderr:"pipe"});
    const timeout=setTimeout(()=>child.kill(),20000),output=Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text()]);
    let exit:number;try{exit=await child.exited;}finally{clearTimeout(timeout);}
    const[stdout,stderr]=await output;writeFileSync(join(receipt,version,"native-stdout.log"),stdout);writeFileSync(join(receipt,version,"native-stderr.log"),stderr);
    expect(exit,stderr).toBe(0);const native=JSON.parse(stdout);expect(native.version).toBe(version);expect(native.observations.length).toBeGreaterThan(0);
    const healthy=native.observations.filter((r:any)=>r.phase==="healthy"),unhealthy=native.observations.filter((r:any)=>r.phase==="unhealthy");
    expect(healthy.length).toBe(pg&&"outage"in fixture?2:1);expect(unhealthy.length).toBe("outage"in fixture?1:0);
    for(const row of healthy){expect(row.version).toBe(version);expect(row.readiness).toBe(200);expect(row.request).toEqual({method:"GET",path:"/witness",authorization:null,cookie:null,apiKeyHeader:null});
      const expected="invalid"in fixture?422:fixture.closed?(version==="before"?401:200):200;expect(row.response.status).toBe(expected);
      if(expected===401)expect(row.response.body.error.code).toBe("authentication_required");else if(expected===422)expect(row.response.body.error.code).toBe("invalid_value");else expect(row.response.body).toBe(fixture.closed?"literal witness":"shadow response");}
    for(const row of unhealthy){expect(row.version).toBe(version);expect(row.readiness).toBe(503);expect(row.response.status).toBe(503);expect(row.response.body.error.code).toBe("dependency_unavailable");}
    observations.push(...native.observations);
    for(const port of [native.applicationPort,native.deploymentPort]){expect(Number.isInteger(port)&&port>0).toBe(true);let stopped=false;try{await fetch(`http://127.0.0.1:${port}/health/live`,{signal:AbortSignal.timeout(250)});}catch{stopped=true;}expect(stopped).toBe(true);}

   }
   const approval=Bun.spawnSync([compiler,"approval",join(root,"after"),"--before",join(root,"before")],{stdout:"pipe",stderr:"pipe"});expect(approval.exitCode,approval.stderr.toString()).toBe(0);
   const s=JSON.parse(approval.stdout.toString()),row=s.canonical.scenarios.find((r:any)=>r.route==="GET /witness");
   expect(row.counterexample.status).toBe("source_derived_anonymous_admission_candidate");expect(row.counterexample.conditional_witness.status).toBe(fixture.closed?"conditional_feasible_under_assumptions":"not_generated");
   expect(row.execution).toBe("not_proved");expect(row.feasibility).toBe("not_analyzed");expect(s.canonical.evidence.runtime_validated).toEqual([]);expect(s.canonical.release_authority).toBe(false);
   writeFileSync(join(receipt,"subject.json"),approval.stdout);writeFileSync(join(receipt,"observations.json"),JSON.stringify({fixture:fixture.name,backend:pg?"postgres":"sqlite",observations,pins},null,2)+"\n");
   console.log(JSON.stringify({kind:"native_dispatch_closed_witness",fixture:fixture.name,backend:pg?"postgres":"sqlite",receipt,observations,pins,conditionalStatus:row.counterexample.conditional_witness.status}));
  }finally{for(const[n,v]of Object.entries(old)){if(v===undefined)delete Bun.env[n];else Bun.env[n]=v;}rmSync(root,{recursive:true,force:true});}
 },30_000);
}
