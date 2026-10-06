// Actual generated startup owner: external watchdog must not perform termination.
import {SQL} from "bun";
import {join} from "node:path";
import {mkdtempSync,readFileSync,writeFileSync,existsSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {createConnection,createServer,type Socket,type AddressInfo} from "node:net";
const value=Bun.env.JADPO_TEMPORAL_SCHEMA_DATABASE_URL;
if(!value) throw Error("Requires disposable PostgreSQL fixture");
const direct=new URL(value);
if(!["127.0.0.1","localhost"].includes(direct.hostname)||!/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(direct.pathname)) throw Error("Not a disposable local database");
const control=new SQL(value,{max:1,prepare:false});
const sockets:Socket[]=[]; const backendOwners=new Set<number>(); let closedConnections=0;
let connections=0,catalogForwarded=0,ackObserved=false,clientCloseObserved=false,backendPid:number|null=null;
let acknowledge!:()=>void;
const acknowledgement=new Promise<void>(resolve=>{acknowledge=resolve;});
const proxy=createServer(downstream=>{
  connections++;
  const upstream=createConnection({host:direct.hostname,port:Number(direct.port||5432)});
  sockets.push(downstream,upstream);
  let frontend=Buffer.alloc(0),started=false,backend=Buffer.alloc(0),catalog=false,catalogQueued=false,connectionPid:number|null=null;
  downstream.on("data",chunk=>{
    frontend=Buffer.concat([frontend,chunk]);
    if(!started){if(frontend.length<4)return;const size=frontend.readUInt32BE(0);if(frontend.length<size)return;upstream.write(frontend.subarray(0,size));frontend=frontend.subarray(size);started=true;}
    while(frontend.length>=5){const size=frontend.readUInt32BE(1)+1;if(frontend.length<size)return;const frame=frontend.subarray(0,size);frontend=frontend.subarray(size);
      const tag=String.fromCharCode(frame[0]);
      let command="";if(tag==="Q")command=frame.subarray(5,-1).toString();else if(tag==="P"){const end=frame.indexOf(0,5);const tail=frame.subarray(end+1);command=tail.subarray(0,tail.indexOf(0)).toString();}
      if(command.includes("pg_catalog.pg_attribute"))catalogQueued=true;
      if(catalogQueued && (tag==="E" || tag==="Q")){catalogForwarded++;catalog=true;catalogQueued=false;backendPid=connectionPid;}
      upstream.write(frame);
    }
  });
  upstream.on("data",chunk=>{
    if(!catalog){
      // BackendKeyData supplies the actual native server owner, without logs or
      // credential/URL disclosure. Startup messages are whole protocol frames.
      backend=Buffer.concat([backend,chunk]);
      while(backend.length>=5){const size=backend.readUInt32BE(1)+1;if(backend.length<size)break;const frame=backend.subarray(0,size);backend=backend.subarray(size);if(frame[0]===75){connectionPid=frame.readInt32BE(5);backendOwners.add(connectionPid);}}
      downstream.write(chunk);return;
    }
    backend=Buffer.concat([backend,chunk]);
    while(backend.length>=5){const size=backend.readUInt32BE(1)+1;if(backend.length<size)return;const frame=backend.subarray(0,size);backend=backend.subarray(size);
      if(frame[0]===67&&/^SELECT [0-9]+\u0000$/u.test(frame.subarray(5).toString())){ackObserved=true;acknowledge();}
      // Withhold actual catalog SELECT CommandComplete and ReadyForQuery. The socket
      // remains live until the native owned pool closes, not fixture timeout.
    }
  });
  downstream.on("close",()=>{closedConnections++;if(catalog)clientCloseObserved=true;upstream.end();upstream.destroy();});
  downstream.on("error",()=>upstream.destroy());upstream.on("error",()=>downstream.destroy());
});
await new Promise<void>(resolve=>proxy.listen(0,"127.0.0.1",resolve));
const proxied=new URL(value);proxied.port=String((proxy.address() as AddressInfo).port);
const fixture=mkdtempSync(join(tmpdir(),"jadpo-owned-startup-probe-"));
const app=Bun.env.JADPO_TEMPORAL_SCHEMA_APP; if(!app)throw Error("Requires freshly generated fixture");
const marker=join(fixture,"listener-started");
const preload=join(fixture,"observe-listen.ts");
writeFileSync(preload,`import {writeFileSync} from "node:fs";import {Server} from "node:net";const marker=${JSON.stringify(marker)};const spawn=Bun.spawn;Bun.spawn=((args:any,options:any)=>spawn([...args.slice(0,-1),"--preload",${JSON.stringify(preload)},args.at(-1)],options))as typeof Bun.spawn;const serve=Bun.serve;Bun.serve=((...args:any[])=>{writeFileSync(marker,"Bun.serve");return serve(...args);}) as typeof Bun.serve;const listen=Server.prototype.listen;Server.prototype.listen=function(...args:any[]){writeFileSync(marker,"Node.listen");return listen.apply(this,args as any);};`);
const reserve=()=>{const server=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});const port=server.port;server.stop(true);return port;};
const appPort=reserve(),deploymentPort=reserve(),started=performance.now(),deadline=started+15_000;
const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null","--preload",preload,app],{env:{PATH:Bun.env.PATH!,DATABASE_URL:proxied.href,PORT:String(appPort),JADPO_DEPLOYMENT_PORT:String(deploymentPort)},stdout:"pipe",stderr:"pipe"});
const output=new Response(child.stdout).text(),errors=new Response(child.stderr).text();
// A separate owned observer keeps pending fetch lifetime out of proxy cleanup.
const observerFile=join(fixture,"network-observer.ts");
writeFileSync(observerFile,`let polls=0,bindings=0;process.once("SIGTERM",()=>{console.log(JSON.stringify({polls,bindings}));process.exit(0)});while(true){for(const port of ${JSON.stringify([appPort,deploymentPort])}){polls++;try{await fetch("http://127.0.0.1:"+port+"/health/live",{signal:AbortSignal.timeout(100)});bindings++}catch{}}await Bun.sleep(20)}`);
const observer=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",observerFile],{env:{PATH:Bun.env.PATH??""},stdout:"pipe",stderr:"pipe"});
const observerOutput=new Response(observer.stdout).text(),observerErrors=new Response(observer.stderr).text();

let expiryObserved=false;
const expiry=setTimeout(()=>{expiryObserved=true;child.kill("SIGKILL");},Math.max(0,deadline-performance.now()));
try{
  await Promise.race([acknowledgement,child.exited.then(()=>{throw Error("No held initial native catalog SELECT");})]);
  if(catalogForwarded!==1||child.exitCode!==null)throw Error("No held initial native catalog SELECT");
  const catalogExecuted=(await control`SELECT state FROM pg_stat_activity WHERE pid=${backendPid}`)[0]?.state === "idle in transaction";
  const before={listenerStarts:existsSync(marker)?1:0,childStillOwns:true,clientSocketClosed:clientCloseObserved,catalogExecuted};
  const exit=await child.exited; // actual OS ownership termination acknowledgement
  const elapsedMs=performance.now()-started;
  for(let i=0;i<100&&(closedConnections!==connections||!clientCloseObserved);i++)await Bun.sleep(5);
  const backendRemaining=Number((await control`SELECT count(*) AS count FROM pg_stat_activity WHERE pid=${backendPid}`)[0].count);
  const allBackendsRemaining=Number((await control.unsafe("SELECT count(*) AS count FROM pg_stat_activity WHERE pid IN ("+[...backendOwners].map((_,index)=>"$"+(index+1)).join(",")+")",[...backendOwners]))[0].count);
  const stdout=await output,stderr=await errors;
  observer.kill("SIGTERM");if(await observer.exited!==0||await observerErrors!=="")throw Error("Native port observer did not acknowledge shutdown");const network=JSON.parse(await observerOutput);
  const listenerStarts=existsSync(marker)?1:0;
  console.log(JSON.stringify({kind:"generated_owned_startup_temporal_catalog_probe",bun:Bun.version,policyMs:10_000,elapsedMs,network,connections,closedConnections,allBackendsRemaining,ackObserved,catalogForwarded,before,expiryObserved,terminationExit:exit,terminationAcknowledged:true,clientCloseObserved,backendRemaining,listenerStarts,publishedReady:stdout.includes("runtime.ready"),startupFaultReported:stderr.includes("RUNTIME_STARTUP_FAILED"),source:"fresh generated temporal-roundtrip fixture",scope:"Compiler-owned initial native startup termination; no request/recovery bound"}));
  if(network.polls===0||network.bindings!==0||connections===0||closedConnections!==connections||allBackendsRemaining!==0||expiryObserved||exit!==1||elapsedMs<9500||elapsedMs>12000||!stderr.includes("RUNTIME_STARTUP_FAILED")||!clientCloseObserved||backendRemaining!==0||listenerStarts!==0||stdout.includes("runtime.ready")||!catalogExecuted)throw Error("Native ownership/bind proof failed");
}finally{clearTimeout(expiry);if(observer.exitCode===null){observer.kill("SIGKILL");await observer.exited;}if(child.exitCode===null){child.kill("SIGKILL");await child.exited;}for(const socket of sockets)socket.destroy();proxy.close();await control.close({timeout:0});rmSync(fixture,{recursive:true,force:true});}
