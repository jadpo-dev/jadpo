import {createHash} from "node:crypto";
import {createReadinessGate} from "../../../jadpo/crates/core/src/runtime/readiness_gate.ts";
// Tests-only owned driver process feasibility; the generated main and serving pool remain unchanged.
import { SQL } from "bun";
import { createConnection, createServer, type AddressInfo, type Socket } from "node:net";
import { appendFileSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const mode=Bun.argv[2]??"WORKER-01";if(!["WORKER-01","WORKER-02","WORKER-03","WORKER-04","WORKER-05"].includes(mode))throw Error("probe.invalid_mode");
const supplied = Bun.env.JADPO_READINESS_TEST_DATABASE_URL, app = Bun.env.JADPO_READINESS_APPLICATION;
if (!supplied || !app) throw Error("probe.native_input_required");
const base = new URL(supplied);
if (!["127.0.0.1", "localhost"].includes(base.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(base.pathname)) throw Error("probe.disposable_database_required");
const database = `jadpo_auth_test_probe_${crypto.randomUUID().replaceAll("-", "")}`;
const admin = new SQL(supplied, { max: 1, prepare: false });
await admin.unsafe(`CREATE DATABASE "${database}"`);
const direct = new URL(supplied); direct.pathname = `/${database}`;
const control = new SQL(direct.href, { max: 1, prepare: false });
const directory = mkdtempSync(join(tmpdir(), "jadpo-native-intrinsic-owner-"));
const pidPath = join(directory, "pids"), listenerPath = join(directory, "listeners"), preload = join(directory, "observe.ts");
writeFileSync(preload, `import {appendFileSync} from "node:fs";import {Server} from "node:net";appendFileSync(${JSON.stringify(pidPath)},JSON.stringify({pid:process.pid,child:typeof process.send==="function"})+"\\n");const spawn=Bun.spawn;Bun.spawn=((a:any,o:any)=>spawn(Array.isArray(a)?[...a.slice(0,-1),"--preload",${JSON.stringify(preload)},a.at(-1)]:a,o))as typeof Bun.spawn;const serve=Bun.serve;Bun.serve=((...a:any[])=>{appendFileSync(${JSON.stringify(listenerPath)},"Bun.serve\\n");return serve(...a);})as typeof Bun.serve;const listen=Server.prototype.listen;Server.prototype.listen=function(...a:any[]){appendFileSync(${JSON.stringify(listenerPath)},"Node.listen\\n");return listen.apply(this,a as any);};`);
const check = (value: unknown, code: string) => { if (!value) throw Error(`probe.${code}`); };
const omitTermination=Bun.env.JADPO_DRIVER_OMIT_KILL==="1",omitHold=Bun.env.JADPO_DRIVER_OMIT_HOLD==="1";
const nonce=crypto.randomUUID().replaceAll("-","").slice(0,16),privateRole="jadpo_intrinsic_"+nonce,ownerLabel="jadpo.intrinsic."+nonce+".owner";
await admin.unsafe(`CREATE ROLE "${privateRole}" LOGIN`);
const sockets:Socket[]=[],owners=new Set<number>(),transports:any[]=[],executionTrace:any[]=[],cancellations:any[]=[],held:any[]=[];
let connections=0,downstreamClosed=0,upstreamClosed=0,protocolFailure:string|null=null,holdReleased=false,acquisitionHeld=false,healthRows=0,healthCommand=false,healthIdle=false,selected:any=null;
function string(frame:Buffer,offset:number):[string,number]{const end=frame.indexOf(0,offset);check(end>=offset,"protocol_string");return[frame.subarray(offset,end).toString(),end+1];}
function kind(sql:string){if(sql.includes("pg_catalog.pg_terminate_backend")){check(sql.trim()==="SELECT pg_catalog.pg_terminate_backend(pg_catalog.pg_backend_pid(), 0) AS signaled","intrinsic_target_only");return"intrinsic";}if(/^SELECT 1(?: AS health)?$/u.test(sql.trim()))return"health";return"other";}
function errorCode(frame:Buffer){let offset=5;while(offset<frame.length&&frame[offset]!==0){const field=String.fromCharCode(frame[offset++]);const[value,next]=string(frame,offset);offset=next;if(field==="C")return/^[0-9A-Z]{5}$/u.test(value)?value:"invalid";}return null;}
const proxy=createServer(downstream=>{
 const index=++connections,upstream=createConnection({host:direct.hostname,port:Number(direct.port||5432)});sockets.push(downstream,upstream);check(connections<=64,"connection_bound");
 const transport:any={connection:index,pid:null,applicationName:null,role:null,downstreamCloseAt:null,upstreamCloseAt:null,downstreamEndAt:null,upstreamEndAt:null,driverTerminateAt:null,nativeFatalCodes:[],heldEOF:false};transports.push(transport);
 let front=Buffer.alloc(0),back=Buffer.alloc(0),started=false,sslResponse=false,pid:number|null=null,applicationName:string|null=null,initialReady=true,lastCompleted:any=null;
 const statements=new Map<string,string>(),portals=new Map<string,string>(),executions:any[]=[];
 const fail=()=>{protocolFailure="bounded_native_protocol_refused";downstream.destroy();upstream.destroy();};
 downstream.on("data",chunk=>{try{front=Buffer.concat([front,chunk]);check(front.length<=1048576,"front_bound");if(!started){if(front.length<4)return;const size=front.readUInt32BE(0);check(size>=8&&size<=65536,"startup_bound");if(front.length<size)return;const packet=front.subarray(0,size);front=front.subarray(size);const code=packet.readUInt32BE(4);if(code===80877103){check(size===8,"ssl_frame");sslResponse=true;upstream.write(packet);return;}if(code===80877102){check(size===16,"cancel_frame");cancellations.push({connection:index,targetPID:packet.readInt32BE(8),at:performance.now()});}else{check(code===196608,"startup_version");let at=8;const parameters:any={};while(at<packet.length-1){const[key,next]=string(packet,at);const[value,end]=string(packet,next);parameters[key]=value;at=end;}applicationName=parameters.application_name??null;transport.applicationName=applicationName;transport.role=parameters.user??null;}upstream.write(packet);started=true;}
 while(front.length>=5){const size=front.readUInt32BE(1)+1;check(size>=5&&size<=1048576,"front_frame");if(front.length<size)return;const frame=front.subarray(0,size);front=front.subarray(size);const tag=String.fromCharCode(frame[0]);let sqls:string[]=[];
 if(tag==="P"){const[name,offset]=string(frame,5);const[sql]=string(frame,offset);statements.set(name,sql);check(statements.size<=1024,"statement_bound");}
 if(tag==="B"){const[portal,offset]=string(frame,5);const[statement]=string(frame,offset);check(statements.has(statement),"bind_mapping");portals.set(portal,statements.get(statement)!);}
 if(tag==="E"){const[portal]=string(frame,5);check(portals.has(portal),"execute_mapping");sqls=[portals.get(portal)!];}
 if(tag==="Q")sqls=string(frame,5)[0].split(';').map(x=>x.trim()).filter(Boolean);
 for(const sql of sqls){const k=kind(sql),target=applicationName===ownerLabel&&k==="health";const e:any={index:executionTrace.length+1,connection:index,pid,applicationName,kind:k,sql,frontendTag:tag,at:performance.now(),dataRows:0,command:null,readyStatus:null,nativeErrorCode:null};executionTrace.push(e);executions.push(e);check(executionTrace.length<=2048,"execution_bound");if(target&&selected===null)selected=e;}
 if(tag==="X")transport.driverTerminateAt=performance.now();upstream.write(frame);
 }}catch{fail();}});
 upstream.on("data",chunk=>{try{back=Buffer.concat([back,chunk]);check(back.length<=1048576,"back_bound");if(sslResponse){if(!back.length)return;check(back[0]===78,"encrypted_profile_not_observed");downstream.write(back.subarray(0,1));back=back.subarray(1);sslResponse=false;}
 while(back.length>=5){const size=back.readUInt32BE(1)+1;check(size>=5&&size<=1048576,"back_frame");if(back.length<size)return;const frame=back.subarray(0,size);back=back.subarray(size);const tag=String.fromCharCode(frame[0]),current=executions[0];
 if(tag==="K"){check(size===13,"backend_key_shape");pid=frame.readInt32BE(5);check(pid>0,"native_pid");owners.add(pid);transport.pid=pid;}
 if(tag==="D"&&current){current.dataRows++;if(current===selected){check(frame.readInt16BE(5)===1,"health_one_column");const n=frame.readInt32BE(7);check(n===1&&frame[11]===49||n===4&&frame.readInt32BE(11)===1,"native_health_one");healthRows++;}}
 if(tag==="C"){check(current!==undefined,"command_mapping");current.command=string(frame,5)[0];current.commandAt=performance.now();if(current===selected)healthCommand=current.command==="SELECT 1";lastCompleted=executions.shift();}
 if(tag==="E"){const code=errorCode(frame);transport.nativeFatalCodes.push({code,at:performance.now()});const e=current??lastCompleted;if(e){e.nativeErrorCode=code;e.errorAt=performance.now();if(current)lastCompleted=executions.shift();}}
 if(tag==="Z"){if(lastCompleted){lastCompleted.readyStatus=String.fromCharCode(frame[5]);lastCompleted.readyAt=performance.now();if(lastCompleted===selected)healthIdle=frame[5]===73;}if(initialReady){transport.nativeInitialReadyAt=performance.now();transport.nativeInitialReadyStatus=String.fromCharCode(frame[5]);initialReady=false;if(mode==="WORKER-04"&&applicationName===ownerLabel&&!omitHold&&!holdReleased){acquisitionHeld=true;held.push({socket:downstream,frame:Buffer.from(frame)});continue;}}lastCompleted=null;}
 if(mode==="WORKER-02"&&applicationName===ownerLabel&&selected!==null&&!omitHold&&!holdReleased){held.push({socket:downstream,frame:Buffer.from(frame)});check(held.reduce((n,v)=>n+v.frame.length,0)<=1048576,"held_bound");}else downstream.write(frame);
 }}catch{fail();}});
 downstream.on("end",()=>transport.downstreamEndAt=performance.now());downstream.on("close",()=>{transport.downstreamCloseAt=performance.now();downstreamClosed++;if(mode==="WORKER-03"&&applicationName===ownerLabel&&!omitHold&&!holdReleased){transport.heldDriverFIN=true;}else upstream.end();});
 upstream.on("end",()=>transport.upstreamEndAt=performance.now());upstream.on("close",()=>{transport.upstreamCloseAt=performance.now();upstreamClosed++;if(mode==="WORKER-02"&&applicationName===ownerLabel&&!omitHold&&!holdReleased)transport.heldEOF=true;else downstream.end();});
 downstream.on("error",(e:any)=>{transport.downstreamError=e.code??null;if(mode==="WORKER-03"&&applicationName===ownerLabel&&!omitHold&&!holdReleased){transport.heldDriverFIN=true;}else upstream.end();});upstream.on("error",(e:any)=>{transport.upstreamError=e.code??null;downstream.end();});
});
await new Promise<void>(resolve=>proxy.listen(0,"127.0.0.1",resolve));const proxied=new URL(direct.href);proxied.port=String((proxy.address()as AddressInfo).port);
function reservePort(){const s=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});const port=s.port;s.stop(true);return port;}
const applicationPort=reservePort();let deploymentPort=reservePort();while(deploymentPort===applicationPort)deploymentPort=reservePort();
async function boundedText(stream:ReadableStream<Uint8Array>){const reader=stream.getReader(),decoder=new TextDecoder();let text="";try{while(true){const p=await reader.read();if(p.done)break;check(text.length+p.value.byteLength<=65536,"output_bound");text+=decoder.decode(p.value,{stream:true});}return text;}finally{reader.releaseLock();}}
const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null","--preload",preload,app],{env:{PATH:Bun.env.PATH??"",DATABASE_URL:proxied.href,PORT:String(applicationPort),JADPO_DEPLOYMENT_PORT:String(deploymentPort)},stdout:"pipe",stderr:"pipe"});const stdout=boundedText(child.stdout),stderr=boundedText(child.stderr);
let watchdog=false,failure:string|null=null,proof:any=null;const watchdogTimer=setTimeout(()=>{watchdog=true;child.kill("SIGKILL");},15000);
const observations:any[]=[],outcomes:any[]=[],privatePools:SQL[]=[],privateOwners:any[]=[],poolEvents:any[]=[],attempts:any[]=[],prototypeEvents:any[]=[];
function safeError(e:any){return{name:typeof e?.name==="string"?e.name:null,code:typeof e?.code==="string"&&/^[A-Z0-9_]{1,80}$/u.test(e.code)?e.code:null,sqlState:typeof e?.errno==="string"&&/^[0-9A-Z]{5}$/u.test(e.errno)?e.errno:null};}
function observe(name:string,p:PromiseLike<any>){const o:any={name,startedAt:performance.now(),settledAt:null,status:"pending"};outcomes.push(o);const done=Promise.resolve(p).then(v=>{o.status="fulfilled";o.settledAt=performance.now();if(Array.isArray(v))o.publicResult=v.map((x:any)=>Array.isArray(x)?x.map((r:any)=>({...r})):{...x});},e=>{o.status="rejected";o.settledAt=performance.now();o.error=safeError(e);});return{o,done};}
async function until(predicate:()=>boolean,ms:number,code:string){const stop=performance.now()+ms;while(!predicate()){check(!watchdog&&protocolFailure===null&&child.exitCode===null&&performance.now()<stop,code);await Bun.sleep(5);}}
function osPresent(pid:number){try{process.kill(pid,0);return true;}catch(e:any){if(e.code==="ESRCH")return false;throw e;}}
async function pidPresent(pid:number){const rows=await control.unsafe("SELECT count(*)::integer AS count FROM pg_stat_activity WHERE pid=$1",[pid]);return{activity:rows[0].count>0,os:osPresent(pid)};}
async function request(port:number,path:string){const r=await fetch(`http://127.0.0.1:${port}${path}`,{signal:AbortSignal.timeout(4000)}),body=await r.json();check(!JSON.stringify(body).includes(database)&&!JSON.stringify(body).includes(proxied.href),"public_leak");return{status:r.status,body,completedAt:performance.now()};}
async function durableState(){const objects=await control.unsafe("SELECT c.relname,c.relkind FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' ORDER BY c.relname,c.relkind"),counts:any[]=[];for(const row of objects.filter(x=>x.relkind==="r")){check(/^[a-z_][a-z0-9_]*$/u.test(row.relname),"table_identity");const r=await control.unsafe(`SELECT count(*)::integer AS count FROM "${row.relname}"`);counts.push({table:row.relname,count:r[0].count});}return{objects,counts};}
async function refused(port:number){return new Promise<any>(resolve=>{const s=createConnection({host:"127.0.0.1",port});let done=false;const finish=(v:any)=>{if(done)return;done=true;s.destroy();resolve(v);};s.on("connect",()=>finish({port,refused:false,code:"CONNECTED"}));s.on("error",(e:any)=>finish({port,refused:e.code==="ECONNREFUSED",code:e.code}));s.setTimeout(300,()=>finish({port,refused:false,code:"TIMEOUT"}));});}
function privatePool(label:string){const u=new URL(proxied.href);u.username=privateRole;const pool=new SQL({url:u.href,max:1,prepare:false,connectionTimeout:2,connection:{application_name:label},onclose:(e:any)=>poolEvents.push({kind:"onclose",label,at:performance.now(),error:safeError(e)})});privatePools.push(pool);poolEvents.push({kind:"constructed",label,at:performance.now()});return pool;}
async function identity(owner:any){const rows=await owner.unsafe("SELECT pg_catalog.pg_backend_pid()::integer AS pid,backend_start::text AS started,application_name AS nonce,current_user AS role,r.rolsuper AS superuser,pg_catalog.pg_has_role(current_user,'pg_signal_backend','MEMBER') AS signal_member FROM pg_catalog.pg_stat_activity a JOIN pg_catalog.pg_roles r ON r.rolname=current_user WHERE a.pid=pg_catalog.pg_backend_pid()");check(rows.length===1,"native_identity_one");check(rows[0].superuser===false&&rows[0].signal_member===false,"native_role_no_new_grant");return{...rows[0]};}
let sibling:any=null,siblingIdentity:any=null,currentOwner:any=null,currentIdentity:any=null,gateReady=true,publishes=0,generations=0;
let initialState:any=null,finalState:any=null;
const workerPath=join(import.meta.dir,"readiness-driver-worker.ts"),workerSHA=createHash("sha256").update(readFileSync(workerPath)).digest("hex");
const omitIPC=Bun.env.JADPO_DRIVER_OMIT_IPC==="1",wrongOwner=Bun.env.JADPO_DRIVER_WRONG_OWNER==="1";
const lateIPC=Bun.env.JADPO_DRIVER_LATE_IPC==="1";
let worker:any=null,workerOutput:Promise<string>|null=null,workerErrors:Promise<string>|null=null,late128:Promise<boolean[]>|null=null;
let originalResponse:Promise<boolean>|null=null,workerExit:any=null,causalRefusal:string|null=null;
const gate=createReadinessGate({ready:()=>gateReady,fatal:()=>false,needsProbe:()=>true,invalidation:()=>0,prepare:()=>null,publish:()=>{publishes++;gateReady=true;prototypeEvents.push({kind:"publish",at:performance.now()});},unavailable:()=>{gateReady=false;prototypeEvents.push({kind:"unavailable",at:performance.now()});},check:async(_recovering,setCancel,deadlineAt)=>{
 const record:any={generation:++generations,origin:deadlineAt-1000,deadlineAt,startedAt:performance.now(),phase:"spawning",healthReceipt:null};attempts.push(record);
 let expired=false,settleReceipt!:(value:boolean)=>void;
 // This promise is the actual owned IPC outcome, raced against the actual exited outcome.
 const receipt=new Promise<boolean>(resolve=>settleReceipt=resolve);
 const u=new URL(proxied.href);u.username=privateRole;
 worker=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",workerPath],{env:{PATH:Bun.env.PATH??"",JADPO_DRIVER_DATABASE_URL:u.href,JADPO_DRIVER_OWNER:ownerLabel,JADPO_DRIVER_LABEL:ownerLabel,JADPO_DRIVER_SOURCE:workerSHA,JADPO_DRIVER_HOLD_IPC:mode==="WORKER-05"&&!omitHold?"1":"",JADPO_DRIVER_OMIT_IPC:omitIPC?"1":"",JADPO_DRIVER_WRONG_OWNER:wrongOwner?"1":""},stdout:"pipe",stderr:"pipe",ipc:(message:any,subprocess:any)=>{
  const at=performance.now(),keys=message&&typeof message==="object"&&!Array.isArray(message)?Object.keys(message).sort().join(','):"";
  const common=message?.version===1&&message.owner===ownerLabel&&message.source===workerSHA&&subprocess===worker;
  if(common&&message.phase==="identity"&&keys==="identity,owner,phase,source,version"){
   const i=message.identity;check(i&&Number.isInteger(i.pid)&&i.pid>0&&i.role===privateRole&&i.nonce===ownerLabel&&i.superuser===false&&i.signal_member===false,"owned_worker_identity");currentIdentity={...i};record.identity={...i};record.identityAt=at;return;
  }
  const accepted=common&&message.phase==="healthy"&&keys==="owner,phase,source,value,version"&&message.value===1;
  prototypeEvents.push({kind:"owned_ipc_message",at,phase:typeof message?.phase==="string"?message.phase:null,accepted,sourceMatched:message?.source===workerSHA,ownerMatched:message?.owner===ownerLabel});
  if(accepted){record.healthReceipt={at,value:message.value,source:workerSHA,owner:ownerLabel};settleReceipt(true);}
 }});
 record.workerPid=worker.pid;record.spawnedAt=performance.now();workerOutput=boundedText(worker.stdout);workerErrors=boundedText(worker.stderr);
 const exited=worker.exited.then((code:number)=>{workerExit={at:performance.now(),pid:worker.pid,code,os:osPresent(worker.pid)};record.exited=workerExit;return false;});
 setCancel(()=>{expired=true;prototypeEvents.push({kind:"cancel_latch",at:performance.now()});
  const joiners=Array.from({length:128},()=>gate.refresh());late128=Promise.all(joiners);observations.push({phase:"late128",at:performance.now(),samePromise:joiners.every(v=>v===originalResponse),count:128,publicAggregatePending:record.finishedAt===undefined,generations,publishes,workerAlive:osPresent(worker.pid)});
  if(!omitTermination&&!(mode==="WORKER-05"&&lateIPC)&&workerExit===null&&worker.exitCode===null){record.killTriggeredAt=performance.now();record.killReason="original_deadline";prototypeEvents.push({kind:"owned_kill_requested",reason:record.killReason,at:record.killTriggeredAt,pid:worker.pid});worker.kill("SIGKILL");}
 });
 const healthy=await Promise.race([receipt,exited]);record.ipcOrProcessSettledAt=performance.now();
 if(workerExit===null&&worker.exitCode===null){record.killTriggeredAt=performance.now();record.killReason="owned_ipc_outcome";prototypeEvents.push({kind:"owned_kill_requested",reason:record.killReason,at:record.killTriggeredAt,pid:worker.pid});worker.kill("SIGKILL");}
 await exited;record.finishedAt=performance.now();record.remainingAtFinish=deadlineAt-record.finishedAt;
 record.phase="actual_owned_IPC_and_exited_aggregate";prototypeEvents.push({kind:"actual_raw_aggregate_settled",at:record.finishedAt});
 return healthy&&!expired&&record.finishedAt<deadlineAt;
}});
function releaseHeld(){holdReleased=true;for(const x of held.splice(0))if(!x.socket.destroyed)x.socket.write(x.frame);for(const t of transports)if(t.heldDriverFIN){const upstream=sockets[(t.connection-1)*2+1];if(!upstream.destroyed)upstream.end();}for(const t of transports)if(t.heldEOF){const socket=sockets[(t.connection-1)*2];if(!socket.destroyed)socket.end();}}
async function nativeResource(){const t=transports.find(v=>v.applicationName===ownerLabel);return{at:performance.now(),identity:currentIdentity?structuredClone(currentIdentity):null,transport:t?structuredClone(t):null,presence:t?.pid?await pidPresent(t.pid):null};}
try{
 let baseline:any=null;const stop=performance.now()+5000;while(performance.now()<stop){try{const r=await request(deploymentPort,"/health/ready");if(r.status===200){baseline=r;break;}}catch{}await Bun.sleep(20);}check(baseline?.status===200&&(await request(applicationPort,"/health/live")).status===200,"unchanged_main_baseline");initialState=await durableState();const mainPids=readFileSync(pidPath,"utf8").trim().split("\n").map(v=>JSON.parse(v));check(mainPids.length===2,"actual_two_main_pids");
 sibling=await privatePool("jadpo.intrinsic."+nonce+".sibling").reserve();privateOwners.push(sibling);siblingIdentity=await identity(sibling);
 const response=gate.refresh();originalResponse=response;void response.then(value=>prototypeEvents.push({kind:"bounded_response",value,at:performance.now()}));const first=Array.from({length:32},()=>gate.refresh());check(first.every(v=>v===response),"32_same_original_response");observations.push({phase:"initial32",samePromise:true,count:32,at:performance.now()});
 if(mode==="WORKER-02"||mode==="WORKER-04"){
  if(!omitHold){await until(()=>mode==="WORKER-02"?healthRows===1&&healthCommand:acquisitionHeld,700,"genuine_native_phase_held");observations.push({phase:"genuine_native_hold",at:performance.now(),heldFrames:held.length,resource:await nativeResource()});}
 }
 const value=await response;observations.push({phase:"bounded_response_observed",at:performance.now(),value,rawPending:attempts[0].finishedAt===undefined,resource:await nativeResource()});
 if(mode==="WORKER-05"&&lateIPC&&!omitHold){observations.push({phase:"actual_owned_IPC_release_after_deadline",at:performance.now(),rawPending:attempts[0].finishedAt===undefined,workerAlive:osPresent(worker.pid)});worker.send({version:1,phase:"release",owner:ownerLabel});}
 if(late128){const values=await late128;check(values.length===128&&values.every(v=>v===false),"actual_late128_false");}
 if(mode==="WORKER-01"||(mode==="WORKER-03")||(mode==="WORKER-05"&&omitHold)){
  if(omitIPC||wrongOwner){if(value!==true)causalRefusal="probe.genuine_health_IPC_required";}else check(value===true,"genuine_health_IPC_before_deadline");
 }else check(value===false,"original_deadline_response_false");
 if(omitTermination&&mode==="WORKER-02"){
  observations.push({phase:"omitted_kill_local_pending",at:performance.now(),workerAlive:osPresent(worker.pid),rawPending:attempts[0].finishedAt===undefined,resource:await nativeResource()});
  check(osPresent(worker.pid)&&attempts[0].finishedAt===undefined,"genuine_omitted_kill_pending");causalRefusal="probe.owned_worker_not_reclaimed_at_deadline";
 }else{await until(()=>attempts[0].finishedAt!==undefined,1000,"actual_child_exit_aggregate");check(workerExit.os===false,"actual_owned_worker_OS_absent");}
 observations.push({phase:"raw_aggregate_or_pending_before_cleanup",at:performance.now(),record:structuredClone(attempts[0]),resource:await nativeResource(),workerExit,publishes,generations});
 if(mode==="WORKER-03"){
  const resource=await nativeResource();observations.push({phase:"local_exited_remote_diagnostic",at:performance.now(),workerExit,resource,strongRemoteQualified:resource.presence?.os===false&&resource.transport?.upstreamCloseAt!==null});
  if(!omitHold){check(resource.presence?.activity===true&&resource.presence.os===true&&resource.transport.heldDriverFIN===true&&resource.transport.upstreamCloseAt===null,"mandatory_remote_owner_counterexample");}
  else causalRefusal="probe.upstream_hold_precondition_omitted";
 }
 if(mode==="WORKER-04")check(!executionTrace.some(v=>v.applicationName===ownerLabel),"cold_worker_no_late_SQL");
 if(mode==="WORKER-05"&&!omitHold){if(lateIPC)check(attempts[0].healthReceipt?.at>=attempts[0].deadlineAt&&publishes===0,"actual_late_IPC_not_published");else check(!attempts[0].healthReceipt&&publishes===0,"withheld_real_IPC_not_published");}else if(mode==="WORKER-05")causalRefusal="probe.genuine_IPC_hold_precondition_omitted";
 const same=await sibling.unsafe("SELECT pg_catalog.pg_backend_pid()::integer AS pid,1 AS value");check(same[0].pid===siblingIdentity.pid&&same[0].value===1,"same_pid_healthy_sibling");observations.push({phase:"same_pid_sibling",at:performance.now(),identity:siblingIdentity,value:same[0].value});await Bun.sleep(1050);
 const ready=await request(deploymentPort,"/health/ready"),live=await request(applicationPort,"/health/live");check(ready.status===200&&live.status===200,"unchanged_main_conservation");check(readFileSync(pidPath,"utf8").trim().split("\n").map(v=>JSON.parse(v)).map(v=>v.pid).join(',')===mainPids.map(v=>v.pid).join(','),"unchanged_main_same_pids");finalState=await durableState();check(JSON.stringify(initialState)===JSON.stringify(finalState),"durable_conservation");observations.push({phase:"unchanged_main_conserved",at:performance.now(),ready,live,pids:mainPids});if(causalRefusal)throw Error(causalRefusal);
}catch(e){failure=e instanceof Error&&e.message.startsWith("probe.")?e.message:"probe.unexpected_fixture_failure";observations.push({phase:"fixture_error",error:safeError(e)});}
finally{
 const fixtureCleanupStartedAt=performance.now();if(worker&&workerExit===null&&worker.exitCode===null){prototypeEvents.push({kind:"manual_fixture_worker_kill",at:performance.now()});worker.kill("SIGKILL");await worker.exited;}if(workerOutput)observations.push({phase:"worker_public_output_after_freeze",stdout:await workerOutput,stderr:await workerErrors});releaseHeld();for(const owner of privateOwners){try{await observe("manual_fixture_reserved_close",owner.close({timeout:0})).done;}catch(e){observations.push({phase:"manual_close_error",error:safeError(e)});}}
 for(const pool of privatePools)observe("manual_fixture_pool_close",pool.close({timeout:0}));if(child.exitCode===null)child.kill("SIGTERM");const nativeExit=await child.exited;
 for(let i=0;i<200;i++){const ids=[...owners];const remain=ids.length?Number((await control.unsafe(`SELECT count(*) AS count FROM pg_stat_activity WHERE pid IN (${ids.map((_,j)=>'$'+(j+1)).join(',')})`,ids))[0].count):0;if(!remain&&downstreamClosed===connections&&upstreamClosed===connections)break;await Bun.sleep(10);}
 const ids=[...owners],backendRemaining=ids.length?Number((await control.unsafe(`SELECT count(*) AS count FROM pg_stat_activity WHERE pid IN (${ids.map((_,j)=>'$'+(j+1)).join(',')})`,ids))[0].count):0;
 const mainProcesses=existsSync(pidPath)?readFileSync(pidPath,"utf8").trim().split("\n").filter(Boolean).map(v=>JSON.parse(v)):[],text=await stdout,errors=await stderr,formerPorts=await Promise.all([applicationPort,deploymentPort].map(refused));const safeNativeOutput=!(text+errors).includes(database)&&!(text+errors).includes(proxied.href)&&!(text+errors).includes(privateRole);
 if(watchdog||nativeExit!==143||backendRemaining!==0||downstreamClosed!==connections||upstreamClosed!==connections||mainProcesses.length!==2||mainProcesses.some(v=>osPresent(v.pid))||formerPorts.some(v=>!v.refused)||!safeNativeOutput||protocolFailure!==null)failure??="probe.native_cleanup_or_safety_failed";
 proof={kind:"process_owner_tests_only",workerSHA,workerExit,omitIPC,wrongOwner,lateIPC,mode,omitTermination,omitHold,bun:Bun.version,attempts,prototypeEvents,poolEvents,outcomes,observations,executionTrace,cancellations,transports,selected,healthRows,healthCommand,healthIdle,acquisitionHeld,generations,publishes,gateReady,initialState,finalState,failure,protocolFailure,fixtureCleanupStartedAt,watchdog,nativeExit,mainProcesses,formerPorts,connections,downstreamClosed,upstreamClosed,backendRemaining,safeNativeOutput,claim:"owned_local_process_and_closed_public_result_IPC_only_remote_ownership_not_qualified_no_product_selection"};clearTimeout(watchdogTimer);for(const s of sockets)s.destroy();proxy.close();await control.close({timeout:0});await admin.unsafe(`DROP DATABASE "${database}" WITH (FORCE)`);await admin.unsafe(`DROP ROLE "${privateRole}"`);await admin.close({timeout:0});rmSync(directory,{recursive:true,force:true});
}
console.log(JSON.stringify(proof));process.exit(failure?1:0);
