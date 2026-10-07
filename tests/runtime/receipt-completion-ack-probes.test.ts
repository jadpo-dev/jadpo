import {afterAll, expect, test} from "bun:test";
import {Database} from "bun:sqlite";
import {SQL} from "bun";
import {cpSync, mkdtempSync, readFileSync, writeFileSync, mkdirSync, existsSync, symlinkSync, rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {pathToFileURL} from "node:url";
import {createConnection,createServer,type Socket,type AddressInfo} from "node:net";
// Tests-only finite private component probes. No public job/profile or recovery
// activation. The untouched checked generator supplies all worker/adapter code.
const repository=new URL('../../',import.meta.url).pathname;
const compiler=Bun.env.JADPO_BIN ?? join(repository,'jadpo/target/debug/jadpo');
const pg=Bun.env.JADPO_OPERATIONAL_PROBE_DATABASE_URL;
if(pg){const u=new URL(pg);if(u.hostname!=='127.0.0.1'||!u.pathname.startsWith('/jadpo_auth_test_'))throw Error('disposable local PostgreSQL required');}
const root=mkdtempSync(join(tmpdir(),'jadpo-rm207-reconcile-'));
cpSync(join(repository,'examples/golden-todo-migration'),root,{recursive:true});
writeFileSync(join(root,'.git'),'isolated component import root\n');mkdirSync(join(root,'tests/assurance'),{recursive:true});cpSync(join(repository,'tests/assurance/service-reference-mail-v0.1.json'),join(root,'tests/assurance/service-reference-mail-v0.1.json'));
const built=Bun.spawnSync([compiler,'build',root],{env:{...Bun.env,DATABASE_URL:'',SQLITE_PATH:''},stdout:'pipe',stderr:'pipe'});
if(built.exitCode!==0)throw Error(new TextDecoder().decode(built.stderr));
const target=join(root,'build/target');
const dependency=join(repository,'build/validation/jwt-dependencies/node_modules');
if(!existsSync(join(dependency,'jose/package.json')))throw Error('pinned JWT dependency unavailable');
symlinkSync(dependency,join(target,'node_modules'),'dir');
const dbPath=join(root,'native.sqlite');
Bun.env.DATABASE_URL=pg??'';Bun.env.SQLITE_PATH=dbPath;Bun.env.NODE_ENV='test';
const href=(name:string)=>pathToFileURL(join(target,name)).href;
const pristine=readFileSync(join(target,'persistence.ts'),'utf8');
const pristineApp=readFileSync(join(target,'app.ts'),'utf8');
// Read-only observation of the real opaque token in the test-owned native module;
// does not mint authority or alter adapter/persistence execution. The original
// module path is required by the real generated authority identity guard.
writeFileSync(join(target,'app.ts'),pristineApp+'\nexport const __observeDeliveryOutcome = (token: unknown) => deliveryOutcomes.get(token as object)?.outcome;\n');
// The persistence module remains untouched, and the test calls no private factory.

const {persistence}=await import(href('persistence.ts'));
const app=await import(href('app.ts'));
const sql=pg?new SQL(pg,{max:3,prepare:false}):null;
const db=pg?null:new Database(dbPath);
const rows=async (q:string,v:any[]=[])=>sql?await sql.unsafe(q,v):db!.prepare(q).all(...v);
const captured:any[]=[];const children:ReturnType<typeof Bun.spawn>[]=[];const serverPorts:number[]=[];
const capture=(v:any)=>{captured.push(v);console.log('RM207_RECONCILE '+JSON.stringify(v));};
const containsCause=(error:any,wanted:any,depth=0):boolean=>depth<8&&(error===wanted||containsCause(error?.cause,wanted,depth+1)||(Array.isArray(error?.errors)&&error.errors.some((child:any)=>containsCause(child,wanted,depth+1))));
const nativeError=(e:any)=>({name:e?.name,code:e?.code,errno:e?.errno,message:e?.message});
async function gone(pid:number){try{process.kill(pid,0);return false;}catch(e:any){return e.code==='ESRCH';}}
async function portClosed(port:number){return new Promise<boolean>(resolve=>{const socket=createConnection({host:'127.0.0.1',port});socket.once('connect',()=>{socket.destroy();resolve(false)});socket.once('error',(e:any)=>resolve(e.code==='ECONNREFUSED'));});}
afterAll(async()=>{
 const settlements=[];for(const c of children){let forced=false;if(!(await gone(c.pid))){forced=true;c.kill('SIGKILL');}const exited=await c.exited;const absent=await gone(c.pid);expect(absent).toBe(true);settlements.push({pid:c.pid,exit:exited,absent,forcedAfterAll:forced});}capture({case:'all_worker_resources',settlements,providerPorts:serverPorts});
 if(sql)await sql.close();db?.close();
 if(Bun.env.JADPO_RECONCILE_CAPTURE){const out=Bun.env.JADPO_RECONCILE_CAPTURE;mkdirSync(out,{recursive:true});writeFileSync(join(out,'observed-app.ts'),pristineApp+'\nexport const __observeDeliveryOutcome = (token: unknown) => deliveryOutcomes.get(token as object)?.outcome;\n');writeFileSync(join(target,'app.ts'),pristineApp);cpSync(join(root,'build'),join(out,'generated'),{recursive:true,filter:q=>!q.includes('node_modules')});writeFileSync(join(out,'observations.json'),JSON.stringify(captured,null,2)+'\n');}
 rmSync(root,{recursive:true,force:true});
});

const configuration={DATABASE_URL:'https://db.example.invalid/component',SESSION_SIGNING_KEY:Buffer.alloc(32,71).toString('base64url'),BROWSER_ORIGIN:'https://todo.example.invalid',OIDC_ISSUER:'https://issuer.example.invalid',OIDC_AUDIENCE:'todo',MAIL_API_KEY:Buffer.alloc(32,72).toString('base64url'),MAIL_SENDER:'todo@example.invalid'};
const binding='job.overdue_reminders.delivery.reminder_v1',at='2026-10-08T08:00:00.000Z';
const profile={executionMs:60000,leaseMs:40000};
async function seed(){
 await app.initializeApplication(configuration);await rows('DELETE FROM "todo"');
 const owner=crypto.randomUUID(),service=crypto.randomUUID(),subject='rm207-'+service,now=new Date().toISOString(),nativeNow=pg?now:Date.parse(now);
 await rows('INSERT INTO "user" (id,authentication_subject,email,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)',[owner,owner,owner+'@example.invalid','active',nativeNow,null]);
 await rows('INSERT INTO service (id,owner_id,name,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)',[service,owner,subject,'active',nativeNow,null]);
 await rows('INSERT INTO reminder_service_membership (id,service_id,role) VALUES ($1,$2,$3)',[crypto.randomUUID(),service,'sender']);
 const issued=await app.authenticationHost().issueServiceCredential('api_bearer',subject,Date.now()+3600000,Date.now());
 const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',issued.credential,Date.now());
 const value={id:crypto.randomUUID(),owner_id:owner,title:'rm207 immutable',status:'open',due_at:'2026-10-06T08:00:00.000Z',reminder_sent_at:null,created_at:now,updated_at:now,deleted_at:null};
 const client=persistence.withOperationTime(at).withPolicy({kind:'user',subject:owner,values:{user_id:owner}},'Todo.patch_todo');await client.create_Todo(value);
 const page=await persistence.withOperationTime(at).transaction((tx:any)=>tx.select_delivery_intents(binding,proof,null));expect(page.intents).toHaveLength(1);
 return{owner,issued,value,intent:page.intents[0],client};
}
function worker(f:any,endpoint:string,body:string,databaseURL=pg??''){const code=`const app=await import(${JSON.stringify(href('app.ts'))});const {persistence}=await import(${JSON.stringify(href('persistence.ts'))});const adapter=await import(${JSON.stringify(href('service-adapter.ts'))});await app.initializeApplication(${JSON.stringify(configuration)});adapter.setReferenceMailEndpointForTesting(${JSON.stringify(endpoint)});const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',process.env.PROBE_WORKER_KEY,Date.now());const binding=${JSON.stringify(binding)},at=${JSON.stringify(at)},profile=${JSON.stringify(profile)};${body}`;const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null','-e',code],{env:{...Bun.env,DATABASE_URL:databaseURL,SQLITE_PATH:dbPath,PROBE_WORKER_KEY:f.issued.credential,NODE_ENV:'test'},stdin:'pipe',stdout:'pipe',stderr:'pipe'});children.push(child);return{child,done:Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited])};}
async function finish(w:ReturnType<typeof worker>){const[out,err,exit]=await w.done;expect(exit,err).toBe(0);expect(await gone(w.child.pid)).toBe(true);return JSON.parse(out.trim().split('\n').at(-1)!);}
async function provider(work:(calls:any[],endpoint:string)=>Promise<void>){const calls:any[]=[];const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(r){const body=await r.json();const claim=(await rows('SELECT possible_dispatch FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1',[body.idempotency_key]))[0];expect(Number(claim.possible_dispatch)).toBe(1);calls.push(body);return Response.json({accepted_at:'2026-10-08T08:00:00.000Z'},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});}});serverPorts.push(server.port);try{await work(calls,`http://127.0.0.1:${server.port}`);}finally{const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);}}
async function receipts(id:string){const meta=pg?await rows("SELECT to_regclass('\"__jadpo_delivery_mail_receipts_v1\"') relation"):await rows("SELECT name FROM sqlite_master WHERE name='__jadpo_delivery_mail_receipts_v1'");const present=pg?meta[0].relation!==null:meta.length>0;return present?await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[id]):[];}
async function claims(id:string){return rows('SELECT * FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1',[id]);}
async function marker(path:string){for(let i=0;i<500;i++){if(existsSync(path))return JSON.parse(readFileSync(path,'utf8'));await Bun.sleep(10);}throw Error('worker token checkpoint absent');}


// Actual native PG wire boundary, armed only after the worker has a validated
// adapter receipt and all startup/admission transactions have acknowledged.
async function completionProxy(){
 if(!pg)return null;
 const direct=new URL(pg),sockets:Socket[]=[],connections:any[]=[];
 let armed=false,dropped=false,commits=0;
 const server=createServer(down=>{
  const up=createConnection({host:direct.hostname,port:Number(direct.port||5432)});
  sockets.push(down,up);const state={connection:connections.length+1,pid:null as number|null,awaitingCommit:false,commandComplete:false,readyIdle:false,frontendCommits:0,closed:false};connections.push(state);
  let front=Buffer.alloc(0),back=Buffer.alloc(0),held=Buffer.alloc(0);
  down.on('data',chunk=>{
   if(armed){front=Buffer.concat([front,chunk]);let offset=0;
    while(offset+5<=front.length){const length=front.readInt32BE(offset+1);if(length<4||length>1048576)throw Error('bounded frontend frame invalid');if(offset+1+length>front.length)break;
     const tag=front[offset],payload=front.subarray(offset+5,offset+1+length);let text='';
     if(tag===81)text=payload.subarray(0,payload.indexOf(0)).toString();
     if(tag===80){const first=payload.indexOf(0);text=payload.subarray(first+1,payload.indexOf(0,first+1)).toString();}
     if(text.trim().replace(/;$/u,'').toUpperCase()==='COMMIT'){commits++;state.frontendCommits++;state.awaitingCommit=true;held=Buffer.alloc(0);}
     offset+=1+length;
    }front=front.subarray(offset);
   }up.write(chunk);
  });
  up.on('data',chunk=>{
   if(state.awaitingCommit&&!dropped)held=Buffer.concat([held,chunk]);
   back=Buffer.concat([back,chunk]);let offset=0;
   while(offset+5<=back.length){const length=back.readInt32BE(offset+1);if(length<4||length>1048576)throw Error('bounded backend frame invalid');if(offset+1+length>back.length)break;
    const tag=back[offset],payload=back.subarray(offset+5,offset+1+length);
    if(tag===75&&payload.length===8)state.pid=payload.readInt32BE(0); // actual BackendKeyData, secret never captured
    if(state.awaitingCommit&&tag===67&&payload.toString()==='COMMIT\0')state.commandComplete=true;
    if(state.awaitingCommit&&state.commandComplete&&tag===90&&payload.toString()==='I')state.readyIdle=true;
    offset+=1+length;
   }back=back.subarray(offset);
   if(state.awaitingCommit&&!dropped){
    if(state.commandComplete&&state.readyIdle){dropped=true;expect(state.pid).toBeNumber();down.destroy();up.destroy();held=Buffer.alloc(0);}return;
   }down.write(chunk);
  });
  down.on('error',()=>{});up.on('error',()=>{});down.on('close',()=>{up.destroy();state.closed=true;});up.on('close',()=>down.destroy());
 });
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));const port=(server.address()as AddressInfo).port;const url=new URL(pg);url.port=String(port);
 return{url:url.toString(),port,arm(){expect(commits).toBe(0);expect(dropped).toBe(false);armed=true;},proof(){return{armed,dropped,commits,connections}},async close(){for(const socket of sockets)socket.destroy();await new Promise<void>(resolve=>server.close(()=>resolve()));expect(await portClosed(port)).toBe(true);expect(connections.every(c=>c.closed)).toBe(true);}};
}

const modes=['current','superseded-B','ABA','disabled-owner','deleted-source','normal'] as const;
const selectedMode=Bun.env.JADPO_RECONCILE_MODE;
if(selectedMode&&!modes.includes(selectedMode as any))throw Error('unknown fixture mode');
for(const mode of modes)test.skipIf(!!selectedMode&&selectedMode!==mode)(`final completion ACK ${mode} preserves committed receipt across distinct restart without resend`,async()=>{
 const f=await seed();await provider(async(calls,endpoint)=>{
  const causal=Bun.env.JADPO_RECONCILE_CAUSAL??'',proxy=mode==='normal'?null:await completionProxy(),checkpoint=join(root,'accepted-'+mode+'.json');
  // The same exact generated app and persistence modules execute all stages.
  const body=`const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);if(!token)throw Error('admission refused');
   const receipt=${JSON.stringify(causal)}==='omit-dispatch'?Object.freeze({}):await app.dispatchDeliveryInvocation(token);
   await Bun.write(${JSON.stringify(checkpoint)},JSON.stringify({pid:process.pid,outcome:app.__observeDeliveryOutcome(receipt)??null}));
   const gate=await Bun.stdin.stream().getReader().read();const command=gate.done?'':new TextDecoder().decode(gate.value).trim();if(command==='ABORT'){console.log(JSON.stringify({pid:process.pid,fixtureAborted:true}));process.exit(0);}if(command!=='COMPLETE')throw Error('completion admission absent');
   const original=(await import('bun:sqlite')).Database.prototype.exec;let nativeCommit=0,injectedAck=false;
   if(!process.env.DATABASE_URL&&${JSON.stringify(mode)}!=='normal'&&${JSON.stringify(causal)}!=='omit-ack') (await import('bun:sqlite')).Database.prototype.exec=function(q){const result=Reflect.apply(original,this,[q]);if(q==='COMMIT'){nativeCommit++;injectedAck=true;throw Object.assign(new Error('test-owned completion ACK lost after real COMMIT'),{code:'SQLITE_IOERR'});}return result;};
   let completed=null,fault=null;try{if(${JSON.stringify(causal)}!=='omit-completion')completed=await app.completeDeliveryInvocation(receipt);}catch(e){fault={name:e.name,kind:e.kind,operation:e.operation,transaction:e.transaction};}finally{(await import('bun:sqlite')).Database.prototype.exec=original;}
   console.log(JSON.stringify({pid:process.pid,completed,fault,nativeCommit,injectedAck}));process.exit(0);`;
  const w=worker(f,endpoint,body,proxy?.url??pg??'');let ready:any,result:any;
  try{
   ready=await marker(checkpoint);capture({case:'receipt_checkpoint',mode,ready,calls});expect(ready.pid).toBe(w.child.pid);expect(ready.outcome?.kind).toBe('accepted');expect(ready.outcome.receipt).toEqual({accepted_at:'2026-10-08T08:00:00.000Z'});expect(calls).toEqual([f.intent.payload]);
   const initial=await persistence.read_delivery_intent(f.intent.intentId),beforeClaim=await claims(f.intent.intentId);expect(initial.state).toBe('running');expect(await receipts(f.intent.intentId)).toEqual([]);expect(Number(beforeClaim[0].possible_dispatch)).toBe(1);expect(Number(beforeClaim[0].invocations)).toBe(1);
   if((mode==='superseded-B'||mode==='ABA')&&causal!=='omit-supersede'){
    await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:'2026-10-07T08:00:00.000Z'},null);
    if(mode==='ABA')await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:f.value.due_at},null);
   }
   if(mode==='disabled-owner')await rows('UPDATE "user" SET status=$1 WHERE id=$2',['disabled',f.owner]);
   if(mode==='deleted-source')await rows('DELETE FROM todo WHERE id=$1',[f.value.id]);
   const sourceBefore=await rows('SELECT * FROM todo WHERE id=$1',[f.value.id]);const sourceRevision=await persistence.transaction((tx:any)=>tx.read_delivery_schedule_revision('Todo',f.value.id));
   if(proxy&&causal!=='omit-ack')proxy.arm();
   w.child.stdin.write('COMPLETE\n');w.child.stdin.end();result=await finish(w);
   capture({case:'completion_result_before_assertion',mode,result,wire:proxy?.proof()??null});
   if(mode==='normal'){expect(result.completed).toBe(true);expect(result.fault).toBeNull();}
   else{expect(result.completed).toBeNull();expect(result.fault?.kind).toBe('unknown');expect(result.fault.transaction).toMatchObject({outcome:'unknown',commitAcknowledged:false,retryableCause:null});if(pg){const proof=proxy!.proof();expect(proof.commits).toBe(1);expect(proof.dropped).toBe(true);expect(proof.connections.filter(c=>c.commandComplete&&c.readyIdle&&c.frontendCommits===1)).toHaveLength(1);}else{expect(result.nativeCommit).toBe(1);expect(result.injectedAck).toBe(true);}}
   const intent=await persistence.read_delivery_intent(f.intent.intentId),receipt=await receipts(f.intent.intentId),claim=await claims(f.intent.intentId),source=await rows('SELECT * FROM todo WHERE id=$1',[f.value.id]);
   expect(intent).toEqual({...initial,state:'succeeded'});expect(receipt).toHaveLength(1);expect(receipt[0].intent_id).toBe(f.intent.intentId);expect(receipt[0].claim_id).toBe(beforeClaim[0].claim_id);expect(String(receipt[0].generation)).toBe(String(beforeClaim[0].generation));expect(receipt[0].payload_version).toBe(f.intent.payloadVersion);expect(receipt[0].accepted_at).toBe(ready.outcome.receipt.accepted_at);expect(receipt[0].observed_at).toBe(ready.outcome.observedAt);expect(claim).toEqual(beforeClaim);expect(calls).toHaveLength(1);
   if(mode==='deleted-source')expect(source).toEqual([]);
   else if(mode==='superseded-B'||mode==='ABA')expect(source).toEqual(sourceBefore);
   else{expect(new Date(source[0].reminder_sent_at).toISOString()).toBe(receipt[0].observed_at);expect(source[0].created_at).toEqual(sourceBefore[0].created_at);}
   expect(sourceRevision).toBe(mode==='superseded-B'?'2':mode==='ABA'?'3':'1');
   if(mode==='disabled-owner')expect((await rows('SELECT status FROM "user" WHERE id=$1',[f.owner]))[0].status).toBe('disabled');
   const next=worker(f,endpoint,`const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);console.log(JSON.stringify({pid:process.pid,token:token!==null,intent:await persistence.read_delivery_intent(${JSON.stringify(f.intent.intentId)})}));process.exit(0);`);const restarted=await finish(next);
   expect(restarted.pid).not.toBe(ready.pid);expect(restarted.token).toBe(false);expect(restarted.intent).toEqual(intent);expect(calls).toHaveLength(1);expect(await receipts(f.intent.intentId)).toEqual(receipt);expect(await claims(f.intent.intentId)).toEqual(claim);expect(await rows('SELECT * FROM todo WHERE id=$1',[f.value.id])).toEqual(source);
   capture({case:'completion_ack_restart',database:pg?'postgres':'sqlite',mode,firstPID:ready.pid,restartPID:restarted.pid,providerCalls:calls,initial,preCompletionClaim:beforeClaim,sourceBefore,sourceRevision,intent,receipt,claim,source,wire:proxy?.proof()??null,qualification:mode==='normal'?'normal native completion without ACK fault; same read-only opaque outcome observer':pg?'actual exact final COMMIT CommandComplete plus ReadyForQuery idle withheld; no startup/admission ACK interception':'actual SQLite COMMIT then explicitly injected acknowledgement exception; no wire-loss claim'});
  }finally{let fixtureAbort=false,forcedKill=false;if(!(await gone(w.child.pid))){fixtureAbort=true;try{w.child.stdin.write('ABORT\n');w.child.stdin.end();}catch{}const settled=await Promise.race([w.child.exited.then(()=>true),Bun.sleep(1500).then(()=>false)]);if(!settled){forcedKill=true;w.child.kill('SIGKILL');}}const[stdout,stderr,exit]=await w.done;capture({case:'completion_worker_settlement',mode,pid:w.child.pid,exit,absent:await gone(w.child.pid),fixtureAbort,forcedKill,stdout,stderr});expect(forcedKill).toBe(false);await proxy?.close();if(mode==='disabled-owner')await rows('UPDATE "user" SET status=$1 WHERE id=$2',['active',f.owner]);}
 });
},20000);

test.skipIf(!!selectedMode)('true adapter receipt before completion dies with worker; fresh worker retains unknown/no resend and blocks current/ABA ordering key',async()=>{
 const f=await seed();await provider(async(calls,endpoint)=>{
  const checkpoint=join(root,'accepted-token.json');const first=worker(f,endpoint,`const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);if(token===null)throw Error('admission refused');const receipt=await app.dispatchDeliveryInvocation(token);await Bun.write(${JSON.stringify(checkpoint)},JSON.stringify({adapterToken:true,pid:process.pid,actualOutcome:app.__observeDeliveryOutcome(receipt)}));await new Response(Bun.stdin).text();process.exit(0);`);
  try{
   const entered=await marker(checkpoint);capture({case:'adapter_checkpoint_before_assertion',entered,providerCalls:calls.length,durableReceipts:(await receipts(f.intent.intentId)).length});expect(entered.pid).toBe(first.child.pid);expect(entered.adapterToken).toBe(true);expect(entered.actualOutcome.kind).toBe('accepted');expect(entered.actualOutcome.receipt).toMatchObject({accepted_at:'2026-10-08T08:00:00.000Z'});expect(calls).toEqual([f.intent.payload]);
   const running=await persistence.read_delivery_intent(f.intent.intentId);const claim=await claims(f.intent.intentId);expect(running.state).toBe('running');expect(await receipts(f.intent.intentId)).toEqual([]);expect(Number(claim[0].possible_dispatch)).toBe(1);expect(Number(claim[0].invocations)).toBe(1);expect((await rows('SELECT reminder_sent_at FROM todo WHERE id=$1',[f.value.id]))[0].reminder_sent_at).toBeNull();
   first.child.kill('SIGKILL');expect(await first.child.exited).toBe(137);await first.done;expect(await gone(first.child.pid)).toBe(true);
   // Explicit fixture expiry changes lease only, not authority/payload/budget.
   await rows('UPDATE "__jadpo_delivery_claims_v1" SET lease_until=$1 WHERE intent_id=$2',['2000-01-01T00:00:00.000Z',f.intent.intentId]);
   const second=worker(f,endpoint,`const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);console.log(JSON.stringify({pid:process.pid,token:token!==null,intent:await persistence.read_delivery_intent(${JSON.stringify(f.intent.intentId)})}));process.exit(0);`);const resumed=await finish(second);capture({case:'restart_before_assertion',resumed,providerCalls:calls.length});expect(resumed.pid).not.toBe(first.child.pid);expect(resumed.token).toBe(false);expect(resumed.intent.state).toBe('outcome_unknown');expect(resumed.intent.payload).toEqual(f.intent.payload);expect(calls).toHaveLength(1);expect(await receipts(f.intent.intentId)).toEqual([]);
   const unknown=await persistence.read_delivery_intent(f.intent.intentId),immutableClaims=await claims(f.intent.intentId);const blocked:any[]=[];
   for(const due of ['2026-10-07T08:00:00.000Z',f.value.due_at]){
    await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:due},null);
    const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',f.issued.credential,Date.now());const page=await persistence.withOperationTime(at).transaction((tx:any)=>tx.select_delivery_intents(binding,proof,null));const next=page.intents[0];expect(next.intentId).not.toBe(f.intent.intentId);expect(next.orderingKey).toBe(f.intent.orderingKey);
    const nextWorker=worker(f,endpoint,`const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(next.intentId)},at,profile);console.log(JSON.stringify({token:token!==null}));process.exit(0);`);expect(await finish(nextWorker)).toEqual({token:false});blocked.push(next);
   }
   expect(calls).toHaveLength(1);expect(await persistence.read_delivery_intent(f.intent.intentId)).toEqual(unknown);expect(await claims(f.intent.intentId)).toEqual(immutableClaims);expect(await receipts(f.intent.intentId)).toEqual([]);
   capture({case:'accepted_before_completion_crash',database:pg?'postgres':'sqlite',providerAcceptance:calls,actualAcceptedOutcome:entered.actualOutcome,firstPID:first.child.pid,restartPID:second.child.pid,running,preCrashClaim:claim,durableReceiptsBeforeCrash:0,resumed:unknown,blockedRevisionIntents:blocked,qualification:'read-only private observer witnesses actual accepted outcome/typed adapter receipt in memory only; SIGKILL and lease expiry injected; no durable receipt reconciliation authority'});
  }finally{if(!(await gone(first.child.pid)))first.child.kill('SIGKILL');const[out,err,exit]=await first.done;capture({case:'crash_worker_settlement',pid:first.child.pid,exit,stdout:out,stderr:err});}
 });
},20000);

