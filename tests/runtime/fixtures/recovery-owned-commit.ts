// Actual generated recovery: held COMMIT acknowledgement retains native ownership beyond the caller deadline.
import {SQL} from "bun";
import {Database} from "bun:sqlite";
import {dirname} from "node:path";
import {pathToFileURL} from "node:url";
import {join} from "node:path";
import {readFileSync} from "node:fs";
import {createConnection,createServer,type Socket,type AddressInfo} from "node:net";
const value=Bun.env.JADPO_READINESS_TEST_DATABASE_URL;
if(!value) throw Error("Requires disposable PostgreSQL fixture");
const direct=new URL(value);
if(!["127.0.0.1","localhost"].includes(direct.hostname)||!/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(direct.pathname)) throw Error("Not a disposable local database");
const control=new SQL(value,{max:1,prepare:false});
const sockets:Socket[]=[]; const backendOwners=new Set<number>(); let closedConnections=0;
let armed=false,beginCount=0;const held:{release:()=>void}[]=[];
const release=()=>{for(const value of held.splice(0))value.release()};
let connections=0,commitsForwarded=0,ackObserved=false,clientCloseObserved=false,backendPid:number|null=null;
let acknowledge!:()=>void;
const acknowledgement=new Promise<void>(resolve=>{acknowledge=resolve;});
const proxy=createServer(downstream=>{
  connections++;
  const upstream=createConnection({host:direct.hostname,port:Number(direct.port||5432)});
  sockets.push(downstream,upstream);
  let frontend=Buffer.alloc(0),started=false,backend=Buffer.alloc(0),commit=false,heldFrames:Buffer[]|null=null,connectionPid:number|null=null;
  downstream.on("data",chunk=>{
    frontend=Buffer.concat([frontend,chunk]);
    if(!started){if(frontend.length<4)return;const size=frontend.readUInt32BE(0);if(frontend.length<size)return;upstream.write(frontend.subarray(0,size));frontend=frontend.subarray(size);started=true;}
    while(frontend.length>=5){const size=frontend.readUInt32BE(1)+1;if(frontend.length<size)return;const frame=frontend.subarray(0,size);frontend=frontend.subarray(size);
      const tag=String.fromCharCode(frame[0]);
      let command="";if(tag==="Q")command=frame.subarray(5,-1).toString();else if(tag==="P"){const end=frame.indexOf(0,5);const tail=frame.subarray(end+1);command=tail.subarray(0,tail.indexOf(0)).toString();}
      if(/^\s*BEGIN\b/iu.test(command))beginCount++;
      if(/^\s*COMMIT\s*;?\s*$/iu.test(command)){commitsForwarded++;if(armed){armed=false;commit=true;backendPid=connectionPid;heldFrames=[];held.push({release:()=>{commit=false;for(const frame of heldFrames!)downstream.write(frame);heldFrames=null}});}}
      upstream.write(frame);
    }
  });
  upstream.on("data",chunk=>{
    if(!commit){
      // BackendKeyData supplies the actual native server owner, without logs or
      // credential/URL disclosure. Startup messages are whole protocol frames.
      backend=Buffer.concat([backend,chunk]);
      while(backend.length>=5){const size=backend.readUInt32BE(1)+1;if(backend.length<size)break;const frame=backend.subarray(0,size);backend=backend.subarray(size);if(frame[0]===75){connectionPid=frame.readInt32BE(5);backendOwners.add(connectionPid);}}
      downstream.write(chunk);return;
    }
    backend=Buffer.concat([backend,chunk]);
    while(backend.length>=5){const size=backend.readUInt32BE(1)+1;if(backend.length<size)return;const frame=backend.subarray(0,size);backend=backend.subarray(size);
      if(frame[0]===67&&frame.subarray(5).toString()==="COMMIT\0"){ackObserved=true;acknowledge();}
      heldFrames!.push(Buffer.from(frame));
      // Withhold actual COMMIT CommandComplete and ReadyForQuery. The socket
      // remains live until the native owned pool closes, not fixture timeout.
    }
  });
  downstream.on("close",()=>{closedConnections++;if(commit)clientCloseObserved=true;upstream.end();upstream.destroy();});
  downstream.on("error",()=>upstream.destroy());upstream.on("error",()=>downstream.destroy());
});
await new Promise<void>(resolve=>proxy.listen(0,"127.0.0.1",resolve));
const proxied=new URL(value);proxied.port=String((proxy.address() as AddressInfo).port);

let emitted:any=null;
const app=Bun.env.JADPO_READINESS_APPLICATION; if(!app)throw Error("Requires generated fixture");
const {initialNativeStartupDeadline}=await import(pathToFileURL(join(dirname(app),"startup-owner.ts")).href);
const source=readFileSync(join(dirname(app),"persistence.ts"),"utf8").replace(/^import .+;\n/gm,"").replace(/\bexport /gu,"").replaceAll("import.meta.url",JSON.stringify(pathToFileURL(join(dirname(app),"persistence.ts")).href));
const javascript=new Bun.Transpiler({loader:"ts"}).transformSync(source)+'\nreturn {refresh:refreshPersistenceReadiness,unavailable:()=>{persistenceReady=false;persistenceProbeNeeded=true},state:()=>({ready:persistenceReady,fatal:persistenceInitializationFatal,client:activePersistenceClient}),close:()=>postgres?.close()};';
const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
try{
 emitted=await new AsyncFunction("initialNativeStartupDeadline","SQL","Database","Bun",javascript)(initialNativeStartupDeadline,SQL,Database,{env:{DATABASE_URL:proxied.href}});
 if(!emitted.state().ready)throw Error("No initial native fixture readiness");
 const originalClient=emitted.state().client;await control.unsafe('DROP INDEX IF EXISTS "account_owner_email_idx"');
 armed=true;emitted.unavailable();const started=performance.now();const responses=Array.from({length:32},()=>emitted.refresh());
 let watchdog:ReturnType<typeof setTimeout>|undefined;
 try{await Promise.race([acknowledgement,new Promise<never>((_,reject)=>{watchdog=setTimeout(()=>reject(Error("No native held recovery COMMIT")),3000)})])}finally{clearTimeout(watchdog)}
 const committed=(await control.unsafe("SELECT oid FROM pg_class WHERE relname='account_owner_email_idx'")).length===1;
 const result=await Promise.all(responses);const responseMs=performance.now()-started;
 await Bun.sleep(100);const later=Array.from({length:128},()=>emitted.refresh());
 const retainedSlot=later.every(value=>value===responses[0]),clientUnchanged=emitted.state().client===originalClient;
 const before={ready:emitted.state().ready,fatal:emitted.state().fatal,committed,beginCount,commitsForwarded,retainedSlot,clientUnchanged,responseMs};
 if(result.some(value=>value!==false)||!committed||beginCount!==2||commitsForwarded!==2||!retainedSlot||!clientUnchanged||emitted.state().ready||emitted.state().fatal||responseMs<900||responseMs>1500)throw Error("Unknown COMMIT ownership or response fence failed");
 release();await Bun.sleep(100);
 if(emitted.state().ready||emitted.state().client!==originalClient)throw Error("Late acknowledgement published stale client");
 await Bun.sleep(200);const recovered=await emitted.refresh();
 if(!recovered||!emitted.state().ready||beginCount!==3)throw Error("Fresh recovery did not follow actual native settlement");
 await emitted.close();emitted=null;
 for(let i=0;i<100&&closedConnections!==connections;i++)await Bun.sleep(5);
 const backendRemaining=Number((await control.unsafe("SELECT count(*) AS count FROM pg_stat_activity WHERE pid IN ("+[...backendOwners].map((_,i)=>"$"+(i+1)).join(",")+")",[...backendOwners]))[0].count);
 if(backendRemaining!==0||closedConnections!==connections)throw Error("Recovery native owner did not close");
 console.log(JSON.stringify({kind:"actual_recovery_commit_ack_hold",before,after:{freshRecovery:recovered,beginCount,commitsForwarded,connections,closedConnections,backendRemaining},scope:"ServerDDL budget does not bound COMMIT transport; original1s caller response retains native slot until acknowledgement, no late publication, committed schema preserved"}));
}finally{release();await emitted?.close();for(const socket of sockets)socket.destroy();proxy.close();await control.close()}
