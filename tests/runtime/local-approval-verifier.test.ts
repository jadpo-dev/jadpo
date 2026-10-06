import {afterAll,beforeAll,expect,test as bunTest} from 'bun:test';
const test=(name:string,body:()=>Promise<unknown>|unknown)=>bunTest(name,body,60_000);
import {createHash} from 'node:crypto';
import {Database} from 'bun:sqlite';
import {createLocalConsumptionLedger} from '../../tools/local_approval_ledger';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,readdirSync,cpSync,rmSync,symlinkSync,renameSync} from 'node:fs';
import {tmpdir} from 'node:os';import {resolve,join,relative} from 'node:path';import {pathToFileURL} from 'node:url';
import {createLocalApprovalVerifier,localPayloadBytes,type LocalRecord,type TrustedContext,type LocalVerifierOptions} from '../../tools/local_approval_verifier';
const root=mkdtempSync(join(tmpdir(),'jadpo-local-approval-'));
const before=join(root,'before'),after=join(root,'after');mkdirSync(before);mkdirSync(after);
const compiler=resolve(process.env.JADPO_BIN??'jadpo/target/debug/jadpo'),workspace=resolve('jadpo');
const dependencies=resolve(process.env.JADPO_JWT_DEPENDENCIES??'build/validation/jwt-dependencies');
const evidence=process.env.JADPO_LOCAL_APPROVAL_EVIDENCE?resolve(process.env.JADPO_LOCAL_APPROVAL_EVIDENCE):null;
if(evidence)mkdirSync(evidence,{recursive:true});
const sha=(v:Uint8Array|string)=>'sha256:'+createHash('sha256').update(v).digest('hex');
function filePins(directory:string){const result:Record<string,string>={};function walk(dir:string){for(const entry of readdirSync(dir,{withFileTypes:true})){const path=join(dir,entry.name);if(entry.isDirectory())walk(path);else if(entry.isFile())result[relative(directory,path).replaceAll('\\','/')]=sha(readFileSync(path));else throw Error('Unexpected fixture dependency shape');}}walk(directory);return result;}
const SOURCE=`entity Customer { id:Uuid policy {Access.public:[read]} }
input FindCustomer {id:Customer.id}
failure MissingCustomer {kind:NotFound code:"missing_customer" message:"Customer missing."}
action leaf(input:FindCustomer) fails MissingCustomer ->Customer {policy {Access.public:[invoke]} return attempt query required Customer {where:id==input.id missing:MissingCustomer}}
action middle(input:FindCustomer) fails MissingCustomer ->Customer {return attempt leaf(input)}
action outer(input:FindCustomer) fails MissingCustomer ->Customer {return attempt middle(input)}
route POST /customer {auth:none input:FindCustomer output:Customer run:outer(input)}
`;
writeFileSync(join(before,'app.jadpo'),SOURCE);writeFileSync(join(after,'app.jadpo'),SOURCE.replace('run:outer(input)','run:middle(input)').replace('code:"missing_customer"','code:"missing_customer_changed"').replace('Customer missing.','Café́ 🐙'));
function derive(beforePath=before,afterPath=after){const result=Bun.spawnSync([compiler,'approval',afterPath,'--before',beforePath],{env:{},stdout:'pipe',stderr:'pipe'});if(result.exitCode)throw Error(`Native fixture failed: ${result.stdout}\n${result.stderr}`);const exported=new TextDecoder().decode(result.stdout).replace(/\n$/,'');return {exported,subject:JSON.parse(exported)};}
let initial:ReturnType<typeof derive>,opts:LocalVerifierOptions,keyPair:CryptoKeyPair,jose:any,baseContext:TrustedContext;
beforeAll(async()=>{
 initial=derive();expect(initial.subject.canonical.decisions.length).toBeGreaterThan(1);
 keyPair=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']) as CryptoKeyPair;
 jose=await import(pathToFileURL(join(dependencies,'node_modules/jose/dist/webapi/index.js')).href);
 baseContext={changeId:'local-change-42',implementers:['implementation-agent'],now:2_000_000_000,issuers:[{identity:'explicit-test-issuer',keyId:'local-ephemeral-key',publicKey:keyPair.publicKey,reviewers:[{identity:'human-test-reviewer',roles:['policy_reviewer'],decisionIds:initial.subject.canonical.decisions.map((d:any)=>d.id)}]}],revokedApprovalIds:new Set(),supersededApprovalIds:new Set(),oneTime:true};
 const approvedBytes=readFileSync('tests/validation/evidence/golden-loop-runtime-provenance/local-runtime-jwt-provenance.json');expect(sha(approvedBytes)).toBe('sha256:fc8f7e84e7b4694b47a0623ae17ee72224a2b3e50175c9a1cd39cf5b94d9ad09');
 const approvedFiles=JSON.parse(approvedBytes.toString()).jwt.installed_files;const dependencyPins=Object.fromEntries(Object.entries(approvedFiles).map(([name,pin])=>[name,'sha256:'+pin]));expect(filePins(join(dependencies,'node_modules/jose'))).toEqual(dependencyPins);
 opts={native:{compilerPath:compiler,compilerSha256:sha(readFileSync(compiler)),compilerWorkspace:workspace,beforePath:before,afterPath:after,beforeSourceDigest:initial.subject.canonical.before.source_digest,beforeStateDigest:initial.subject.canonical.before.state_digest,afterSourceDigest:initial.subject.canonical.after.source_digest,afterStateDigest:initial.subject.canonical.after.state_digest},dependencyDirectory:dependencies,dependencyFiles:dependencyPins,context:()=>baseContext};
 if(evidence){writeFileSync(join(evidence,'native-subject.json'),initial.exported+'\n');writeFileSync(join(evidence,'before-app.jadpo'),SOURCE);writeFileSync(join(evidence,'after-app.jadpo'),readFileSync(join(after,'app.jadpo')));}
});
afterAll(()=>rmSync(root,{recursive:true,force:true}));
const contextCopy=():TrustedContext=>({...baseContext,implementers:[...baseContext.implementers],issuers:baseContext.issuers.map(i=>({...i,reviewers:i.reviewers.map(r=>({...r,roles:[...r.roles],decisionIds:[...r.decisionIds]}))})),revokedApprovalIds:new Set(),supersededApprovalIds:new Set()});
async function setup(context= contextCopy(),override:Partial<LocalVerifierOptions>={}){const verifier=await createLocalApprovalVerifier({...opts,...override,context:()=>context}),inspection=verifier.inspect();return {verifier,inspection,context};}
function records(inspection:any):LocalRecord[]{return inspection.decisions.map((d:any,index:number)=>({schema_version:1,kind:'local_test_approval',release_authority:false,approval_id:`approval-${index}`,issuer:'explicit-test-issuer',reviewer:'human-test-reviewer',role:'policy_reviewer',change_id:'local-change-42',binding:{...inspection.binding},decision_id:d.id,decision:'approve',alternative:'accept_exact_after',native_delta_json:d.nativeJson,rationale:'Review the exact native change for this local test.',issued_at:1_999_999_990,expires_at:2_000_000_100,supersedes:null}));}
async function sign(record:LocalRecord,header:any={alg:'EdDSA',kid:'local-ephemeral-key',typ:'jadpo-local-approval+jws'},key=keyPair.privateKey,payload=localPayloadBytes(record)){return new jose.CompactSign(payload).setProtectedHeader(header).sign(key);}
const envelope=(signed:string[])=>JSON.stringify({schema_version:1,kind:'local_test_approval_set',records:signed});
async function signedSet(list:LocalRecord[]){return envelope(await Promise.all(list.map(r=>sign(r))));}
async function refuse(code:string,mutate:(list:LocalRecord[],ctx:TrustedContext)=>void,context=contextCopy()){
 const {verifier,inspection}=await setup(context);const list=records(inspection);mutate(list,context);
 await expect(verifier.verify(inspection.export,await signedSet(list))).rejects.toMatchObject({code});expect(verifier.consumedCount).toBe(0);
 // Entire untouched set remains usable after the failed operation.
 context.revokedApprovalIds=new Set();context.supersededApprovalIds=new Set();
}

test('all local positives remain explicitly non-releasable; complete signed set consumes atomically and replay refuses',async()=>{
 const {verifier,inspection}=await setup();const set=await signedSet(records(inspection));const result=await verifier.verify(inspection.export,set);
 expect(result).toEqual({status:'local_test_non_releasable',release_authority:false,consumed:inspection.decisions.length,replay_scope:'test_verifier_instance_memory_only'});
 await expect(verifier.verify(inspection.export,set)).rejects.toMatchObject({code:'consumed_approval'});expect(verifier.consumedCount).toBe(inspection.decisions.length);
 if(evidence)writeFileSync(join(evidence,'local-positive-result.json'),JSON.stringify(result,null,2)+'\n');
});
test('durable local consumption survives a new verifier and native database reopen without conferring release authority',async()=>{
 const path=join(root,'reopened-approval-ledger.sqlite');let database=new Database(path);
 try {
  const ledger=createLocalConsumptionLedger(database),{verifier,inspection}=await setup(contextCopy(),{ledger});
  const original=records(inspection),set=await signedSet(original);
  await expect(verifier.verify(inspection.export,await signedSet(original.slice(0,-1)))).rejects.toMatchObject({code:'incomplete_decisions'});
  expect(ledger.consumedCount).toBe(0);
  expect(await verifier.verify(inspection.export,set)).toEqual({status:'local_test_non_releasable',release_authority:false,consumed:original.length,replay_scope:'caller_owned_sqlite_ledger'});
  const rows=database.query('SELECT approval_id,descriptor_json,descriptor_digest FROM __jadpo_local_approval_consumption ORDER BY approval_id').all() as any[];
  expect(rows.length).toBe(original.length);
  for(const row of rows){const record=original.find(value=>value.approval_id===row.approval_id)!;expect(row.descriptor_json).toBe(new TextDecoder().decode(localPayloadBytes(record)));expect(row.descriptor_digest).toBe(sha(localPayloadBytes(record)));}
  database.close();database=new Database(path);
  const reopened=createLocalConsumptionLedger(database),fresh=await setup(contextCopy(),{ledger:reopened});
  await expect(fresh.verifier.verify(fresh.inspection.export,set)).rejects.toMatchObject({code:'consumed_approval'});
  expect(reopened.consumedCount).toBe(original.length);
 }finally{database.close();}
});
test('two verifier instances sharing native durable storage admit exactly one complete signed set',async()=>{
 const path=join(root,'concurrent-approval-ledger.sqlite'),a=new Database(path),b=new Database(path);
 try {
  const one=await setup(contextCopy(),{ledger:createLocalConsumptionLedger(a)}),two=await setup(contextCopy(),{ledger:createLocalConsumptionLedger(b)}),set=await signedSet(records(one.inspection));
  const results=await Promise.allSettled([one.verifier.verify(one.inspection.export,set),two.verifier.verify(two.inspection.export,set)]);
  expect(results.filter(value=>value.status==='fulfilled').length).toBe(1);
  const refusal=results.find(value=>value.status==='rejected') as PromiseRejectedResult;expect(refusal.reason).toMatchObject({code:'consumed_approval'});
  expect(one.verifier.consumedCount).toBe(one.inspection.decisions.length);expect(two.verifier.consumedCount).toBe(one.inspection.decisions.length);
 }finally{a.close();b.close();}
});
test('actual native ledger write lock refuses without consumption and the identical signed set works after release',async()=>{
 const path=join(root,'locked-approval-ledger.sqlite'),database=new Database(path),locker=new Database(path);
 try {
  const ledger=createLocalConsumptionLedger(database),{verifier,inspection}=await setup(contextCopy(),{ledger}),set=await signedSet(records(inspection));
  locker.run('BEGIN IMMEDIATE');const started=performance.now();
  expect(()=>ledger.consume(records(inspection),baseContext.now,localPayloadBytes)).toThrow(expect.objectContaining({code:'ledger_unavailable'}));
  // Bound the native storage cut separately from full compiler rederivation.
  expect(performance.now()-started).toBeLessThan(100);
  await expect(verifier.verify(inspection.export,set)).rejects.toMatchObject({code:'ledger_unavailable'});
  expect(ledger.consumedCount).toBe(0);
  locker.run('ROLLBACK');expect((await verifier.verify(inspection.export,set)).consumed).toBe(inspection.decisions.length);
 }finally{if(locker.inTransaction)locker.run('ROLLBACK');locker.close();database.close();}
});
test('ledger schema tampering and caller-owned transactions fail closed before signed-record consumption',async()=>{
 const database=new Database(join(root,'tampered-approval-ledger.sqlite'));
 try {
  const ledger=createLocalConsumptionLedger(database),{verifier,inspection}=await setup(contextCopy(),{ledger}),set=await signedSet(records(inspection));
  database.run('BEGIN IMMEDIATE');await expect(verifier.verify(inspection.export,set)).rejects.toMatchObject({code:'ledger_transaction_already_owned'});database.run('ROLLBACK');
  database.run("CREATE TRIGGER untrusted_consumption AFTER INSERT ON __jadpo_local_approval_consumption BEGIN DELETE FROM __jadpo_local_approval_consumption; END");
  await expect(verifier.verify(inspection.export,set)).rejects.toMatchObject({code:'ledger_schema_mismatch'});
  expect((database.query('SELECT count(*) AS n FROM __jadpo_local_approval_consumption').get() as any).n).toBe(0);
  database.run('DROP TRIGGER untrusted_consumption');expect((await verifier.verify(inspection.export,set)).consumed).toBe(inspection.decisions.length);
 }finally{database.close();}
});
test('durable ledger rejects memory, disabled journalling or asynchronous durability; forged adapters cannot be substituted',async()=>{
 const memory=new Database(':memory:'),database=new Database(join(root,'invalid-approval-ledger.sqlite'));
 try {
  expect(()=>createLocalConsumptionLedger(memory)).toThrow(expect.objectContaining({code:'durable_ledger_required'}));
  // Bun1.2.20's native defensive setting ignores journal_mode=OFF; MEMORY
  // actually changes the mode and exercises the non-durable storage guard.
  database.run('PRAGMA journal_mode=MEMORY');expect((database.query('PRAGMA journal_mode').get() as any).journal_mode).toBe('memory');expect(()=>createLocalConsumptionLedger(database)).toThrow(expect.objectContaining({code:'durable_ledger_required'}));
  database.run('PRAGMA journal_mode=DELETE');database.run('PRAGMA synchronous=OFF');expect(()=>createLocalConsumptionLedger(database)).toThrow(expect.objectContaining({code:'durable_ledger_required'}));
  await expect(setup(contextCopy(),{ledger:{replayScope:'caller_owned_sqlite_ledger',has:()=>false,consume:()=>{},consumedCount:0} as any})).rejects.toMatchObject({code:'native_ledger_required'});
 }finally{memory.close();database.close();}
});
test('connection-local temporary schema objects cannot change the caller-owned durable ledger contract',async()=>{
 const variants=[
  {create:'CREATE TEMP TABLE __jadpo_local_approval_consumption (placeholder TEXT)',drop:'DROP TABLE temp.__jadpo_local_approval_consumption'},
  {create:'CREATE TEMP VIEW __jadpo_local_approval_consumption AS SELECT 1 AS placeholder',drop:'DROP VIEW temp.__jadpo_local_approval_consumption'},
  {create:'CREATE TEMP TABLE __jadpo_local_approval_consumption_version (placeholder TEXT)',drop:'DROP TABLE temp.__jadpo_local_approval_consumption_version'},
  {create:'CREATE TEMP TRIGGER ledger_test_observer AFTER INSERT ON main.__jadpo_local_approval_consumption BEGIN SELECT 1; END',drop:'DROP TRIGGER temp.ledger_test_observer'},
  {create:'CREATE TEMP TABLE __JADPO_LOCAL_APPROVAL_CONSUMPTION (placeholder TEXT)',drop:'DROP TABLE temp.__JADPO_LOCAL_APPROVAL_CONSUMPTION'},
  {create:'CREATE TEMP VIEW __JADPO_LOCAL_APPROVAL_CONSUMPTION_VERSION AS SELECT 1 AS placeholder',drop:'DROP VIEW temp.__JADPO_LOCAL_APPROVAL_CONSUMPTION_VERSION'},
  {create:'CREATE TEMP TRIGGER ledger_test_upper_temp AFTER INSERT ON main."__JADPO_LOCAL_APPROVAL_CONSUMPTION" BEGIN SELECT 1; END',drop:'DROP TRIGGER temp.ledger_test_upper_temp'},
  {create:'CREATE TRIGGER ledger_test_upper_main AFTER INSERT ON main."__JADPO_LOCAL_APPROVAL_CONSUMPTION" BEGIN SELECT 1; END',drop:'DROP TRIGGER main.ledger_test_upper_main'},
  {create:'CREATE TRIGGER ledger_test_upper_version AFTER INSERT ON main."__JADPO_LOCAL_APPROVAL_CONSUMPTION_VERSION" BEGIN SELECT 1; END',drop:'DROP TRIGGER main.ledger_test_upper_version'},
 ];
 for(const [index,variant] of variants.entries()){
  const database=new Database(join(root,`temporary-schema-${index}.sqlite`));
  try{
   const ledger=createLocalConsumptionLedger(database),{verifier,inspection}=await setup(contextCopy(),{ledger}),set=await signedSet(records(inspection));
   database.run(variant.create);
   await expect(verifier.verify(inspection.export,set)).rejects.toMatchObject({code:'ledger_schema_mismatch'});
   expect((database.query('SELECT count(*) AS n FROM main.__jadpo_local_approval_consumption').get() as any).n).toBe(0);
   expect(()=>createLocalConsumptionLedger(database)).toThrow(expect.objectContaining({code:'ledger_schema_mismatch'}));
   database.run(variant.drop);expect((await verifier.verify(inspection.export,set)).consumed).toBe(inspection.decisions.length);
  }finally{database.close();}
 }
});
test('native descriptor insert failure after an earlier row rolls back the entire ledger set',async()=>{
 const database=new Database(join(root,'rollback-approval-ledger.sqlite'));
 try {
  const ledger=createLocalConsumptionLedger(database),{inspection}=await setup(),list=records(inspection);
  expect(list.length).toBeGreaterThan(1);const malformed=structuredClone(list);(malformed[1] as any).issuer=null;
  // This deliberately injects malformed storage input below the signed verifier,
  // provoking SQLite NOT NULL after the first actual insert, rather than a JS fault.
  expect(()=>ledger.consume(malformed,baseContext.now,localPayloadBytes)).toThrow(expect.objectContaining({code:'ledger_unavailable'}));
  expect(ledger.consumedCount).toBe(0);ledger.consume(list,baseContext.now,localPayloadBytes);expect(ledger.consumedCount).toBe(list.length);
 }finally{database.close();}
});
test('AP-01: ordinary repository approval JSON and JWS-supplied key material confer no issuer authority',async()=>{
 const {verifier,inspection}=await setup();await expect(verifier.verify(inspection.export,JSON.stringify({approval:true}))).rejects.toMatchObject({code:'closed_shape'});expect(verifier.consumedCount).toBe(0);
 const record=records(inspection)[0];const signed=await sign(record,{alg:'EdDSA',kid:'local-ephemeral-key',typ:'jadpo-local-approval+jws',jwk:{kty:'OKP',crv:'Ed25519',x:'untrusted'}});
 await expect(verifier.verify(inspection.export,envelope([signed]))).rejects.toMatchObject({code:'closed_shape'});expect(verifier.consumedCount).toBe(0);
});
test('AP-02: copied subject digest or change identity refuses',async()=>{await refuse('subject_digest_mismatch',list=>list[0].binding.subjectDigest='sha256:'+'0'.repeat(64));await refuse('change_mismatch',list=>list[0].change_id='other-change');});
test('AP-03: graph digest differs with matching policy',async()=>refuse('graph_digest_mismatch',list=>list[0].binding.afterGraphDigest='sha256:'+'0'.repeat(64)));
test('AP-04: policy digest differs with matching graph',async()=>refuse('policy_digest_mismatch',list=>list[0].binding.afterPolicyDigest='sha256:'+'0'.repeat(64)));
test('AP-05: implementation identity cannot act as reviewer',async()=>refuse('separation_of_authority',list=>list[0].reviewer='implementation-agent'));
test('AP-06: exact reviewer role and decision entitlements required',async()=>{
 await refuse('authority_mismatch',list=>list[0].role='administrator');await refuse('authority_mismatch',(_,ctx)=>(ctx.issuers[0].reviewers[0].decisionIds as string[]).splice(0,1));
});
test('AP-07: expired, revoked, superseded, future-issued and invalid expiry all refuse',async()=>{
 await refuse('inactive_approval',list=>list[0].expires_at=2_000_000_000);
 await refuse('inactive_approval',(list,ctx)=>(ctx.revokedApprovalIds as Set<string>).add(list[0].approval_id));
 await refuse('inactive_approval',(list,ctx)=>(ctx.supersededApprovalIds as Set<string>).add(list[0].approval_id));
 await refuse('inactive_approval',list=>list[0].issued_at=2_000_000_001);
 await refuse('inactive_approval',list=>list[0].expires_at=list[0].issued_at);
});
test('AP-08: missing, duplicate and foreign decisions fail without partial consumption',async()=>{
 await refuse('incomplete_decisions',list=>list.pop());await refuse('duplicate_approval',list=>list.push({...list[0]}));
 await refuse('duplicate_decision',list=>list.push({...list[0],approval_id:'different-id'}));
 const {verifier,inspection}=await setup();const original=records(inspection),list=[...original.slice(0,-1)];await expect(verifier.verify(inspection.export,await signedSet(list))).rejects.toMatchObject({code:'incomplete_decisions'});expect(verifier.consumedCount).toBe(0);
 expect((await verifier.verify(inspection.export,await signedSet(original))).consumed).toBe(original.length);
});
test('AP-09: compiler, schema, canonicalization and binary provenance cannot be substituted',async()=>{
 for(const field of ['compilerVersion','policySchemaVersion','approvalSchemaVersion','canonicalization','compilerBinaryDigest','compilerManifestDigest'] as const)await refuse('version_mismatch',list=>{(list[0].binding as any)[field]=typeof list[0].binding[field]==='number'?99:'wrong';});
});
test('AP-10: independently copied source checkout accepted only with identical native canonical subject',async()=>{
 const relocated=join(root,'relocated');mkdirSync(relocated);cpSync(before,join(relocated,'before'),{recursive:true});cpSync(after,join(relocated,'after'),{recursive:true});
 const clone=derive(join(relocated,'before'),join(relocated,'after'));expect(clone.exported).toBe(initial.exported);
 const {verifier,inspection}=await setup(contextCopy(),{native:{...opts.native,beforePath:join(relocated,'before'),afterPath:join(relocated,'after')}});
 expect((await verifier.verify(inspection.export,await signedSet(records(inspection)))).release_authority).toBe(false);
});
test('AP-11: actual native transitive service effect omission cannot be hidden by a coherently rehashed export',async()=>{
 const serviceRoot=join(root,'service');mkdirSync(serviceRoot);writeFileSync(join(serviceRoot,'.git'),'immutable test root marker');mkdirSync(join(serviceRoot,'tests/assurance'),{recursive:true});cpSync('tests/assurance/service-reference-mail-v0.1.json',join(serviceRoot,'tests/assurance/service-reference-mail-v0.1.json'));
 const b=join(serviceRoot,'before'),a=join(serviceRoot,'after');mkdirSync(b);mkdirSync(a);const base=readFileSync('tests/compile/pass/170_checked_service_operation.jadpo','utf8');writeFileSync(join(b,'app.jadpo'),base);
 writeFileSync(join(a,'app.jadpo'),base+`\naction send_via_nested(mail:ReminderMessage) fails ReminderRecipientRejected,ReminderTemporarilyUnavailable,Misconfigured,Unavailable,OutcomeUnknown ->ReminderReceipt {return attempt deliver(mail)}\nroute POST /send-mail {auth:none input:ReminderMessage output:ReminderReceipt run:send_via_nested(input)}\n`);
 const actual=derive(b,a),c=actual.subject.canonical;expect(c.after.effect_paths['POST /send-mail']['ReminderMail.send_overdue_reminder:external_service:ReminderMail.send_overdue_reminder'].path).toEqual(['POST /send-mail','send_via_nested','deliver','ReminderMail.send_overdue_reminder']);
 const native={...opts.native,beforePath:b,afterPath:a,beforeSourceDigest:c.before.source_digest,beforeStateDigest:c.before.state_digest,afterSourceDigest:c.after.source_digest,afterStateDigest:c.after.state_digest};
 const ctx=contextCopy();ctx.issuers[0].reviewers[0].decisionIds=c.decisions.map((d:any)=>d.id);const {verifier,inspection}=await setup(ctx,{native});
 const omitted=structuredClone(actual.subject);delete omitted.canonical.after.effect_paths['POST /send-mail']['ReminderMail.send_overdue_reminder:external_service:ReminderMail.send_overdue_reminder'];
 const canonical=(v:any):string=>Array.isArray(v)?'['+v.map(canonical).join(',')+']':v!==null&&typeof v==='object'?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canonical(v[k])).join(',')+'}':JSON.stringify(v);
 omitted.subject_digest=sha(canonical(omitted.canonical));await expect(verifier.verify(canonical(omitted),await signedSet(records(inspection)))).rejects.toMatchObject({code:'review_surface_nonconformance'});expect(verifier.consumedCount).toBe(0);
 if(evidence){writeFileSync(join(evidence,'native-service-subject.json'),inspection.export+'\n');writeFileSync(join(evidence,'omitted-service-export.json'),canonical(omitted)+'\n');}
});
test('AP-12: absent protected provider yields local inspection only, never release success',async()=>{
 const ctx=contextCopy();ctx.issuers=[];const {verifier,inspection}=await setup(ctx);expect(await verifier.verify(inspection.export,envelope([]))).toEqual({status:'local_inspection_only_non_releasable',release_authority:false,consumed:0,replay_scope:'test_verifier_instance_memory_only'});expect(verifier.consumedCount).toBe(0);
});
test('invalid signatures, private/foreign keys, algorithm confusion, duplicate JSON keys and malformed envelopes fail closed',async()=>{
 const {verifier,inspection}=await setup();const list=records(inspection),valid=await sign(list[0]),parts=valid.split('.');parts[2]=parts[2].slice(0,-1)+(parts[2].endsWith('A')?'B':'A');await expect(verifier.verify(inspection.export,envelope([parts.join('.')]))).rejects.toMatchObject({code:expect.stringMatching(/invalid_signature|malformed_jws/)});
 const foreign=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']) as CryptoKeyPair;await expect(verifier.verify(inspection.export,envelope([await sign(list[0],undefined,foreign.privateKey)]))).rejects.toMatchObject({code:'invalid_signature'});
 const duplicate=new TextEncoder().encode(new TextDecoder().decode(localPayloadBytes(list[0])).replace('"approval_id":','"approval_id":"first","approval_id":'));await expect(verifier.verify(inspection.export,envelope([await sign(list[0],undefined,undefined,duplicate)]))).rejects.toMatchObject({code:'duplicate_json_key'});
 for(const bad of ['{','{"schema_version":1,"schema_version":1,"kind":"local_test_approval_set","records":[]}',JSON.stringify({schema_version:1,kind:'local_test_approval_set',records:[],extra:true}),envelope(['a.b.c'])])await expect(verifier.verify(inspection.export,bad)).rejects.toBeDefined();expect(verifier.consumedCount).toBe(0);
 const ctx=contextCopy();ctx.issuers[0].publicKey=keyPair.privateKey;await expect((await setup(ctx)).verifier.verify(inspection.export,await signedSet(list))).rejects.toMatchObject({code:'invalid_trusted_public_key'});
});
test('narrow alternatives, exact native delta, rationale and immutable release scope required',async()=>{
 await refuse('exact_delta_mismatch',list=>list[0].native_delta_json='{}');await refuse('unapproved_decision',list=>list[0].alternative='retain_before');await refuse('unapproved_decision',list=>list[0].decision='request_clarification');await refuse('missing_value',list=>list[0].rationale=' ');await refuse('invalid_local_scope',list=>(list[0] as any).release_authority=true);
});
test('concurrent verification consumes exactly one complete set, and context mutation after signature awaits consumes nothing',async()=>{
 const {verifier,inspection}=await setup(),set=await signedSet(records(inspection));const outcomes=await Promise.allSettled([verifier.verify(inspection.export,set),verifier.verify(inspection.export,set)]);expect(outcomes.filter(o=>o.status==='fulfilled').length).toBe(1);expect(verifier.consumedCount).toBe(inspection.decisions.length);
 const ctx=contextCopy(),other=await setup(ctx),signed=await signedSet(records(other.inspection));const pending=other.verifier.verify(other.inspection.export,signed);ctx.now++;await expect(pending).rejects.toMatchObject({code:'trusted_context_changed'});expect(other.verifier.consumedCount).toBe(0);
});
test('fresh native rederivation, exact canonical export and dependency byte pins are indispensable',async()=>{
 const {verifier,inspection}=await setup();await expect(verifier.verify(JSON.stringify(JSON.parse(inspection.export),null,2),await signedSet(records(inspection)))).rejects.toMatchObject({code:'review_surface_nonconformance'});
 const wrong={...opts.dependencyFiles};wrong[Object.keys(wrong)[0]]='sha256:'+'0'.repeat(64);await expect(createLocalApprovalVerifier({...opts,dependencyFiles:wrong})).rejects.toMatchObject({code:'dependency_bytes_mismatch'});
 await expect(createLocalApprovalVerifier({...opts,native:{...opts.native,compilerSha256:'sha256:'+'0'.repeat(64)}}).then(v=>v.inspect())).rejects.toMatchObject({code:'compiler_bytes_mismatch'});
 const source=readFileSync(join(after,'app.jadpo'));try{writeFileSync(join(after,'app.jadpo'),source+'\n');await expect(verifier.verify(inspection.export,await signedSet(records(inspection)))).rejects.toMatchObject({code:'source_state_mismatch'});}finally{writeFileSync(join(after,'app.jadpo'),source);}expect(verifier.consumedCount).toBe(0);
});


test('native Unicode decision fragments remain exact; malformed signed records and unknown entitlements refuse',async()=>{
 const {verifier,inspection}=await setup();expect(inspection.decisions.some(d=>d.nativeJson.includes('Café́ 🐙'))).toBe(true);
 await refuse('foreign_decision',(list,ctx)=>{const foreign='sha256:'+'f'.repeat(64);list[0].decision_id=foreign;(ctx.issuers[0].reviewers[0].decisionIds as string[]).push(foreign);});
 const list=records(inspection);
 for(const transform of [(s:string)=>s.replace('"issued_at":1999999990','"issued_at":1e309'),(s:string)=>s.replace('"rationale":','"extra":true,"rationale":'),(s:string)=>s.replace(/,"rationale":"[^"]*"/,'')]){
 const payload=new TextEncoder().encode(transform(new TextDecoder().decode(localPayloadBytes(list[0]))));await expect(verifier.verify(inspection.export,envelope([await sign(list[0],undefined,undefined,payload)]))).rejects.toBeDefined();expect(verifier.consumedCount).toBe(0);}
 const malformedHeader='{ "alg":"EdDSA", "alg":"EdDSA", "kid":"local-ephemeral-key", "typ":"jadpo-local-approval+jws" }';const h=Buffer.from(malformedHeader).toString('base64url'),p=Buffer.from(localPayloadBytes(list[0])).toString('base64url');const sig=(await sign(list[0])).split('.')[2];
 await expect(verifier.verify(inspection.export,envelope([h+'.'+p+'.'+sig]))).rejects.toMatchObject({code:'duplicate_json_key'});expect(verifier.consumedCount).toBe(0);
 const noIssuer=contextCopy();noIssuer.issuers=[];const missing=await setup(noIssuer);await expect(missing.verifier.verify(missing.inspection.export,await signedSet(list))).rejects.toMatchObject({code:'untrusted_issuer'});
});
test('explicit supersession, optional expiry and revocation arriving during verification respect trusted context',async()=>{
 const ctx=contextCopy();(ctx.supersededApprovalIds as Set<string>).add('old-record');const {verifier,inspection}=await setup(ctx);const list=records(inspection);list[0].supersedes='old-record';list[0].expires_at=null;expect((await verifier.verify(inspection.export,await signedSet(list))).consumed).toBe(list.length);
 await refuse('invalid_supersession',rows=>rows[0].supersedes='unconfirmed-predecessor');
 const raceCtx=contextCopy(),race=await setup(raceCtx),raceList=records(race.inspection),signed=await signedSet(raceList);const pending=race.verifier.verify(race.inspection.export,signed);(raceCtx.revokedApprovalIds as Set<string>).add(raceList[0].approval_id);await expect(pending).rejects.toMatchObject({code:'trusted_context_changed'});expect(race.verifier.consumedCount).toBe(0);
});


test('actual compiler source, registry and installed dependency byte drift refuse independently of supplied labels',async()=>{
 const cloneWorkspace=join(root,'compiler-copy');mkdirSync(cloneWorkspace);const inputs=initial.subject.canonical.after.provenance.compiler.files;
 for(const name of Object.keys(inputs)){const dest=join(cloneWorkspace,name);mkdirSync(join(dest,'..'),{recursive:true});cpSync(join(workspace,name),dest);}
 const {verifier,inspection}=await setup(contextCopy(),{native:{...opts.native,compilerWorkspace:cloneWorkspace}});expect(inspection.export).toBe(initial.exported);
 const edited=join(cloneWorkspace,'crates/core/src/lib.rs'),original=readFileSync(edited);writeFileSync(edited,Buffer.concat([original,Buffer.from('\n')]));await expect(verifier.verify(inspection.export,await signedSet(records(inspection)))).rejects.toMatchObject({code:'compiler_source_mismatch'});writeFileSync(edited,original);expect(verifier.consumedCount).toBe(0);
 const cache=join(root,'dependency-copy');cpSync(dependencies,cache,{recursive:true});const dependency=join(cache,'node_modules/jose/dist/webapi/index.js');writeFileSync(dependency,readFileSync(dependency,'utf8')+'\n');await expect(createLocalApprovalVerifier({...opts,dependencyDirectory:cache})).rejects.toMatchObject({code:'dependency_bytes_mismatch'});
 const registry=join(after,'schema.identities.json');try{writeFileSync(registry,'{}');await expect(verifier.verify(inspection.export,await signedSet(records(inspection)))).rejects.toBeDefined();}finally{rmSync(registry,{force:true});}expect(verifier.consumedCount).toBe(0);
 const invalid=records(inspection)[0];invalid.expires_at=Number.NaN;expect(()=>localPayloadBytes(invalid)).toThrow();
 const valid=await sign(records(inspection)[0]),parts=valid.split('.');parts[0]=Buffer.from(JSON.stringify({alg:'none',kid:'local-ephemeral-key',typ:'jadpo-local-approval+jws'})).toString('base64url');await expect(verifier.verify(inspection.export,envelope([parts.join('.')]))).rejects.toMatchObject({code:'algorithm_mismatch'});
});




test('strict selected source, compiler and dependency roots refuse caller-created symlink paths',async()=>{
 const good=await setup(),signed=await signedSet(records(good.inspection));
 const refusal=async(native:Partial<LocalVerifierOptions['native']>,code:string,dependencyDirectory=dependencies)=>{
  const attempt=async()=>{const candidate=await createLocalApprovalVerifier({...opts,native:{...opts.native,...native},dependencyDirectory});try{return await candidate.verify(good.inspection.export,signed);}finally{expect(candidate.consumedCount).toBe(0);}};
  await expect(attempt()).rejects.toMatchObject({code});
 };
 const beforeLink=join(root,'before-root-link'),afterLink=join(root,'after-root-link');symlinkSync(before,beforeLink,'dir');symlinkSync(after,afterLink,'dir');
 await refusal({beforePath:beforeLink},'project_symlink');await refusal({afterPath:afterLink},'project_symlink');
 const ancestor=join(root,'source-ancestor-link');symlinkSync(root,ancestor,'dir');await refusal({beforePath:ancestor+'/before'},'project_symlink');await refusal({afterPath:ancestor+'/after'},'project_symlink');
 // Checking the lexical components before `..` prevents normalization from
 // erasing a caller-created alias that the filesystem would otherwise follow.
 await refusal({beforePath:beforeLink+'/../before'},'project_symlink');
 const binaryLink=join(root,'compiler-file-link');symlinkSync(compiler,binaryLink,'file');await refusal({compilerPath:binaryLink},'compiler_symlink');
 const binaryAncestor=join(root,'compiler-binary-ancestor-link');symlinkSync(resolve(compiler,'..'),binaryAncestor,'dir');await refusal({compilerPath:binaryAncestor+'/jadpo'},'compiler_symlink');
 const compilerRoot=join(root,'compiler-root-link');symlinkSync(workspace,compilerRoot,'dir');await refusal({compilerWorkspace:compilerRoot},'compiler_symlink');
 const compilerParent=join(root,'compiler-ancestor-link');symlinkSync(resolve(workspace,'..'),compilerParent,'dir');await refusal({compilerWorkspace:compilerParent+'/jadpo'},'compiler_symlink');
 const cacheRoot=join(root,'dependency-root-link');symlinkSync(dependencies,cacheRoot,'dir');await refusal({},'dependency_symlink',cacheRoot);
 const cacheAncestor=join(root,'dependency-ancestor-link');symlinkSync(resolve(dependencies,'..'),cacheAncestor,'dir');await refusal({},'dependency_symlink',cacheAncestor+'/jwt-dependencies');
 const cache=join(root,'symlink-cache-copy');cpSync(dependencies,cache,{recursive:true});const jose=join(cache,'node_modules/jose'),retained=join(cache,'node_modules/jose-retained');renameSync(jose,retained);symlinkSync(retained,jose,'dir');await refusal({},'dependency_symlink',cache);
 const intact=await good.verifier.verify(good.inspection.export,signed);expect(intact.release_authority).toBe(false);expect(intact.consumed).toBe(good.inspection.decisions.length);
});

test('strict root guards after signature awaits consume zero; verified OS aliases preserve unchanged native inputs',async()=>{
 const candidate=await setup(),signed=await signedSet(records(candidate.inspection));const retained=before+'-retained';
 const pending=candidate.verifier.verify(candidate.inspection.export,signed);renameSync(before,retained);symlinkSync(retained,before,'dir');
 try{await expect(pending).rejects.toMatchObject({code:'project_symlink'});expect(candidate.verifier.consumedCount).toBe(0);}finally{rmSync(before);renameSync(retained,before);}
 expect((await candidate.verifier.verify(candidate.inspection.export,signed)).consumed).toBe(candidate.inspection.decisions.length);
 const cache=join(root,'await-cache-copy'),cacheRetained=cache+'-retained';cpSync(dependencies,cache,{recursive:true});
 const dep=await setup(contextCopy(),{dependencyDirectory:cache}),depSigned=await signedSet(records(dep.inspection)),depPending=dep.verifier.verify(dep.inspection.export,depSigned);renameSync(cache,cacheRetained);symlinkSync(cacheRetained,cache,'dir');
 try{await expect(depPending).rejects.toMatchObject({code:'dependency_symlink'});expect(dep.verifier.consumedCount).toBe(0);}finally{rmSync(cache);renameSync(cacheRetained,cache);}
 expect((await dep.verifier.verify(dep.inspection.export,depSigned)).consumed).toBe(dep.inspection.decisions.length);

 const alias=(path:string)=>path.startsWith('/private/tmp/')?path.replace('/private/tmp/','/tmp/'):path.startsWith('/private/var/')?path.replace('/private/var/','/var/'):path;
 const native={...opts.native,compilerPath:alias(compiler),compilerWorkspace:alias(workspace),beforePath:alias(before),afterPath:alias(after)};
 const ordinary=await setup(contextCopy(),{native});expect(ordinary.inspection.export).toBe(initial.exported);expect((await ordinary.verifier.verify(ordinary.inspection.export,await signedSet(records(ordinary.inspection)))).release_authority).toBe(false);
});
