// Bounded observation child owns actual generated hosts/listeners; normal exit
// releases inaccessible native PG pools without modifying generated code.
import {expect} from "bun:test";
import {Database} from "bun:sqlite";
import {SQL} from "bun";
import {join} from "node:path";
import {pathToFileURL} from "node:url";
const[project,version,rawFixture]=Bun.argv.slice(2),fixture=JSON.parse(rawFixture);
const pg=Bun.env.JADPO_FINITE_WITNESS_DATABASE_URL;
const environment={AUTH_SIGNING_KEY:Buffer.alloc(32,7).toString("base64url"),AUTH_PREVIOUS_SIGNING_KEY:Buffer.alloc(32,8).toString("base64url"),BROWSER_ORIGIN:"https://example.test"};
const observations:any[]=[];
    if(pg){Bun.env.DATABASE_URL=pg;delete Bun.env.SQLITE_PATH;}else{delete Bun.env.DATABASE_URL;Bun.env.SQLITE_PATH=join(project,"fixture.sqlite");}
    const handles=new Set<Database>(),original=Database.prototype.prepare;
    if(!pg)Database.prototype.prepare=function(...args:any[]){handles.add(this);return original.apply(this,args as any);};
    const app=await import(pathToFileURL(join(project,"build/target/app.ts")).href);
    await app.initializeApplication(environment);
    const server=Bun.serve({hostname:"127.0.0.1",port:0,fetch:app.handleRequest}),deployment=app.createDeploymentServer(0);expect(server.port).not.toBe(deployment.port);const applicationPort=server.port,deploymentPort=deployment.port;
    let control:SQL|null=null,disabled=false,closed=false;
    const ready=()=>fetch(`http://127.0.0.1:${deployment.port}/health/ready`,{signal:AbortSignal.timeout(2500)});
    let lastRequest:Request;
    const business=()=>{lastRequest=new Request(`http://127.0.0.1:${server.port}/witness`,{signal:AbortSignal.timeout(2500)});return fetch(lastRequest);};
    const assertNormal=async()=>{
     const r=await ready();expect(r.status).toBe(200);expect((await r.json()).checks).toEqual({database:"available"});
     const response=await business(),body=await response.json();
     const status="invalid"in fixture?422:fixture.closed?(version==="before"?401:200):200;
     expect(response.status).toBe(status);
     if(status===401)expect(body.error.code).toBe("authentication_required");else if(status===422)expect(body.error.code).toBe("invalid_value");else expect(body).toBe(fixture.closed?"literal witness":"shadow response");
     observations.push({version,phase:"healthy",readiness:r.status,response:{status:response.status,body},request:{method:lastRequest!.method,path:new URL(lastRequest!.url).pathname,authorization:lastRequest!.headers.get("authorization"),cookie:lastRequest!.headers.get("cookie"),apiKeyHeader:lastRequest!.headers.get("x-api-key")}});
    };
    try{
     await assertNormal();

     if("outage"in fixture){
      if(pg){const u=new URL(pg);u.pathname="/postgres";control=new SQL({url:u.toString(),prepare:false});await control.unsafe('ALTER DATABASE "jadpo_auth_test_runtime" ALLOW_CONNECTIONS false');disabled=true;
       const terminated=await control.unsafe("SELECT pg_terminate_backend(pid) AS terminated FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()",["jadpo_auth_test_runtime"]);expect(terminated.length).toBeGreaterThan(0);expect(terminated.every(r=>r.terminated===true)).toBe(true);
       observations.push({version,phase:"actual_connection_outage",terminated:terminated.length});
      }else{expect(handles.size).toBe(1);for(const h of handles)h.close();closed=true;observations.push({version,phase:"actual_native_handle_closed",handles:handles.size});}
      const deadline=performance.now()+5000;let unavailable:Response|undefined;
      while(performance.now()<deadline){unavailable=await ready();if(unavailable.status===503)break;expect(unavailable.status).toBe(200);await Bun.sleep(30);}
      expect(unavailable?.status).toBe(503);const readyBody=await unavailable!.json();expect(readyBody.checks).toEqual({database:"unavailable"});
      const failed=await business(),body=await failed.json();expect(failed.status).toBe(503);expect(body.error.code).toBe("dependency_unavailable");
      observations.push({version,phase:"unhealthy",readiness:503,response:{status:failed.status,body}});
      if(control){await control.unsafe('ALTER DATABASE "jadpo_auth_test_runtime" ALLOW_CONNECTIONS true');disabled=false;const until=performance.now()+5000;while(performance.now()<until){const r=await ready();if(r.status===200)break;expect(r.status).toBe(503);await Bun.sleep(30);}await assertNormal();}
     }
    }finally{if(disabled)await control!.unsafe('ALTER DATABASE "jadpo_auth_test_runtime" ALLOW_CONNECTIONS true');await control?.close();server.stop(true);deployment.stop(true);Database.prototype.prepare=original;if(!pg&&!closed)for(const h of handles)h.close();}
await Bun.write(Bun.stdout,JSON.stringify({version,observations,applicationPort,deploymentPort})+"\n");
process.exit(0);
