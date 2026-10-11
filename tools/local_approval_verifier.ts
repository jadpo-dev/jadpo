/** Test-process approval inspection only. This API can never authorize release.
 * Native canonical export bytes own conformance; JOSE owns EdDSA JWS verification.
 * All keys, identities, time and replay policy come from an explicit trusted caller.
 */
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, existsSync, lstatSync, readlinkSync } from 'node:fs';
import { resolve, relative, join, dirname, isAbsolute } from 'node:path';
import { pathToFileURL } from 'node:url';

export class LocalApprovalError extends Error { constructor(public readonly code:string) {super(code);} }
const fail=(code:string):never=>{throw new LocalApprovalError(code);};
const hash=(bytes:Uint8Array|string)=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const text=(bytes:Uint8Array)=>{try{return new TextDecoder('utf-8',{fatal:true}).decode(bytes);}catch{return fail('malformed_utf8');}};
const plain=(value:any)=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const keys=(value:any,expected:string[])=>{if(!plain(value)||Object.keys(value).sort().join('\0')!==[...expected].sort().join('\0'))fail('closed_shape');};
const nonempty=(value:any)=>{if(typeof value!=='string'||!value.trim())fail('missing_value');return value as string;};
const digest=(value:any)=>{if(typeof value!=='string'||!/^sha256:[a-f0-9]{64}$/.test(value))fail('invalid_digest');return value as string;};
const integer=(value:any)=>{if(!Number.isSafeInteger(value)||value<0)fail('invalid_time');return value as number;};

type Node={value:any,start:number,end:number,children?:Map<string,Node>|Node[]};
/** JSON grammar boundary: catches duplicate decoded keys before JSON.parse loses them.
 * This is not the native subject canonicalizer. Raw spans preserve native bytes.
 */
function parseStrict(input:string):Node {
 if(input.length>32*1024*1024)fail('oversized_json');let at=0;
 const space=()=>{while(/[\t\n\r ]/.test(input[at]??'x'))at++;};
 const string=()=>{const start=at;if(input[at++]!=='"')fail('malformed_json');let escaped=false;
  while(at<input.length){const c=input[at++];if(c==='"'&&!escaped){try{return JSON.parse(input.slice(start,at));}catch{fail('malformed_json');}}
   if(c==='\\'&&!escaped)escaped=true;else escaped=false;}return fail('malformed_json');};
 const value=(depth:number):Node=>{if(depth>128)fail('deep_json');space();const start=at;let result:any,children:Node['children'];
  if(input[at]==='{'){at++;result=Object.create(null);const map=new Map<string,Node>();children=map;space();
   if(input[at]!=='}')while(true){space();const key=string();if(map.has(key))fail('duplicate_json_key');space();if(input[at++]!==':')fail('malformed_json');const child=value(depth+1);map.set(key,child);result[key]=child.value;space();if(input[at]!==',')break;at++;}
   if(input[at++]!=='}')fail('malformed_json');
  }else if(input[at]==='['){at++;result=[];const list:Node[]=[];children=list;space();if(input[at]!==']')while(true){const child=value(depth+1);list.push(child);result.push(child.value);space();if(input[at]!==',')break;at++;}if(input[at++]!==']')fail('malformed_json');
  }else if(input[at]==='"')result=string();else {const token=input.slice(at).match(/^(?:true|false|null|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)/)?.[0];if(!token)fail('malformed_json');at+=token.length;result=JSON.parse(token);if(typeof result==='number'&&(!Number.isFinite(result)||!Number.isSafeInteger(result)))fail('unsupported_json_number');}
  return {value:result,start,end:at,children};};
 const root=value(0);space();if(at!==input.length)fail('malformed_json');return root;
}
const child=(node:Node,name:string):Node=>{if(!(node.children instanceof Map)||!node.children.has(name))return fail('missing_native_fact');return node.children.get(name)!;};
const canonicalLocal=(value:any):string=>{if(Array.isArray(value))return '['+value.map(canonicalLocal).join(',')+']';if(plain(value))return '{'+Object.keys(value).sort().map(k=>JSON.stringify(k)+':'+canonicalLocal(value[k])).join(',')+'}';if(typeof value==='number'&&(!Number.isFinite(value)||!Number.isSafeInteger(value)))fail('unsupported_json_number');if(value===undefined||typeof value==='bigint'||typeof value==='function'||typeof value==='symbol')fail('unsupported_local_value');return JSON.stringify(value);};
/** Refuse selected roots and every traversed ancestor, including components before
 * `..`. Only verified host aliases /tmp -> /private/tmp and /var -> /private/var
 * normalize to their physical roots. Caller-created aliases never qualify.
 * These are observed local guards, not an atomic protected-filesystem attestation.
 */
function strictPath(path:string,code:string,optionalLeaf=false):string {
 const absolute=isAbsolute(path)?path:process.cwd()+'/'+path;
 const parts=absolute.split('/').filter(part=>part!==''&&part!=='.');let current='/';
 for(let index=0;index<parts.length;index++){
  const part=parts[index];if(part==='..'){current=dirname(current);continue;}current=join(current,part);
  let info;try{info=lstatSync(current);}catch(error){if(optionalLeaf&&index===parts.length-1&&(error as any)?.code==='ENOENT')return current;throw error;}
  if(info.isSymbolicLink()){
   const expected=current==='/tmp'?'/private/tmp':current==='/var'?'/private/var':null;
   if(expected===null||resolve(dirname(current),readlinkSync(current))!==expected)fail(code);
   current=strictPath(expected,code);
  }else if(index<parts.length-1&&!info.isDirectory())fail('invalid_path_component');
 }
 return current;
}
const files=(directory:string):Record<string,string>=>{const result:Record<string,string>={};const walk=(dir:string)=>{for(const entry of readdirSync(dir,{withFileTypes:true})){const path=join(dir,entry.name);if(entry.isSymbolicLink())fail('dependency_symlink');if(entry.isDirectory())walk(path);else if(entry.isFile())result[relative(directory,path).replaceAll('\\','/')]=hash(readFileSync(path));else fail('dependency_file_kind');}};walk(directory);return result;};
const sameFiles=(a:Record<string,string>,b:Record<string,string>)=>canonicalLocal(a)===canonicalLocal(b);

export type NativeInputs={compilerPath:string,compilerSha256:string,compilerWorkspace:string,beforePath:string,afterPath:string,beforeSourceDigest:string,beforeStateDigest:string,afterSourceDigest:string,afterStateDigest:string};
export type Reviewer={identity:string,roles:readonly string[],decisionIds:readonly string[]};
export type Issuer={identity:string,keyId:string,publicKey:CryptoKey,reviewers:readonly Reviewer[]};
export type TrustedContext={changeId:string,implementers:readonly string[],now:number,issuers:readonly Issuer[],revokedApprovalIds:ReadonlySet<string>,supersededApprovalIds:ReadonlySet<string>,oneTime:true};
export type LocalVerifierOptions={native:NativeInputs,dependencyDirectory:string,dependencyFiles:Record<string,string>,context:()=>TrustedContext};
export type LocalDecision={id:string,nativeJson:string,alternatives:readonly string[]};
export type LocalBinding={subjectDigest:string,beforePolicyDigest:string,afterPolicyDigest:string,beforeGraphDigest:string,afterGraphDigest:string,compilerVersion:string,policySchemaVersion:number,approvalSchemaVersion:number,canonicalization:string,compilerBinaryDigest:string,compilerManifestDigest:string,reviewExportDigest:string};
export type LocalInspection={export:string,binding:LocalBinding,decisions:readonly LocalDecision[]};
export type LocalRecord={schema_version:1,kind:'local_test_approval',release_authority:false,approval_id:string,issuer:string,reviewer:string,role:string,change_id:string,binding:LocalBinding,decision_id:string,decision:'approve'|'reject'|'request_clarification',alternative:'accept_exact_after'|'retain_before'|'request_clarification',native_delta_json:string,rationale:string,issued_at:number,expires_at:number|null,supersedes:string|null};
const bindingKeys=['subjectDigest','beforePolicyDigest','afterPolicyDigest','beforeGraphDigest','afterGraphDigest','compilerVersion','policySchemaVersion','approvalSchemaVersion','canonicalization','compilerBinaryDigest','compilerManifestDigest','reviewExportDigest'];
const recordKeys=['schema_version','kind','release_authority','approval_id','issuer','reviewer','role','change_id','binding','decision_id','decision','alternative','native_delta_json','rationale','issued_at','expires_at','supersedes'];
export const localPayloadBytes=(record:LocalRecord)=>new TextEncoder().encode(canonicalLocal(record));

export async function createLocalApprovalVerifier(options:LocalVerifierOptions){
 // Copy configuration: later caller mutation cannot change the selected native pins.
 const native=structuredClone(options.native),dependencyDirectory=options.dependencyDirectory,dependencyFiles=structuredClone(options.dependencyFiles),context=options.context;
 for(const value of [native.compilerSha256,native.beforeSourceDigest,native.beforeStateDigest,native.afterSourceDigest,native.afterStateDigest])digest(value);
 if(Object.keys(dependencyFiles).length===0)fail('missing_dependency_pin');for(const pin of Object.values(dependencyFiles))digest(pin);
 const dependencyGuard=()=>{const compilerWorkspace=strictPath(native.compilerWorkspace,'compiler_symlink'),selectedDependency=strictPath(dependencyDirectory,'dependency_symlink');const runtime=join(compilerWorkspace,'crates/core/src/runtime/jwt');for(const name of ['package.json','bun.lock'])if(!readFileSync(strictPath(join(runtime,name),'compiler_symlink')).equals(readFileSync(strictPath(join(selectedDependency,name),'dependency_symlink'))))fail('dependency_declaration_mismatch');
  const joseRoot=strictPath(join(selectedDependency,'node_modules/jose'),'dependency_symlink');
  const packageInfo=parseStrict(text(readFileSync(strictPath(join(joseRoot,'package.json'),'dependency_symlink')))).value;
  if(packageInfo.name!=='jose'||packageInfo.version!=='6.2.12'||Object.keys(packageInfo.dependencies??{}).length||Object.keys(packageInfo.optionalDependencies??{}).length||Object.keys(packageInfo.peerDependencies??{}).length)fail('dependency_package_mismatch');
  if(!sameFiles(files(joseRoot),dependencyFiles))fail('dependency_bytes_mismatch');return selectedDependency;};
 dependencyGuard();
 const selectedDependency=dependencyGuard();
 const jose=await import(pathToFileURL(strictPath(join(selectedDependency,'node_modules/jose/dist/webapi/index.js'),'dependency_symlink')).href);
 dependencyGuard();
 const consumed=new Set<string>();
 const projectFiles=(directory:string)=>{directory=strictPath(directory,'project_symlink');if(!lstatSync(directory).isDirectory())fail('directory_projects_required');const result:Record<string,string>={};
  const walk=(dir:string)=>{for(const entry of readdirSync(dir,{withFileTypes:true})){if(entry.isDirectory()&&entry.name==='build')continue;const path=join(dir,entry.name);if(entry.isSymbolicLink())fail('project_symlink');if(entry.isDirectory())walk(path);else if(entry.isFile()&&entry.name.endsWith('.jadpo'))result[relative(directory,path).replaceAll('\\','/')]=hash(readFileSync(path));}};walk(directory);return result;};
 const nativePaths=()=>({compilerPath:strictPath(native.compilerPath,'compiler_symlink'),compilerWorkspace:strictPath(native.compilerWorkspace,'compiler_symlink'),beforePath:strictPath(native.beforePath,'project_symlink'),afterPath:strictPath(native.afterPath,'project_symlink')});
 const inspect=():LocalInspection=>{const selected=nativePaths();if(hash(readFileSync(selected.compilerPath))!==native.compilerSha256)fail('compiler_bytes_mismatch');
  const sourceBefore=[projectFiles(selected.beforePath),projectFiles(selected.afterPath)];
  const process=Bun.spawnSync([selected.compilerPath,'approval',selected.afterPath,'--before',selected.beforePath,'--expected-before',native.beforeSourceDigest,'--expected-before-state',native.beforeStateDigest],{cwd:resolve(selected.compilerWorkspace,'..'),env:{},stdout:'pipe',stderr:'pipe'});
  if(process.exitCode!==0)fail('native_rederivation_failed');if(canonicalLocal(nativePaths())!==canonicalLocal(selected))fail('native_path_changed');if(hash(readFileSync(selected.compilerPath))!==native.compilerSha256)fail('compiler_bytes_mismatch');
  let output=text(process.stdout);if(output.endsWith('\n'))output=output.slice(0,-1);
  const node=parseStrict(output),subject=node.value,canonical=subject.canonical;
  if(subject.schema_version!==5||subject.kind!=='approval_subject'||canonical.schema_version!==5||canonical.kind!=='approval_subject'||canonical.status!=='local_inspection_non_releasable'||canonical.release_authority!==false||canonical.policy_schema_version!==1||canonical.canonicalization!=='jadpo.approval-subject.v5'||canonical.before===null)fail('unsupported_native_subject');
  const before=canonical.before,after=canonical.after;
  if(before.source_digest!==native.beforeSourceDigest||before.state_digest!==native.beforeStateDigest||after.source_digest!==native.afterSourceDigest||after.state_digest!==native.afterStateDigest)fail('source_state_mismatch');
  for(const [index,snapshot] of [before,after].entries()){
   const directory=index===0?selected.beforePath:selected.afterPath;if(!sameFiles(sourceBefore[index],snapshot.sources)||!sameFiles(projectFiles(directory),snapshot.sources))fail('source_bytes_changed');
   const registry=snapshot.provenance.schema_registry,registryPath=strictPath(join(directory,'schema.identities.json'),'project_symlink',true);if(registry.status==='absent'){if(existsSync(registryPath))fail('schema_registry_changed');}else if(!existsSync(registryPath)||hash(readFileSync(registryPath))!==registry.byte_digest)fail('schema_registry_changed');const compiler=snapshot.provenance.compiler;if(compiler.status!=='build_time_source_manifest'||!plain(compiler.files)||Object.keys(compiler.files).length===0)fail('missing_compiler_manifest');
   for(const [name,pin] of Object.entries(compiler.files)){if(name.startsWith('/')||name.split('/').includes('..')||hash(readFileSync(strictPath(join(selected.compilerWorkspace,name),'compiler_symlink')))!==pin)fail('compiler_source_mismatch');}}
  if(before.provenance.compiler.manifest_digest!==after.provenance.compiler.manifest_digest)fail('compiler_manifest_mismatch');
  const decisionsNode=child(child(node,'canonical'),'decisions');if(!Array.isArray(decisionsNode.children)||decisionsNode.children.length===0)fail('no_native_decisions');const ids=new Set<string>();
  const decisions=decisionsNode.children.map(entry=>{const d=entry.value;digest(d.id);if(ids.has(d.id))fail('duplicate_native_decision');ids.add(d.id);if(d.approval!==null||!Array.isArray(d.alternatives)||canonicalLocal(d.alternatives)!==canonicalLocal(['retain_before','accept_exact_after','request_clarification']))fail('unsupported_native_alternative');return {id:d.id,nativeJson:output.slice(entry.start,entry.end),alternatives:Object.freeze([...d.alternatives])};});
  const binding:LocalBinding={subjectDigest:digest(subject.subject_digest),beforePolicyDigest:digest(before.policy_digest),afterPolicyDigest:digest(after.policy_digest),beforeGraphDigest:digest(before.semantic_graph_digest),afterGraphDigest:digest(after.semantic_graph_digest),compilerVersion:nonempty(canonical.compiler_version),policySchemaVersion:integer(canonical.policy_schema_version),approvalSchemaVersion:5,canonicalization:canonical.canonicalization,compilerBinaryDigest:native.compilerSha256,compilerManifestDigest:digest(after.provenance.compiler.manifest_digest),reviewExportDigest:hash(output)};
  dependencyGuard();return Object.freeze({export:output,binding:Object.freeze(binding),decisions:Object.freeze(decisions.map(Object.freeze))});};
 const snapshot=()=>{const raw=context();if(!raw||raw.oneTime!==true)fail('untrusted_replay_policy');nonempty(raw.changeId);integer(raw.now);if(!Array.isArray(raw.implementers)||!raw.implementers.length)fail('missing_implementer');
  const value={changeId:raw.changeId,implementers:[...raw.implementers],now:raw.now,oneTime:true,revokedApprovalIds:new Set(raw.revokedApprovalIds),supersededApprovalIds:new Set(raw.supersededApprovalIds),issuers:raw.issuers.map(issuer=>({identity:nonempty(issuer.identity),keyId:nonempty(issuer.keyId),publicKey:issuer.publicKey,reviewers:issuer.reviewers.map(r=>({identity:nonempty(r.identity),roles:[...r.roles],decisionIds:[...r.decisionIds]}))}))};
  if(new Set(value.issuers.map(i=>i.keyId)).size!==value.issuers.length||new Set(value.issuers.map(i=>i.identity)).size!==value.issuers.length)fail('duplicate_trusted_issuer');
  for(const issuer of value.issuers){if(!(issuer.publicKey instanceof CryptoKey)||issuer.publicKey.type!=='public'||issuer.publicKey.algorithm.name!=='Ed25519'||!issuer.publicKey.usages.includes('verify'))fail('invalid_trusted_public_key');if(new Set(issuer.reviewers.map(r=>r.identity)).size!==issuer.reviewers.length)fail('duplicate_trusted_reviewer');}
  return value;};
 const contextSame=(a:ReturnType<typeof snapshot>,b:ReturnType<typeof snapshot>)=>canonicalLocal({...a,issuers:a.issuers.map(i=>({...i,publicKey:null})),revokedApprovalIds:[...a.revokedApprovalIds].sort(),supersededApprovalIds:[...a.supersededApprovalIds].sort()})===canonicalLocal({...b,issuers:b.issuers.map(i=>({...i,publicKey:null})),revokedApprovalIds:[...b.revokedApprovalIds].sort(),supersededApprovalIds:[...b.supersededApprovalIds].sort()})&&a.issuers.every((i,n)=>i.publicKey===b.issuers[n].publicKey);
 return Object.freeze({inspect,
  async verify(reviewExport:string,envelopeJson:string){
   const current=inspect();if(reviewExport!==current.export)fail('review_surface_nonconformance');const ctx=snapshot();
   const envelope=parseStrict(envelopeJson).value;keys(envelope,['schema_version','kind','records']);if(envelope.schema_version!==1||envelope.kind!=='local_test_approval_set'||!Array.isArray(envelope.records))fail('closed_shape');
   if(!ctx.issuers.length&&envelope.records.length===0)return Object.freeze({status:'local_inspection_only_non_releasable',release_authority:false,consumed:0,replay_scope:'test_verifier_instance_memory_only'});
   const records:LocalRecord[]=[],ids=new Set<string>(),decisionIds=new Set<string>();
   for(const compact of envelope.records){if(typeof compact!=='string'||compact.length>4*1024*1024||compact.split('.').length!==3)fail('malformed_jws');const parts=compact.split('.');
    const decode=(part:string)=>{if(!/^[A-Za-z0-9_-]+$/.test(part))fail('malformed_jws');const bytes=Buffer.from(part,'base64url');if(bytes.toString('base64url')!==part)fail('malformed_jws');return bytes;};
    const header=parseStrict(text(decode(parts[0]))).value;keys(header,['alg','kid','typ']);if(header.alg!=='EdDSA'||header.typ!=='jadpo-local-approval+jws')fail('algorithm_mismatch');
    const issuer=ctx.issuers.find(i=>i.keyId===header.kid);if(!issuer)fail('untrusted_issuer');decode(parts[1]);decode(parts[2]);
    let verified;try{verified=await jose.compactVerify(compact,issuer.publicKey,{algorithms:['EdDSA']});}catch{fail('invalid_signature');}
    const payload=text(verified.payload),record=parseStrict(payload).value as LocalRecord;keys(record,recordKeys);if(canonicalLocal(record)!==payload)fail('noncanonical_local_payload');
    if(record.schema_version!==1||record.kind!=='local_test_approval'||record.release_authority!==false)fail('invalid_local_scope');keys(record.binding,bindingKeys);
    if(record.issuer!==issuer.identity)fail('untrusted_issuer');if(record.change_id!==ctx.changeId)fail('change_mismatch');
    const approval=nonempty(record.approval_id);if(ids.has(approval))fail('duplicate_approval');ids.add(approval);if(consumed.has(approval))fail('consumed_approval');
    if(ctx.revokedApprovalIds.has(approval)||ctx.supersededApprovalIds.has(approval))fail('inactive_approval');
    integer(record.issued_at);if(record.issued_at>ctx.now)fail('inactive_approval');if(record.expires_at!==null&&(integer(record.expires_at)<=ctx.now||record.expires_at<=record.issued_at))fail('inactive_approval');
    if(record.supersedes!==null&&(!ctx.supersededApprovalIds.has(nonempty(record.supersedes))||record.supersedes===approval))fail('invalid_supersession');
    if(ctx.implementers.includes(record.reviewer))fail('separation_of_authority');const reviewer=issuer.reviewers.find(r=>r.identity===record.reviewer);if(!reviewer||record.role!=='policy_reviewer'||!reviewer.roles.includes(record.role)||!reviewer.decisionIds.includes(record.decision_id))fail('authority_mismatch');
    for(const key of bindingKeys){if(record.binding[key as keyof LocalBinding]!==current.binding[key as keyof LocalBinding]){if(key.includes('Policy'))fail('policy_digest_mismatch');if(key.includes('Graph'))fail('graph_digest_mismatch');if(['compilerVersion','policySchemaVersion','approvalSchemaVersion','canonicalization','compilerBinaryDigest','compilerManifestDigest'].includes(key))fail('version_mismatch');fail('subject_digest_mismatch');}}
    const decision=current.decisions.find(d=>d.id===record.decision_id);if(!decision)fail('foreign_decision');if(decisionIds.has(decision.id))fail('duplicate_decision');decisionIds.add(decision.id);
    if(record.native_delta_json!==decision.nativeJson)fail('exact_delta_mismatch');if(record.decision!=='approve'||record.alternative!=='accept_exact_after'||!decision.alternatives.includes(record.alternative))fail('unapproved_decision');nonempty(record.rationale);records.push(record);
   }
   if(decisionIds.size!==current.decisions.length)fail('incomplete_decisions');
   // Recheck trusted time/entitlements and native bytes after every asynchronous signature
   // verification. No await lies between final replay checks and atomic set insertion.
   const finalContext=snapshot();if(!contextSame(ctx,finalContext))fail('trusted_context_changed');const finalNative=inspect();if(finalNative.export!==current.export)fail('native_subject_changed');
   if(records.some(r=>consumed.has(r.approval_id)))fail('consumed_approval');for(const record of records)consumed.add(record.approval_id);
   return Object.freeze({status:'local_test_non_releasable',release_authority:false,consumed:records.length,replay_scope:'test_verifier_instance_memory_only'});
  },get consumedCount(){return consumed.size;}
 });
}
