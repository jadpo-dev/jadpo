import {afterAll, expect, test} from "bun:test";
import {cpSync, mkdtempSync, mkdirSync, writeFileSync, symlinkSync, rmSync, readFileSync, existsSync} from "node:fs";
import {Database} from "bun:sqlite";
import {SQL} from "bun";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {createConnection,createServer,type Socket,type AddressInfo} from "node:net";
import {pathToFileURL} from "node:url";
// The actual checked native application owns every issuer and persistence seam.
const control=Bun.env.JADPO_RECONCILIATION_CONTROL??'';
const repository=new URL("../../",import.meta.url).pathname;
const root=mkdtempSync(join(tmpdir(),"jadpo-reconcile02-"));
cpSync(join(repository,"examples/golden-todo-migration"),root,{recursive:true});
writeFileSync(join(root,".git"),"isolated checked import root\n");
mkdirSync(join(root,"tests/assurance"),{recursive:true});
cpSync(join(repository,"tests/assurance/service-reference-mail-v0.1.json"),join(root,"tests/assurance/service-reference-mail-v0.1.json"));
const compiler=Bun.env.JADPO_BIN??join(repository,"jadpo/target/debug/jadpo");
if(!existsSync(compiler))throw Error("Build the compiler or set JADPO_BIN before native reconciliation tests");
const build=Bun.spawnSync([compiler,"build",root],{env:{...Bun.env,DATABASE_URL:"",SQLITE_PATH:""},stdout:"pipe",stderr:"pipe"});
if(build.exitCode!==0)throw Error(new TextDecoder().decode(build.stderr));
const target=join(root,"build/target");
const dependency=Bun.env.JADPO_JWT_DEPENDENCIES??join(repository,"build/validation/jwt-dependencies/node_modules");
if(!existsSync(join(dependency,"jose/package.json")))throw Error("Install the compiler-pinned JWT dependency before native reconciliation tests");
symlinkSync(dependency,join(target,"node_modules"),"dir");
const pg=Bun.env.JADPO_RECONCILIATION_DATABASE_URL;
if(pg){const url=new URL(pg);if(url.hostname!=="127.0.0.1"||!url.pathname.startsWith("/jadpo_auth_test_"))throw Error("disposable local PostgreSQL required");}
Bun.env.DATABASE_URL=pg??"";Bun.env.SQLITE_PATH=join(root,"native.sqlite");Bun.env.NODE_ENV="test";
const app=await import(pathToFileURL(join(target,"app.ts")).href);
const href=(name:string)=>pathToFileURL(join(target,name)).href;
const children:ReturnType<typeof Bun.spawn>[]=[];
async function gone(pid:number){try{process.kill(pid,0);return false;}catch(error:any){return error.code==="ESRCH";}}
async function portClosed(port:number){return new Promise<boolean>(resolve=>{const socket=createConnection({host:"127.0.0.1",port});socket.once("connect",()=>{socket.destroy();resolve(false);});socket.once("error",(error:any)=>resolve(error.code==="ECONNREFUSED"));});}
const {persistence}=await import(pathToFileURL(join(target,"persistence.ts")).href);
const adapter=await import(pathToFileURL(join(target,"service-adapter.ts")).href);
const sql=pg?new SQL(pg,{max:3,prepare:false}):null;const db=pg?null:new Database(Bun.env.SQLITE_PATH!);
const rows=async(q:string,v:any[]=[])=>sql?await sql.unsafe(q,v):db!.prepare(q).all(...v);
afterAll(async()=>{const resources=[];for(const child of children){const absent=await gone(child.pid);expect(absent).toBe(true);resources.push({pid:child.pid,absent,exit:await child.exited});}await sql?.close();db?.close();if(Bun.env.JADPO_RECONCILIATION_CAPTURE){const out=Bun.env.JADPO_RECONCILIATION_CAPTURE;mkdirSync(out,{recursive:true});cpSync(join(root,"build"),join(out,"generated"),{recursive:true,filter:path=>!path.includes("node_modules")});writeFileSync(join(out,"worker-resources.json"),JSON.stringify({resources,qualification:"all tracked children already gone; no afterAll kill fallback"},null,2)+"\n");}rmSync(root,{recursive:true,force:true});});
test("checked native reconciliation has a purpose-sealed factory, distinct from no-effect replay",()=>{
 expect(typeof app.createDeliveryReceiptReconciler).toBe("function");
 expect(typeof app.readCurrentDeliveryReceiptReconciliationGrant).toBe("function");
 expect(typeof app.readCurrentDeliveryAcceptedEvidence).toBe("function");
});

const configuration={DATABASE_URL:"https://db.example.invalid/component",SESSION_SIGNING_KEY:Buffer.alloc(32,71).toString("base64url"),BROWSER_ORIGIN:"https://todo.example.invalid",OIDC_ISSUER:"https://issuer.example.invalid",OIDC_AUDIENCE:"todo",MAIL_API_KEY:Buffer.alloc(32,72).toString("base64url"),MAIL_SENDER:"todo@example.invalid"};
const binding="job.overdue_reminders.delivery.reminder_v1",at="2026-10-08T08:00:00.000Z",profile={executionMs:4000,leaseMs:1500};
async function seed(){
 await app.initializeApplication(configuration);await rows('DELETE FROM todo');
 const owner=crypto.randomUUID(),service=crypto.randomUUID(),subject="reconcile-"+service,now=new Date().toISOString(),nativeNow=pg?now:Date.parse(now);
 await rows('INSERT INTO "user" (id,authentication_subject,email,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)',[owner,owner,owner+"@example.invalid","active",nativeNow,null]);
 await rows('INSERT INTO service (id,owner_id,name,status,created_at,disabled_at) VALUES ($1,$2,$3,$4,$5,$6)',[service,owner,subject,"active",nativeNow,null]);
 await rows('INSERT INTO reminder_service_membership (id,service_id,role) VALUES ($1,$2,$3)',[crypto.randomUUID(),service,"sender"]);
 const issued=await app.authenticationHost().issueServiceCredential("api_bearer",subject,Date.now()+3600000,Date.now());
 const proof=await app.authenticationHost().prepareDeliveryCredential("api_bearer",issued.credential,Date.now());
 const value={id:crypto.randomUUID(),owner_id:owner,title:"original immutable",status:"open",due_at:"2026-10-06T08:00:00.000Z",reminder_sent_at:null,created_at:now,updated_at:now,deleted_at:null};
 const client=persistence.withOperationTime(at).withPolicy({kind:"user",subject:owner,values:{user_id:owner}},"Todo.patch_todo");await client.create_Todo(value);
 const page=await persistence.withOperationTime(at).transaction((tx:any)=>tx.select_delivery_intents(binding,proof,null));expect(page.intents).toHaveLength(1);
 return{owner,proof,value,client,issued,intent:page.intents[0]};
}
test("native reconciliation purpose refuses worker, replay, cloned and withdrawn provenance",async()=>{
 await app.initializeApplication(configuration);const host=app.authenticationHost(),grant=host.issueDeliveryReceiptReconciliationGrant(),replay=host.issueDeliveryOperatorGrant();
 let getters=0;expect(()=>app.createDeliveryReceiptReconciler(binding,grant,{get executionMs(){getters++;return 4000;},leaseMs:1500})).toThrow();expect(getters).toBe(0);
 const {createFirstPartyAuthentication}=await import(href('first-party-authentication.ts'));const {authenticationStorage}=await import(href('persistence.ts'));const unpublished=await createFirstPartyAuthentication([],authenticationStorage,async()=>({kind:'unresolved'}));expect(()=>app.createDeliveryReceiptReconciler(binding,unpublished.issueDeliveryReceiptReconciliationGrant(),profile)).toThrow();
 const reconciler=app.createDeliveryReceiptReconciler(binding,grant,profile);expect(Object.keys(reconciler)).toEqual(["reconcile"]);
 for(const wrong of [{},structuredClone(grant),replay,null])expect(()=>app.createDeliveryReceiptReconciler(binding,wrong,profile)).toThrow();
 host.revokeDeliveryReceiptReconciliationGrant(grant);await expect(reconciler.reconcile(crypto.randomUUID())).rejects.toThrow();
 const old=host.issueDeliveryReceiptReconciliationGrant();await app.initializeApplication(configuration);expect(()=>app.createDeliveryReceiptReconciler(binding,old,profile)).toThrow();
});
test("absent proof consumer performs no schema creation or state inference",async()=>{
 const f=await seed(),grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant();
 const catalog=()=>rows(pg?"SELECT relname FROM pg_class WHERE relname LIKE '__jadpo_delivery%' ORDER BY relname":"SELECT name FROM sqlite_master WHERE type='table' AND name LIKE '__jadpo_delivery%' ORDER BY name");
 const before=await catalog(),intent=await persistence.read_delivery_intent(f.intent.intentId);
 expect(await app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId)).toBe(false);
 expect(await catalog()).toEqual(before);expect(await persistence.read_delivery_intent(f.intent.intentId)).toEqual(intent);
 for(const raw of [{},{receipt:{accepted_at:at}},f.proof,app.authenticationHost().issueDeliveryOperatorGrant(),grant]){
  await expect(persistence.transaction((tx:any)=>tx.record_delivery_acceptance_proof(binding,raw))).rejects.toThrow();
 }
 expect(await catalog()).toEqual(before);expect(await persistence.read_delivery_intent(f.intent.intentId)).toEqual(intent);
});
test("actual adapter acceptance journals original proof; elapsed lease reconciles without a new invocation",async()=>{
 const f=await seed(),calls:any[]=[];
 const server=Bun.serve({hostname:"127.0.0.1",port:0,async fetch(request){const payload=await request.json();calls.push(payload);return Response.json({accepted_at:"2026-10-08T08:00:00.000Z"},{status:202,headers:{"Idempotency-Key":payload.idempotency_key}});}});
 try{
  adapter.setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);
  const invocation=await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile);expect(invocation).not.toBeNull();
  const outcome=await app.dispatchDeliveryInvocation(invocation);expect(outcome).not.toBeNull();
  const proofs=await rows('SELECT *, CAST(generation AS TEXT) AS generation, CAST(key_sequence AS TEXT) AS key_sequence FROM "__jadpo_delivery_acceptance_proofs_v1" WHERE intent_id=$1',[f.intent.intentId]);
  expect(proofs).toHaveLength(1);const proof=proofs[0];expect(proof.intent_id).toBe(f.intent.intentId);expect(proof.source_revision).toBe(f.intent.sourceRevision);expect(JSON.parse(proof.payload)).toEqual(f.intent.payload);expect(proof.accepted_at).toBe("2026-10-08T08:00:00.000Z");expect(Number(proof.lease_ms)).toBe(1500);expect(calls).toEqual([f.intent.payload]);
  const originalClaim=await rows('SELECT * FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1',[f.intent.intentId]);
  await Bun.sleep(1600);expect(await app.completeDeliveryInvocation(outcome)).toBe(false);
  const grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant();const reconciler=app.createDeliveryReceiptReconciler(binding,grant,profile);
  expect(await reconciler.reconcile(f.intent.intentId)).toBe(true);expect(await reconciler.reconcile(f.intent.intentId)).toBe(true);
  expect((await persistence.read_delivery_intent(f.intent.intentId)).state).toBe("succeeded");expect(await rows('SELECT * FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1',[f.intent.intentId])).toEqual(originalClaim);
  expect(await rows('SELECT *, CAST(generation AS TEXT) AS generation, CAST(key_sequence AS TEXT) AS key_sequence FROM "__jadpo_delivery_acceptance_proofs_v1" WHERE intent_id=$1',[f.intent.intentId])).toEqual(proofs);
  const receipts=await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId]);expect(receipts).toHaveLength(1);expect(receipts[0].observed_at).toBe(proof.observed_at);
  expect(new Date((await rows('SELECT reminder_sent_at FROM todo WHERE id=$1',[f.value.id]))[0].reminder_sent_at).toISOString()).toBe(proof.observed_at);expect(calls).toHaveLength(1);
  console.log("RECONCILE02_NATIVE "+JSON.stringify({database:pg?"postgres":"sqlite",qualification:"real elapsed matching private 4000/1500 fixture; no canonical 40s expiry claim",proof,originalClaim,receipts,providerCalls:calls.length}));
 }finally{server.stop(true);}
},15000);

async function acceptedFixture(work:(f:any,calls:any[])=>Promise<void>){
 const f=await seed(),calls:any[]=[];
 const server=Bun.serve({hostname:"127.0.0.1",port:0,async fetch(request){const payload=await request.json();calls.push(payload);return Response.json({accepted_at:at},{status:202,headers:{"Idempotency-Key":payload.idempotency_key}});}});
 try{adapter.setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);const token=await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile);expect(token).not.toBeNull();f.outcome=await app.dispatchDeliveryInvocation(token);expect(f.outcome).not.toBeNull();expect(calls).toEqual([f.intent.payload]);await work(f,calls);}finally{server.stop(true);}
}
async function snapshot(f:any){return{proof:await rows('SELECT * FROM "__jadpo_delivery_acceptance_proofs_v1" WHERE intent_id=$1',[f.intent.intentId]),claim:await rows('SELECT * FROM "__jadpo_delivery_claims_v1" WHERE intent_id=$1',[f.intent.intentId]),intent:await persistence.read_delivery_intent(f.intent.intentId),source:await rows('SELECT * FROM todo WHERE id=$1',[f.value.id])};}
test("malformed existing proof schema refuses read-only without repairing durable data",async()=>{
 await ensureProofFixture();const f=await seed(),grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant();
 await rows('ALTER TABLE "__jadpo_delivery_acceptance_proofs_v1" RENAME TO "__jadpo_acceptance_fixture_preserved"');
 try{
  await rows('CREATE TABLE "__jadpo_delivery_acceptance_proofs_v1" (intent_id TEXT PRIMARY KEY, schema_version INTEGER)');
  await rows('INSERT INTO "__jadpo_delivery_acceptance_proofs_v1" VALUES ($1,$2)',[f.intent.intentId,1]);
  const before=await rows('SELECT * FROM "__jadpo_delivery_acceptance_proofs_v1"');const intent=await persistence.read_delivery_intent(f.intent.intentId);
  await expect(app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId)).rejects.toThrow();
  expect(await rows('SELECT * FROM "__jadpo_delivery_acceptance_proofs_v1"')).toEqual(before);expect(await persistence.read_delivery_intent(f.intent.intentId)).toEqual(intent);
 }finally{await rows('DROP TABLE "__jadpo_delivery_acceptance_proofs_v1"');await rows('ALTER TABLE "__jadpo_acceptance_fixture_preserved" RENAME TO "__jadpo_delivery_acceptance_proofs_v1"');}
});
test("durable proof corruption and mismatched immutable profile refuse unchanged",async()=>acceptedFixture(async(f,calls)=>{
 const grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),before=await snapshot(f);
 await expect(app.createDeliveryReceiptReconciler(binding,grant,{executionMs:4000,leaseMs:1000}).reconcile(f.intent.intentId)).rejects.toThrow();expect(await snapshot(f)).toEqual(before);
 for(const [field,value] of [["accepted_at","2026-02-30T00:00:00.000Z"],["binding_contract_sha256","0".repeat(64)],["payload",JSON.stringify({...f.intent.payload,todo_title:"corrupted"})]]){
  if(control!=='omit-proof-corruption')await rows(`UPDATE "__jadpo_delivery_acceptance_proofs_v1" SET "${field}"=$1 WHERE intent_id=$2`,[value,f.intent.intentId]);const corrupt=await snapshot(f);
  await expect(app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId)).rejects.toThrow();expect(await snapshot(f)).toEqual(corrupt);
  await rows(`UPDATE "__jadpo_delivery_acceptance_proofs_v1" SET "${field}"=$1 WHERE intent_id=$2`,[before.proof[0][field],f.intent.intentId]);
 }
 if(!pg){await rows('PRAGMA ignore_check_constraints=ON');try{await rows('UPDATE "__jadpo_delivery_acceptance_proofs_v1" SET generation=$1 WHERE intent_id=$2',["0x10",f.intent.intentId]);}finally{await rows('PRAGMA ignore_check_constraints=OFF');}
  const corrupt=await snapshot(f);expect((await rows('SELECT typeof(generation) storage FROM "__jadpo_delivery_acceptance_proofs_v1" WHERE intent_id=$1',[f.intent.intentId]))[0].storage).toBe("text");
  await expect(app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId)).rejects.toThrow();expect(await snapshot(f)).toEqual(corrupt);await rows('UPDATE "__jadpo_delivery_acceptance_proofs_v1" SET generation=$1 WHERE intent_id=$2',[before.proof[0].generation,f.intent.intentId]);
 }
 expect(calls).toHaveLength(1);expect(await snapshot(f)).toEqual(before);
}),15000);
test('coherent proof and claim changes cannot widen the original three-invocation one-hour contract',async()=>acceptedFixture(async(f,calls)=>{
 const grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),original=await snapshot(f);
 for(const field of ['max_invocations','lifetime_ms']){
  const replacement=field==='max_invocations'?4:3600001;
  const deadline=field==='lifetime_ms'?new Date(Date.parse(original.proof[0].lifetime_deadline)+1).toISOString():original.proof[0].lifetime_deadline;
  try{
   await rows(`UPDATE "__jadpo_delivery_claims_v1" SET "${field}"=$1,lifetime_deadline=$2 WHERE intent_id=$3`,[replacement,deadline,f.intent.intentId]);
   await rows(`UPDATE "__jadpo_delivery_acceptance_proofs_v1" SET "${field}"=$1,lifetime_deadline=$2 WHERE intent_id=$3`,[replacement,deadline,f.intent.intentId]);
   const forged=await snapshot(f);expect(Number(forged.proof[0][field])).toBe(replacement);expect(Number(forged.claim[0][field])).toBe(replacement);
   await expect(app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId)).rejects.toThrow();expect(await snapshot(f)).toEqual(forged);
  }finally{
   await rows(`UPDATE "__jadpo_delivery_claims_v1" SET "${field}"=$1,lifetime_deadline=$2 WHERE intent_id=$3`,[original.claim[0][field],original.claim[0].lifetime_deadline,f.intent.intentId]);
   await rows(`UPDATE "__jadpo_delivery_acceptance_proofs_v1" SET "${field}"=$1,lifetime_deadline=$2 WHERE intent_id=$3`,[original.proof[0][field],original.proof[0].lifetime_deadline,f.intent.intentId]);
  }
 }
 expect(await snapshot(f)).toEqual(original);expect(calls).toHaveLength(1);
}),15000);

for(const mode of ['superseded-B','ABA','disabled-owner','deleted-source'])test(`original accepted proof reconciles ${mode} without an extra business admission cut`,async()=>acceptedFixture(async(f,calls)=>{
 if(mode==='superseded-B'||mode==='ABA'){
  await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:'2026-10-07T08:00:00.000Z'},null);
  if(mode==='ABA')await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:f.value.due_at},null);
 }
 if(mode==='disabled-owner')await rows('UPDATE "user" SET status=$1 WHERE id=$2',['disabled',f.owner]);
 if(mode==='deleted-source')await rows('DELETE FROM todo WHERE id=$1',[f.value.id]);
 try{
  const before=await snapshot(f);await Bun.sleep(1600);
  const reconciler=app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile);
  expect(await reconciler.reconcile(f.intent.intentId)).toBe(true);
  const after=await snapshot(f);expect(after.proof).toEqual(before.proof);expect(after.claim).toEqual(before.claim);expect(after.intent).toEqual({...before.intent,state:'succeeded'});
  if(mode==='disabled-owner'){expect(new Date(after.source[0].reminder_sent_at).toISOString()).toBe(before.proof[0].observed_at);expect((await rows('SELECT status FROM "user" WHERE id=$1',[f.owner]))[0].status).toBe('disabled');}
  else expect(after.source).toEqual(before.source);
  expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toHaveLength(1);expect(calls).toHaveLength(1);
 }finally{if(mode==='disabled-owner')await rows('UPDATE "user" SET status=$1 WHERE id=$2',['active',f.owner]);}
}),15000);

test('grant withdrawal at an actual owned native boundary rolls back proof resolution',async()=>acceptedFixture(async(f,calls)=>{
 const host=app.authenticationHost(),grant=host.issueDeliveryReceiptReconciliationGrant(),before=await snapshot(f);
 let provisional:any=null,observed=0;
 if(pg){
  let unlock!:()=>void,entered!:()=>void;const hold=new Promise<void>(resolve=>unlock=resolve),ready=new Promise<void>(resolve=>entered=resolve);let blockerPID=0;
  const blocker=sql!.begin(async(tx:any)=>{blockerPID=Number((await tx.unsafe('SELECT pg_backend_pid() pid'))[0].pid);await tx.unsafe('SELECT ordering_key FROM "__jadpo_delivery_keys_v1" WHERE job=$1 AND ordering_key=$2 FOR UPDATE',[f.intent.job,f.intent.orderingKey]);entered();await hold;});
  await ready;const attempt=app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId);attempt.catch(()=>{});
  try{
   for(let i=0;i<100;i++){const waiters=await rows("SELECT pid, query, pg_blocking_pids(pid) blockers FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND $1=ANY(pg_blocking_pids(pid))",[blockerPID]);if(waiters.some((row:any)=>row.query.includes('UPDATE "__jadpo_delivery_keys_v1"'))){provisional={waiters,blockerPID};observed=1;break;}await Bun.sleep(5);}
   expect(observed).toBe(1);if(control!=='omit-withdrawal')host.revokeDeliveryReceiptReconciliationGrant(grant);
  }finally{unlock();await blocker;}
  await expect(attempt).rejects.toThrow();
 }else{
  const prepare=Database.prototype.prepare;
  Database.prototype.prepare=function(query:any){const statement=Reflect.apply(prepare,this,[query]);if(typeof query!=='string'||!query.startsWith('UPDATE "__jadpo_deliveries_v1" SET "state" = \'succeeded\''))return statement;const original=statement.all,writer=this;statement.all=function(...values:any[]){const result=Reflect.apply(original,this,values);const inspection=Reflect.apply(prepare,writer,['SELECT state FROM "__jadpo_deliveries_v1" WHERE intent_id=$1']);try{provisional={rows:inspection.all(f.intent.intentId),nativeTransaction:writer.inTransaction};}finally{inspection.finalize();}observed++;if(control!=='omit-withdrawal')host.revokeDeliveryReceiptReconciliationGrant(grant);return result;};return statement;};
  try{await expect(app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(f.intent.intentId)).rejects.toThrow();}finally{Database.prototype.prepare=prepare;}
  expect(provisional.rows).toEqual([{state:'succeeded'}]);expect(provisional.nativeTransaction).toBe(true);
 }
 expect(observed).toBe(1);expect(await snapshot(f)).toEqual(before);expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toEqual([]);expect(calls).toHaveLength(1);
 console.log('RECONCILE02_WITHDRAWAL '+JSON.stringify({database:pg?'postgres':'sqlite',provisional,observed,durableRolledBack:true,qualification:pg?'genuine PostgreSQL ordering-key lock wait; revoke purpose after actual waiter and before native result settles':'tests-only observer withdraws genuine purpose after same-writer terminal UPDATE returns; no native preemption claim'}));
}),15000);

test('genuinely exhausted third invocation with durable accepted proof terminates without allocating another claim',async()=>{
 const f=await seed(),calls:any[]=[];let accepting=false;
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){const body=await request.json();calls.push({body,accepted:accepting});return accepting?Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}}):Response.json({code:'rate_limited'},{status:429});}});
 const random=Math.random;
 try{
  adapter.setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);Math.random=()=>0;
  for(let invocation=1;invocation<=3;invocation++){
   accepting=invocation===3;const token=await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile);expect(token).not.toBeNull();const outcome=await app.dispatchDeliveryInvocation(token);
   if(!accepting)expect(await app.completeDeliveryInvocation(outcome)).toBe('retry_wait');
  }
  const before=await snapshot(f);expect(Number(before.claim[0].invocations)).toBe(3);expect(Number(before.claim[0].max_invocations)).toBe(3);expect(Number(before.proof[0].invocation)).toBe(3);expect(calls.filter(call=>call.accepted)).toHaveLength(1);expect(calls).toHaveLength(7);
  await Bun.sleep(1600);expect(await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile)).toBeNull();
  const afterRefusal=await snapshot(f);expect(afterRefusal.intent.state).toBe('outcome_unknown');
  expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(true);
  const after=await snapshot(f);expect(after.claim).toEqual(before.claim);expect(after.proof).toEqual(before.proof);expect(after.intent.state).toBe('succeeded');expect(calls).toHaveLength(7);
  console.log('RECONCILE02_EXHAUSTED '+JSON.stringify({qualification:'actual three native claims and six typed no-effect HTTP429 responses then one accepted HTTP202; seeded zero jitter only, actual clocks and unchanged limits',invocations:after.claim[0].invocations,requests:calls.length,accepted:calls.filter(call=>call.accepted).length}));
 }finally{Math.random=random;server.stop(true);}
},15000);

function worker(f:any,body:string,databaseURL=pg??''){
 const code=`const app=await import(${JSON.stringify(href('app.ts'))});const {persistence}=await import(${JSON.stringify(href('persistence.ts'))});const adapter=await import(${JSON.stringify(href('service-adapter.ts'))});await app.initializeApplication(${JSON.stringify(configuration)});const profile=${JSON.stringify(profile)},binding=${JSON.stringify(binding)},at=${JSON.stringify(at)};${body}`;
 const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null','-e',code],{env:{...Bun.env,DATABASE_URL:databaseURL,SQLITE_PATH:Bun.env.SQLITE_PATH,RECONCILE_WORKER_KEY:f.issued?.credential??'',NODE_ENV:'test'},stdin:'pipe',stdout:'pipe',stderr:'pipe'});
 children.push(child);return{child,done:Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited])};
}
async function finish(w:ReturnType<typeof worker>){const[out,err,exit]=await w.done;expect(exit,err).toBe(0);expect(await gone(w.child.pid)).toBe(true);return JSON.parse(out.trim().split('\n').at(-1)!);}
async function journalAckProxy(holdAck=false,boundary:'commit'|'reset'='commit'){
 if(!pg)return null;
 const direct=new URL(pg),sockets:Socket[]=[],connections:any[]=[];
 let armed=false,dropped=false,commits=0;let releaseAck:(()=>void)|null=null;
 const server=createServer(down=>{
  const up=createConnection({host:direct.hostname,port:Number(direct.port||5432)});
  sockets.push(down,up);const state={connection:connections.length+1,pid:null as number|null,awaitingCommit:false,commandComplete:false,readyIdle:false,frontendCommits:0,timeoutResets:0,closed:false,proofInsertSeen:false,insertAcknowledged:false,insertReadyTransaction:false};connections.push(state);
  let front=Buffer.alloc(0),back=Buffer.alloc(0),held=Buffer.alloc(0);
  down.on('data',chunk=>{
   if(armed){front=Buffer.concat([front,chunk]);let offset=0;
    while(offset+5<=front.length){const length=front.readInt32BE(offset+1);if(length<4||length>1048576)throw Error('bounded frontend frame invalid');if(offset+1+length>front.length)break;
     const tag=front[offset],payload=front.subarray(offset+5,offset+1+length);let text='';
     if(tag===81)text=payload.subarray(0,payload.indexOf(0)).toString();
     if(tag===80){const first=payload.indexOf(0);text=payload.subarray(first+1,payload.indexOf(0,first+1)).toString();}
     if(text.includes('INSERT INTO \"__jadpo_delivery_acceptance_proofs_v1\"'))state.proofInsertSeen=true;
     const selected=text.trim().replace(/;$/u,'').toUpperCase();if(selected==='COMMIT'){commits++;state.frontendCommits++;}if(selected==='SET LOCAL STATEMENT_TIMEOUT = 0')state.timeoutResets++;if((boundary==='commit'&&selected==='COMMIT')||(boundary==='reset'&&selected==='SET LOCAL STATEMENT_TIMEOUT = 0')){state.awaitingCommit=true;held=Buffer.alloc(0);}
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
    if(state.proofInsertSeen&&tag===67&&payload.toString()==='INSERT 0 1\0')state.insertAcknowledged=true;
    if(state.insertAcknowledged&&!state.awaitingCommit&&tag===90&&payload.toString()==='T')state.insertReadyTransaction=true;
    if(state.awaitingCommit&&tag===67&&payload.toString()===(boundary==='commit'?'COMMIT\0':'SET\0'))state.commandComplete=true;
    if(state.awaitingCommit&&state.commandComplete&&tag===90&&payload.toString()===(boundary==='commit'?'I':'T'))state.readyIdle=true;
    offset+=1+length;
   }back=back.subarray(offset);
   if(state.awaitingCommit&&!dropped){
    if(state.commandComplete&&state.readyIdle){if(control==='omit-journal-ack-loss'&&!holdAck){state.awaitingCommit=false;down.write(held);held=Buffer.alloc(0);return;}dropped=true;expect(state.pid).toBeNumber();if(holdAck){releaseAck=()=>{down.write(held);held=Buffer.alloc(0);state.awaitingCommit=false;};}else{down.destroy();up.destroy();held=Buffer.alloc(0);}}return;
   }down.write(chunk);
  });
  down.on('error',()=>{});up.on('error',()=>{});down.on('close',()=>{up.destroy();state.closed=true;});up.on('close',()=>down.destroy());
 });
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));const port=(server.address()as AddressInfo).port;const url=new URL(pg);url.port=String(port);
 return{url:url.toString(),port,arm(){expect(commits).toBe(0);expect(dropped).toBe(false);armed=true;},held(){return releaseAck!==null;},release(){if(!releaseAck)throw Error('actual COMMIT acknowledgment not held');const release=releaseAck;releaseAck=null;release();},proof(){return{armed,dropped,commits,connections,holdAck,boundary}},async close(){for(const socket of sockets)socket.destroy();await new Promise<void>(resolve=>server.close(()=>resolve()));expect(await portClosed(port)).toBe(true);expect(connections.every(c=>c.closed)).toBe(true);}};
}

test('journal COMMIT ACK uncertainty keeps genuine committed proof and permits only terminal resolution in a distinct worker',async()=>{
 const f=await seed(),calls:any[]=[],proxy=await journalAckProxy();
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){const body=await request.json();calls.push(body);proxy?.arm();return Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});}});
 let first:ReturnType<typeof worker>|null=null;
 try{
  const endpoint=`http://127.0.0.1:${server.port}`;
  const sqliteFault=control==='omit-journal-ack-loss'?'let nativeCommit=0;':`const {Database}=await import('bun:sqlite');const prepare=Database.prototype.prepare,exec=Database.prototype.exec;let armed=false,nativeCommit=0;Database.prototype.prepare=function(query){const stmt=Reflect.apply(prepare,this,[query]);if(typeof query==='string'&&query.startsWith('INSERT INTO "__jadpo_delivery_acceptance_proofs_v1"'))armed=true;return stmt;};Database.prototype.exec=function(query){const result=Reflect.apply(exec,this,[query]);if(armed&&query==='COMMIT'){nativeCommit++;throw Object.assign(new Error('test-owned journal ACK lost after real COMMIT'),{code:'SQLITE_IOERR'});}return result;};`;
  first=worker(f,`adapter.setReferenceMailEndpointForTesting(${JSON.stringify(endpoint)});const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',process.env.RECONCILE_WORKER_KEY,Date.now());const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);${pg?'let nativeCommit=null;':sqliteFault}let outcome=null,fault=null;try{outcome=await app.dispatchDeliveryInvocation(token);}catch(error){fault={name:error.name,kind:error.kind,operation:error.operation,transaction:error.transaction};}console.log(JSON.stringify({pid:process.pid,outcomeIssued:outcome!==null,fault,nativeCommit}));process.exit(0);`,proxy?.url??pg??'');
  const uncertain=await finish(first);expect(uncertain.outcomeIssued).toBe(false);expect(uncertain.fault.kind).toBe('unknown');expect(uncertain.fault.transaction).toMatchObject({outcome:'unknown',commitAcknowledged:false,retryableCause:null});if(!pg)expect(uncertain.nativeCommit).toBe(1);
  const before=await snapshot(f);expect(before.proof).toHaveLength(1);expect(before.intent.state).toBe('running');expect(calls).toEqual([f.intent.payload]);
  if(pg){const wire=proxy!.proof();expect(wire.commits).toBe(1);expect(wire.dropped).toBe(true);expect(wire.connections.filter(c=>c.proofInsertSeen&&c.insertAcknowledged&&c.insertReadyTransaction&&c.commandComplete&&c.readyIdle&&c.frontendCommits===1)).toHaveLength(1);}
  const next=worker(f,`const grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant();const reconciler=app.createDeliveryReceiptReconciler(binding,grant,profile);const completed=await reconciler.reconcile(${JSON.stringify(f.intent.intentId)});console.log(JSON.stringify({pid:process.pid,completed}));process.exit(0);`);
  const terminal=await finish(next);expect(terminal.pid).not.toBe(uncertain.pid);expect(terminal.completed).toBe(true);const after=await snapshot(f);expect(after.proof).toEqual(before.proof);expect(after.claim).toEqual(before.claim);expect(after.intent.state).toBe('succeeded');expect(calls).toHaveLength(1);
  console.log('RECONCILE02_JOURNAL_ACK '+JSON.stringify({database:pg?'postgres':'sqlite',uncertain,terminal,wire:proxy?.proof()??null,providerCalls:calls.length,qualification:pg?'actual proof INSERT acknowledgment and ReadyForQuery T then same-PID COMMIT C/ZI withheld and TCP destroyed':'actual SQLite journal COMMIT then explicitly injected acknowledgment error; no wire-loss claim'}));
 }finally{if(first&&!(await gone(first.child.pid))){first.child.kill('SIGKILL');await first.done;}const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);await proxy?.close();}
},20000);
async function ensureProofFixture(){
 const exists=pg?(await rows("SELECT to_regclass('\"__jadpo_delivery_acceptance_proofs_v1\"') relation"))[0].relation!==null:(await rows("SELECT name FROM sqlite_master WHERE name='__jadpo_delivery_acceptance_proofs_v1'")).length>0;
 if(!exists)await acceptedFixture(async(f)=>{expect(await app.completeDeliveryInvocation(f.outcome)).toBe(true);});
}
async function failureTrigger(table:string,event:string,name:string){
 if(pg){await rows(`CREATE FUNCTION "${name}"() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled reconciliation native fixture' USING ERRCODE='P0001'; END $$`);await rows(`CREATE TRIGGER "${name}" BEFORE ${event} ON "${table}" FOR EACH ROW EXECUTE FUNCTION "${name}"()`);}
 else await rows(`CREATE TRIGGER "${name}" BEFORE ${event} ON "${table}" BEGIN SELECT RAISE(ABORT,'controlled reconciliation native fixture'); END`);
 return async()=>{await rows(pg?`DROP TRIGGER "${name}" ON "${table}"`:`DROP TRIGGER "${name}"`);if(pg)await rows(`DROP FUNCTION "${name}"()`);};
}
test('actual proof storage failure stays outside provider retry classification and cannot synthesize no-effect evidence',async()=>{
 await ensureProofFixture();const f=await seed(),calls:any[]=[];
 const remove=control==='omit-storage-failure'?async()=>{}:await failureTrigger('__jadpo_delivery_acceptance_proofs_v1','INSERT','reconcile_journal_failure');
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){const body=await request.json();calls.push(body);return Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});}});
 try{
  adapter.setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);const token=await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile);expect(token).not.toBeNull();
  let issued=null,error:any=null;try{issued=await app.dispatchDeliveryInvocation(token);}catch(fault){error=fault;}
  expect(issued).toBeNull();expect(error.name).toBe('PersistenceFault');expect(error.operation).toBe('delivery.acceptance.insert');expect(error.transaction).toMatchObject({outcome:'no_commit',rollbackProven:true,retryableCause:null});expect(calls).toEqual([f.intent.payload]);
  const before=await snapshot(f);expect(before.proof).toEqual([]);expect(before.intent.state).toBe('running');expect(Number(before.claim[0].possible_dispatch)).toBe(1);expect(Number(before.claim[0].invocations)).toBe(1);
  const attemptsPresent=pg?(await rows("SELECT to_regclass('\"__jadpo_delivery_attempts_v1\"') relation"))[0].relation!==null:(await rows("SELECT name FROM sqlite_master WHERE name='__jadpo_delivery_attempts_v1'")).length>0;
  if(attemptsPresent)expect(await rows('SELECT * FROM "__jadpo_delivery_attempts_v1" WHERE intent_id=$1',[f.intent.intentId])).toEqual([]);
  expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(false);
  await expect(app.dispatchDeliveryInvocation(token)).rejects.toThrow();await Bun.sleep(1600);expect(await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile)).toBeNull();
  const after=await snapshot(f);expect(after.intent.state).toBe('outcome_unknown');expect(after.proof).toEqual([]);expect(after.claim).toEqual(before.claim);expect(after.source).toEqual(before.source);expect(calls).toHaveLength(1);
  console.log('RECONCILE02_JOURNAL_FAILURE '+JSON.stringify({database:pg?'postgres':'sqlite',nativeFault:{name:error.name,kind:error.kind,operation:error.operation,transaction:error.transaction},providerCalls:calls.length,proofCount:after.proof.length,qualification:'test-owned BEFORE INSERT trigger causes real native storage error after genuine accepted adapter result; proof TX rolls back but external dispatch cannot be refunded'}));
 }finally{await remove();const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);}
},15000);
test('real source-write failure rolls back terminal receipt and state while retaining immutable proof',async()=>acceptedFixture(async(f,calls)=>{
 const before=await snapshot(f),remove=await failureTrigger('todo','UPDATE','reconcile_sent_failure');
 const reconciler=app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile);
 try{await expect(reconciler.reconcile(f.intent.intentId)).rejects.toThrow();expect(await snapshot(f)).toEqual(before);expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toEqual([]);}finally{await remove();}
 expect(await reconciler.reconcile(f.intent.intentId)).toBe(true);expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toHaveLength(1);expect(calls).toHaveLength(1);
}),15000);

async function waitFor(check:()=>Promise<boolean>,limit=3000){const deadline=performance.now()+limit;while(performance.now()<deadline){if(await check())return true;await Bun.sleep(5);}return false;}
for(const phase of ['before-journal','after-journal'])test(`actual process death ${phase} retains only genuinely committed acceptance evidence`,async()=>{
 await ensureProofFixture();const f=await seed(),calls:any[]=[],marker=join(root,crypto.randomUUID()+'.checkpoint');
 let unlock:(()=>void)|null=null,blocker:Promise<any>|null=null,blockerPID=0,trigger=false,first:ReturnType<typeof worker>|null=null,selected:any=null;
 const lock=7419283;
 if(pg&&phase==='before-journal'){
  let ready!:()=>void;const entered=new Promise<void>(resolve=>ready=resolve),hold=new Promise<void>(resolve=>unlock=resolve);
  blocker=sql!.begin(async(tx:any)=>{blockerPID=Number((await tx.unsafe('SELECT pg_backend_pid() pid'))[0].pid);await tx.unsafe('SELECT pg_advisory_xact_lock($1)',[lock]);ready();await hold;});await entered;
  await rows(`CREATE FUNCTION reconcile_death_wait() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(${lock}); RETURN NEW; END $$`);
  await rows('CREATE TRIGGER reconcile_death_wait BEFORE INSERT ON "__jadpo_delivery_acceptance_proofs_v1" FOR EACH ROW EXECUTE FUNCTION reconcile_death_wait()');trigger=true;
 }
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){const body=await request.json();calls.push(body);return Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});}});
 try{
  const hook=!pg&&phase==='before-journal'?`const {Database}=await import('bun:sqlite');const original=Database.prototype.prepare;Database.prototype.prepare=function(query){if(typeof query==='string'&&query.startsWith('INSERT INTO "__jadpo_delivery_acceptance_proofs_v1"')){require('node:fs').writeFileSync(${JSON.stringify(marker)},JSON.stringify({phase:'before-journal',pid:process.pid,nativeTransaction:this.inTransaction}));process.kill(process.pid,'SIGKILL');}return Reflect.apply(original,this,[query]);};`:'';
  first=worker(f,`adapter.setReferenceMailEndpointForTesting(${JSON.stringify(`http://127.0.0.1:${server.port}`)});const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',process.env.RECONCILE_WORKER_KEY,Date.now());const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);${hook}const outcome=await app.dispatchDeliveryInvocation(token);require('node:fs').writeFileSync(${JSON.stringify(marker)},JSON.stringify({phase:'after-journal',pid:process.pid,outcomeIssued:outcome!==null}));await Bun.sleep(30000);`);
  if(pg&&phase==='before-journal'){
   expect(await waitFor(async()=>{const found=await rows("SELECT pid,query,pg_blocking_pids(pid) blockers FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND $1=ANY(pg_blocking_pids(pid))",[blockerPID]);selected=found.find((item:any)=>item.query.includes('INSERT INTO "__jadpo_delivery_acceptance_proofs_v1"'));return Boolean(selected);})).toBe(true);
   first.child.kill('SIGKILL');
  }else{
   expect(await waitFor(async()=>{try{selected=JSON.parse(readFileSync(marker,'utf8'));return true;}catch{return false;}})).toBe(true);
   if(phase==='after-journal'){expect(selected.outcomeIssued).toBe(true);first.child.kill('SIGKILL');}else expect(selected.nativeTransaction).toBe(true);
  }
  const[out,err,exit]=await first.done;expect(exit,err).toBe(137);expect(await gone(first.child.pid)).toBe(true);
  if(unlock){unlock();await blocker;unlock=null;expect(await waitFor(async()=>(await rows('SELECT pid FROM pg_stat_activity WHERE pid=$1',[selected.pid])).length===0)).toBe(true);}
  const before=await snapshot(f);expect(before.proof).toHaveLength(phase==='after-journal'?1:0);expect(before.intent.state).toBe('running');expect(Number(before.claim[0].possible_dispatch)).toBe(1);expect(calls).toEqual([f.intent.payload]);
  await Bun.sleep(1600);
  const next=worker(f,`const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',process.env.RECONCILE_WORKER_KEY,Date.now());const invocation=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);const grant=app.authenticationHost().issueDeliveryReceiptReconciliationGrant();const completed=await app.createDeliveryReceiptReconciler(binding,grant,profile).reconcile(${JSON.stringify(f.intent.intentId)});console.log(JSON.stringify({pid:process.pid,invocationIssued:invocation!==null,completed}));process.exit(0);`);
  const terminal=await finish(next);expect(terminal.pid).not.toBe(first.child.pid);expect(terminal.invocationIssued).toBe(false);expect(terminal.completed).toBe(phase==='after-journal');
  const after=await snapshot(f);expect(after.proof).toEqual(before.proof);expect(after.claim).toEqual(before.claim);expect(after.intent.state).toBe(phase==='after-journal'?'succeeded':'outcome_unknown');expect(calls).toHaveLength(1);
  if(phase==='before-journal'){
   expect(after.source).toEqual(before.source);expect(await receiptRows(f.intent.intentId)).toEqual([]);
   const revisions:any[]=[];for(const due of ['2026-10-07T08:00:00.000Z',f.value.due_at]){
    await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:due},null);
    const page=await persistence.withOperationTime(at).transaction((tx:any)=>tx.select_delivery_intents(binding,f.proof,null));expect(page.intents).toHaveLength(1);const nextIntent=page.intents[0];expect(nextIntent.sourceEntityId).toBe(f.value.id);expect(nextIntent.intentId).not.toBe(f.intent.intentId);expect(nextIntent.sourceRevision).not.toBe(f.intent.sourceRevision);expect(nextIntent.payload.due_at).toBe(due);
    expect(await app.prepareDeliveryInvocation(binding,f.proof,nextIntent.intentId,at,profile)).toBeNull();revisions.push({intentId:nextIntent.intentId,sourceRevision:nextIntent.sourceRevision,state:(await persistence.read_delivery_intent(nextIntent.intentId)).state});
   }
   expect(revisions[0].intentId).not.toBe(revisions[1].intentId);expect(revisions[0].sourceRevision).not.toBe(revisions[1].sourceRevision);const unknown=await snapshot(f);expect(unknown.proof).toEqual([]);expect(unknown.claim).toEqual(before.claim);expect(unknown.intent.state).toBe('outcome_unknown');expect(unknown.source[0].reminder_sent_at).toBeNull();expect(calls).toHaveLength(1);console.log('RECONCILE02_UNKNOWN_KEY_BLOCK '+JSON.stringify({revisions,originalIntent:f.intent.intentId,providerCalls:1}));
  }
  console.log('RECONCILE02_PROCESS_DEATH '+JSON.stringify({database:pg?'postgres':'sqlite',phase,selected,exit,terminal,proofCount:before.proof.length,providerCalls:calls.length,qualification:phase==='after-journal'?'actual adapter acceptance and acknowledged journal then real SIGKILL; distinct worker after actual lease expiry':'actual adapter acceptance; SQLite observer kills before native proof INSERT, PostgreSQL actual BEFORE INSERT advisory lock waits behind exact parent PID then SIGKILL; no durable proof inferred'}));
 }finally{
  if(first&&!(await gone(first.child.pid))){first.child.kill('SIGKILL');await first.done;}if(unlock){unlock();await blocker;}
  if(trigger){await rows('DROP TRIGGER reconcile_death_wait ON "__jadpo_delivery_acceptance_proofs_v1"');await rows('DROP FUNCTION reconcile_death_wait()');}
  const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);rmSync(marker,{force:true});
 }
},20000);

for(const mode of ['superseded-B','ABA','disabled-owner','deleted-source'])test(`provider admission before journal preserves original proof despite ${mode}`,async()=>{
 const f=await seed(),calls:any[]=[];let changed:any=null;
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){
  const body=await request.json();calls.push(body);
  if((mode==='superseded-B'||mode==='ABA')&&control!=='omit-source-supersession'){await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:'2026-10-07T08:00:00.000Z'},null);if(mode==='ABA')await f.client.update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at(f.value.id,{due_at:f.value.due_at},null);}
  if(mode==='disabled-owner')await rows('UPDATE "user" SET status=$1 WHERE id=$2',['disabled',f.owner]);
  if(mode==='deleted-source')await rows('DELETE FROM todo WHERE id=$1',[f.value.id]);
  changed=await rows('SELECT * FROM todo WHERE id=$1',[f.value.id]);return Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});
 }});
 try{
  adapter.setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);const token=await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile);expect(token).not.toBeNull();const outcome=await app.dispatchDeliveryInvocation(token);expect(outcome).not.toBeNull();
  const before=await snapshot(f);expect(before.proof).toHaveLength(1);expect(before.proof[0].source_revision).toBe(f.intent.sourceRevision);expect(JSON.parse(before.proof[0].payload)).toEqual(f.intent.payload);expect(before.source).toEqual(changed);
  expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(true);
  const after=await snapshot(f);expect(after.proof).toEqual(before.proof);expect(after.claim).toEqual(before.claim);expect(after.intent.state).toBe('succeeded');
  if(mode==='disabled-owner')expect(new Date(after.source[0].reminder_sent_at).toISOString()).toBe(before.proof[0].observed_at);else expect(after.source).toEqual(changed);
  expect(calls).toEqual([f.intent.payload]);expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toHaveLength(1);
 }finally{if(mode==='disabled-owner')await rows('UPDATE "user" SET status=$1 WHERE id=$2',['active',f.owner]);const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);}
},15000);

for(const cut of ['withdrawal','reinitialization'])test(`purpose ${cut} after actual reconciliation COMMIT refuses publication without claiming rollback`,async()=>acceptedFixture(async(f,calls)=>{
 let publication:any=null,wire:any=null;
 if(pg){
  const proxy=await journalAckProxy(true),readyMarker=join(root,crypto.randomUUID()+'.ready'),cutMarker=join(root,crypto.randomUUID()+'.cut');let current:ReturnType<typeof worker>|null=null;
  try{
   current=worker(f,`const host=app.authenticationHost(),grant=host.issueDeliveryReceiptReconciliationGrant(),reconciler=app.createDeliveryReceiptReconciler(binding,grant,profile);require('node:fs').writeFileSync(${JSON.stringify(readyMarker)},'ready');let pending;for await(const chunk of process.stdin){const command=chunk.toString().trim();if(command==='GO'){pending=reconciler.reconcile(${JSON.stringify(f.intent.intentId)}).then(value=>({value,fault:null}),error=>({value:null,fault:{name:error.name,kind:error.kind}}));}if(command==='CUT'){${control==='omit-postcommit-cut'?'':cut==='withdrawal'?'host.revokeDeliveryReceiptReconciliationGrant(grant);':'await app.initializeApplication('+JSON.stringify(configuration)+');'}require('node:fs').writeFileSync(${JSON.stringify(cutMarker)},'cut');break;}}const result=await pending;console.log(JSON.stringify({pid:process.pid,...result}));process.exit(0);`,proxy!.url);
   expect(await waitFor(async()=>{try{return readFileSync(readyMarker,'utf8')==='ready';}catch{return false;}})).toBe(true);proxy!.arm();current.child.stdin.write('GO\n');current.child.stdin.flush();
   expect(await waitFor(async()=>proxy!.held())).toBe(true);const committed=await snapshot(f);expect(committed.intent.state).toBe('succeeded');expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toHaveLength(1);
   current.child.stdin.write('CUT\n');current.child.stdin.flush();expect(await waitFor(async()=>{try{return readFileSync(cutMarker,'utf8')==='cut';}catch{return false;}})).toBe(true);proxy!.release();publication=await finish(current);expect(publication.value).toBeNull();expect(publication.fault.name).toBe('AuthenticationFault');wire=proxy!.proof();expect(wire.commits).toBe(1);expect(wire.connections.filter((c:any)=>c.commandComplete&&c.readyIdle&&c.frontendCommits===1)).toHaveLength(1);expect(await snapshot(f)).toEqual(committed);
  }finally{if(current&&!(await gone(current.child.pid))){current.child.kill('SIGKILL');await current.done;}await proxy!.close();rmSync(readyMarker,{force:true});rmSync(cutMarker,{force:true});}
 }else{
  const host=app.authenticationHost(),grant=host.issueDeliveryReceiptReconciliationGrant(),reconciler=app.createDeliveryReceiptReconciler(binding,grant,profile),prepare=Database.prototype.prepare,exec=Database.prototype.exec;let armed=false,commits=0,reinitialize:Promise<any>|null=null;
  Database.prototype.prepare=function(query:any){if(typeof query==='string'&&query.startsWith('UPDATE "__jadpo_deliveries_v1" SET "state" = \'succeeded\''))armed=true;return Reflect.apply(prepare,this,[query]);};
  Database.prototype.exec=function(query:any){const result=Reflect.apply(exec,this,[query]);if(armed&&query==='COMMIT'){commits++;if(control!=='omit-postcommit-cut'){if(cut==='withdrawal')host.revokeDeliveryReceiptReconciliationGrant(grant);else reinitialize=app.initializeApplication(configuration);}}return result;};
  try{let value=null,error:any=null;try{value=await reconciler.reconcile(f.intent.intentId);}catch(fault){error=fault;}publication={value,fault:{name:error?.name,kind:error?.kind}};expect(value).toBeNull();expect(error.name).toBe('AuthenticationFault');expect(commits).toBe(1);}finally{Database.prototype.prepare=prepare;Database.prototype.exec=exec;await reinitialize;}
 }
 const committed=await snapshot(f);expect(committed.intent.state).toBe('succeeded');expect(await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[f.intent.intentId])).toHaveLength(1);
 expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(true);expect(await snapshot(f)).toEqual(committed);expect(calls).toHaveLength(1);
 console.log('RECONCILE02_POSTCOMMIT_CUT '+JSON.stringify({database:pg?'postgres':'sqlite',cut,publication,wire,durableState:'succeeded',receiptCount:1,providerCalls:calls.length,qualification:pg?'same-PID native COMMIT C/ZI held; genuine purpose cut before driver acknowledgment then release; committed state cannot be rolled back':'actual SQLite COMMIT completes before synchronous observer purpose cut; post-await publication refuses, no wire uncertainty claim'}));
}),20000);

test('accepted proof storage work consumes the original lease and cannot refund external dispatch',async()=>{
 await ensureProofFixture();const f=await seed(),calls:any[]=[];let provisional:any=null;
 const prepare=Database.prototype.prepare;
 if(pg){await rows("CREATE FUNCTION reconcile_proof_delay() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.8); RETURN NEW; END $$");await rows('CREATE TRIGGER reconcile_proof_delay BEFORE INSERT ON "__jadpo_delivery_acceptance_proofs_v1" FOR EACH ROW EXECUTE FUNCTION reconcile_proof_delay()');}
 else Database.prototype.prepare=function(query:any){const stmt=Reflect.apply(prepare,this,[query]);if(typeof query!=='string'||!query.startsWith('INSERT INTO "__jadpo_delivery_acceptance_proofs_v1"'))return stmt;const original=stmt.all,writer=this;stmt.all=function(...values:any[]){const result=Reflect.apply(original,this,values);provisional={nativeTransaction:writer.inTransaction,inserted:result};if(control!=='omit-proof-delay')Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,1800);return result;};return stmt;};
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){const body=await request.json();calls.push(body);return Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});}});
 try{
  if(pg&&control==='omit-proof-delay'){await rows('DROP TRIGGER reconcile_proof_delay ON "__jadpo_delivery_acceptance_proofs_v1"');}
  adapter.setReferenceMailEndpointForTesting(`http://127.0.0.1:${server.port}`);const token=await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile);expect(token).not.toBeNull();const started=performance.now();let outcome=null,error:any=null;try{outcome=await app.dispatchDeliveryInvocation(token);}catch(fault){error=fault;}
  expect(outcome).toBeNull();expect(error).not.toBeNull();expect(performance.now()-started).toBeGreaterThanOrEqual(1400);if(!pg)expect(provisional.nativeTransaction).toBe(true);
  const before=await snapshot(f);expect(before.proof).toEqual([]);expect(before.intent.state).toBe('running');expect(Number(before.claim[0].possible_dispatch)).toBe(1);expect(calls).toEqual([f.intent.payload]);
  expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(false);expect(await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile)).toBeNull();
  const after=await snapshot(f);expect(after.intent.state).toBe('outcome_unknown');expect(after.proof).toEqual([]);expect(after.claim).toEqual(before.claim);expect(calls).toHaveLength(1);
  console.log('RECONCILE02_PRODUCER_EXPIRY '+JSON.stringify({database:pg?'postgres':'sqlite',elapsedMs:performance.now()-started,provisional,fault:{name:error.name,kind:error.kind,transaction:error.transaction},proofCount:0,providerCalls:1,qualification:pg?'actual AFTER accepted202 proof INSERT trigger waits pg_sleep beyond original1500ms lease; native statement budget cancels and rollback discards proof':'test-only same-writer observer lets real proof INSERT run in transaction then blocks synchronously1800ms; cooperative boundary refuses and rolls back, no hard preemption claim'}));
 }finally{Database.prototype.prepare=prepare;if(pg){if(control!=='omit-proof-delay')await rows('DROP TRIGGER reconcile_proof_delay ON "__jadpo_delivery_acceptance_proofs_v1"');await rows('DROP FUNCTION reconcile_proof_delay()');}const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);}
},15000);

for(const cut of ['withdrawal','reinitialization'])test.skipIf(!pg)(`purpose ${cut} during genuine timeout reset ACK refuses before reconciliation COMMIT`,async()=>{await acceptedFixture(async(f,calls)=>{const before=await snapshot(f);
 let publication:any=null,wire:any=null;
 if(pg){
  const proxy=await journalAckProxy(true,'reset'),readyMarker=join(root,crypto.randomUUID()+'.ready'),cutMarker=join(root,crypto.randomUUID()+'.cut');let current:ReturnType<typeof worker>|null=null;
  try{
   current=worker(f,`const host=app.authenticationHost(),grant=host.issueDeliveryReceiptReconciliationGrant(),reconciler=app.createDeliveryReceiptReconciler(binding,grant,profile);require('node:fs').writeFileSync(${JSON.stringify(readyMarker)},'ready');let pending;for await(const chunk of process.stdin){const command=chunk.toString().trim();if(command==='GO'){pending=reconciler.reconcile(${JSON.stringify(f.intent.intentId)}).then(value=>({value,fault:null}),error=>({value:null,fault:{name:error.name,kind:error.kind}}));}if(command==='CUT'){${control==='omit-postcommit-cut'?'':cut==='withdrawal'?'host.revokeDeliveryReceiptReconciliationGrant(grant);':'await app.initializeApplication('+JSON.stringify(configuration)+');'}require('node:fs').writeFileSync(${JSON.stringify(cutMarker)},'cut');break;}}const result=await pending;console.log(JSON.stringify({pid:process.pid,...result}));process.exit(0);`,proxy!.url);
   expect(await waitFor(async()=>{try{return readFileSync(readyMarker,'utf8')==='ready';}catch{return false;}})).toBe(true);proxy!.arm();current.child.stdin.write('GO\n');current.child.stdin.flush();
   expect(await waitFor(async()=>proxy!.held())).toBe(true);const committed=await snapshot(f);expect(committed).toEqual(before);expect(await receiptRows(f.intent.intentId)).toEqual([]);
   current.child.stdin.write('CUT\n');current.child.stdin.flush();expect(await waitFor(async()=>{try{return readFileSync(cutMarker,'utf8')==='cut';}catch{return false;}})).toBe(true);proxy!.release();publication=await finish(current);expect(publication.value).toBeNull();expect(publication.fault.name).toBe('AuthenticationFault');wire=proxy!.proof();console.log('RECONCILE02_RESET_OBSERVATION '+JSON.stringify({cut,publication,wire,settled:await snapshot(f),receipts:await receiptRows(f.intent.intentId)}));expect(wire.commits).toBe(0);expect(wire.connections.filter((c:any)=>c.commandComplete&&c.readyIdle&&c.timeoutResets===1&&c.frontendCommits===0)).toHaveLength(1);expect(await snapshot(f)).toEqual(committed);
  }finally{if(current&&!(await gone(current.child.pid))){current.child.kill('SIGKILL');await current.done;}await proxy!.close();rmSync(readyMarker,{force:true});rmSync(cutMarker,{force:true});}
 }
 const committed=await snapshot(f);expect(committed).toEqual(before);expect(await receiptRows(f.intent.intentId)).toEqual([]);
 expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(true);expect((await snapshot(f)).intent.state).toBe('succeeded');expect(calls).toHaveLength(1);
 console.log('RECONCILE02_RESET_CUT '+JSON.stringify({database:pg?'postgres':'sqlite',cut,publication,wire,durableState:'rolled_back_before_commit',receiptCount:0,providerCalls:calls.length,qualification:pg?'same-PID native SET LOCAL C/ReadyForQuery T held after work; genuine purpose cut before COMMIT then release; owned TX must roll back':'actual SQLite COMMIT completes before synchronous observer purpose cut; post-await publication refuses, no wire uncertainty claim'}));
});},20000);


async function receiptRows(id:string){const exists=pg?(await rows("SELECT to_regclass('\"__jadpo_delivery_mail_receipts_v1\"') relation"))[0].relation!==null:(await rows("SELECT name FROM sqlite_master WHERE name='__jadpo_delivery_mail_receipts_v1'")).length>0;return exists?await rows('SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE intent_id=$1',[id]):[];}

test.skipIf(!pg)('genuine producer owner reinitialization during timeout reset ACK rolls back proof before COMMIT',async()=>{
 await ensureProofFixture();const f=await seed(),calls:any[]=[],proxy=await journalAckProxy(true,'reset'),cutMarker=join(root,crypto.randomUUID()+'.producer-cut');let current:ReturnType<typeof worker>|null=null;
 const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){const body=await request.json();calls.push(body);proxy!.arm();return Response.json({accepted_at:at},{status:202,headers:{'Idempotency-Key':body.idempotency_key}});}});
 try{
  current=worker(f,`adapter.setReferenceMailEndpointForTesting(${JSON.stringify(`http://127.0.0.1:${server.port}`)});const proof=await app.authenticationHost().prepareDeliveryCredential('api_bearer',process.env.RECONCILE_WORKER_KEY,Date.now());const token=await app.prepareDeliveryInvocation(binding,proof,${JSON.stringify(f.intent.intentId)},at,profile);const pending=app.dispatchDeliveryInvocation(token).then(value=>({outcomeIssued:value!==null,fault:null}),error=>({outcomeIssued:false,fault:{name:error.name,kind:error.kind}}));for await(const chunk of process.stdin){if(chunk.toString().trim()==='CUT'){await app.initializeApplication(${JSON.stringify(configuration)});require('node:fs').writeFileSync(${JSON.stringify(cutMarker)},'cut');break;}}console.log(JSON.stringify({pid:process.pid,...await pending}));process.exit(0);`,proxy!.url);
  expect(await waitFor(async()=>proxy!.held())).toBe(true);expect(proxy!.proof().commits).toBe(0);expect((await snapshot(f)).proof).toEqual([]);current.child.stdin.write('CUT\n');current.child.stdin.flush();expect(await waitFor(async()=>{try{return readFileSync(cutMarker,'utf8')==='cut';}catch{return false;}})).toBe(true);proxy!.release();const publication=await finish(current),wire=proxy!.proof(),settled=await snapshot(f);
  console.log('RECONCILE02_PRODUCER_RESET '+JSON.stringify({publication,wire,settled,qualification:'actual checked adapter202 and proof INSERT C/T; same-PID timeout reset C/T held; actual application reinitialization invalidates original opaque owner before any COMMIT'}));
  expect(publication.outcomeIssued).toBe(false);expect(publication.fault.name).toBe('AuthenticationFault');expect(wire.commits).toBe(0);expect(wire.connections.filter((c:any)=>c.proofInsertSeen&&c.insertAcknowledged&&c.insertReadyTransaction&&c.commandComplete&&c.readyIdle&&c.timeoutResets===1&&c.frontendCommits===0)).toHaveLength(1);expect(settled.proof).toEqual([]);expect(settled.intent.state).toBe('running');expect(Number(settled.claim[0].possible_dispatch)).toBe(1);expect(calls).toEqual([f.intent.payload]);
  await Bun.sleep(1600);expect(await app.prepareDeliveryInvocation(binding,f.proof,f.intent.intentId,at,profile)).toBeNull();expect(await app.createDeliveryReceiptReconciler(binding,app.authenticationHost().issueDeliveryReceiptReconciliationGrant(),profile).reconcile(f.intent.intentId)).toBe(false);expect((await snapshot(f)).intent.state).toBe('outcome_unknown');expect(calls).toHaveLength(1);
 }finally{if(current&&!(await gone(current.child.pid))){current.child.kill('SIGKILL');await current.done;}await proxy!.close();const port=server.port;server.stop(true);expect(await portClosed(port)).toBe(true);rmSync(cutMarker,{force:true});}
},20000);
