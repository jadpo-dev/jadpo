/** Caller-owned durable storage for local-test approval consumption only.
 * The caller supplies an already opened native handle. This store supplies no
 * issuer identity, protected filesystem guarantee or release authority.
 */
import {Database} from 'bun:sqlite';
import {createHash} from 'node:crypto';
import type {LocalRecord} from './local_approval_verifier';

export class LocalLedgerError extends Error {
 constructor(public readonly code:string){super(code);}
}
const fail=(code:string):never=>{throw new LocalLedgerError(code);};
const table='__jadpo_local_approval_consumption';
const metadata='__jadpo_local_approval_consumption_version';
const schemas=[
 `CREATE TABLE "${table}" (approval_id TEXT PRIMARY KEY NOT NULL, issuer TEXT NOT NULL, change_id TEXT NOT NULL, subject_digest TEXT NOT NULL, decision_id TEXT NOT NULL, descriptor_digest TEXT NOT NULL, descriptor_json TEXT NOT NULL, consumed_at INTEGER NOT NULL CHECK(consumed_at >= 0))`,
 `CREATE TABLE "${metadata}" (version INTEGER PRIMARY KEY NOT NULL CHECK(version = 1), scope TEXT NOT NULL CHECK(scope = 'local_test_non_releasable'))`,
];
export type LocalConsumptionLedger={
 readonly replayScope:'caller_owned_sqlite_ledger';
 has(approvalId:string):boolean;
 consume(records:readonly LocalRecord[],now:number,payload:(record:LocalRecord)=>Uint8Array):void;
 readonly consumedCount:number;
};
const ledgers=new WeakSet<object>();
export const isLocalConsumptionLedger=(value:unknown):value is LocalConsumptionLedger=>
 typeof value==='object'&&value!==null&&ledgers.has(value);

export function createLocalConsumptionLedger(database:Database):LocalConsumptionLedger {
 if(!(database instanceof Database))fail('native_ledger_required');
 const storage=()=>{try{
  if(database.inTransaction)fail('ledger_transaction_already_owned');
  const main=database.query('PRAGMA database_list').all().find((row:any)=>row.name==='main') as any;
  const mode=(database.query('PRAGMA main.journal_mode').get() as any)?.journal_mode;
  const sync=(database.query('PRAGMA main.synchronous').get() as any)?.synchronous;
  if(!main?.file||!['delete','truncate','persist','wal'].includes(mode)||!Number.isInteger(sync)||sync<2)fail('durable_ledger_required');
  if((database.query('PRAGMA busy_timeout').get() as any)?.timeout!==0)fail('bounded_ledger_required');
 }catch(error){if(error instanceof LocalLedgerError)throw error;fail('ledger_unavailable');}};
 const temporarySchema=()=>{
  const row=database.query("SELECT count(*) AS n FROM sqlite_temp_master WHERE name COLLATE NOCASE IN (?, ?) OR (type='trigger' AND tbl_name COLLATE NOCASE IN (?, ?))").get(table,metadata,table,metadata) as any;
  if(row.n!==0)fail('ledger_schema_mismatch');
 };
 const schema=()=>{
  temporarySchema();
  const rows=database.query('SELECT name, type, sql FROM main.sqlite_master WHERE name COLLATE NOCASE IN (?, ?) ORDER BY name').all(table,metadata) as any[];
  if(rows.length!==2||schemas.some(sql=>!rows.some(row=>row.type==='table'&&row.sql===sql)))fail('ledger_schema_mismatch');
  if((database.query("SELECT count(*) AS n FROM main.sqlite_master WHERE type='trigger' AND tbl_name COLLATE NOCASE IN (?, ?)").get(table,metadata) as any).n!==0)fail('ledger_schema_mismatch');
  const versions=database.query(`SELECT version, scope FROM main."${metadata}"`).all() as any[];
  if(versions.length!==1||versions[0].version!==1||versions[0].scope!=='local_test_non_releasable')fail('ledger_schema_mismatch');
 };
 const guard=()=>{storage();try{schema();}catch(error){if(error instanceof LocalLedgerError)throw error;fail('ledger_unavailable');}};
 const transaction=(operation:()=>void)=>{
  storage();let begun=false,committing=false;
  try {
   database.run('BEGIN IMMEDIATE');begun=true;operation();committing=true;database.run('COMMIT');begun=false;
  }catch(error){
   if(begun)try{database.run('ROLLBACK');}catch{fail('ledger_outcome_unknown');}
   if(error instanceof LocalLedgerError)throw error;
   fail(committing?'ledger_outcome_unknown':'ledger_unavailable');
  }
 };
 storage();
 temporarySchema();
 const existing=(database.query('SELECT count(*) AS n FROM main.sqlite_master WHERE name COLLATE NOCASE IN (?, ?)').get(table,metadata) as any).n;
 if(existing===0)transaction(()=>{
  for(const sql of schemas)database.run(sql.replace('CREATE TABLE ', 'CREATE TABLE main.'));
  database.query(`INSERT INTO main."${metadata}" (version, scope) VALUES (1, 'local_test_non_releasable')`).run();
  schema();
 });
 guard();
 const ledger:LocalConsumptionLedger=Object.freeze({
  replayScope:'caller_owned_sqlite_ledger' as const,
  has(approvalId:string){guard();return database.query(`SELECT 1 FROM main."${table}" WHERE approval_id=?`).get(approvalId)!==null;},
  consume(records:readonly LocalRecord[],now:number,payload:(record:LocalRecord)=>Uint8Array){
   guard();if(!Number.isSafeInteger(now)||now<0||!records.length)fail('invalid_ledger_descriptor');
   const ids=new Set<string>();for(const record of records){if(typeof record.approval_id!=='string'||!record.approval_id.trim()||ids.has(record.approval_id))fail('invalid_ledger_descriptor');ids.add(record.approval_id);}
   // Prepare immutable descriptor bytes before ownership. No await or callback
   // occurs between the replay check, whole-set insertion and acknowledged commit.
   const descriptors=records.map(record=>({record:structuredClone(record),bytes:Buffer.from(payload(record))}));
   transaction(()=>{
    schema();const present=database.query(`SELECT 1 FROM main."${table}" WHERE approval_id=?`);
    if(descriptors.some(({record})=>present.get(record.approval_id)!==null))fail('consumed_approval');
    const insert=database.query(`INSERT INTO main."${table}" (approval_id,issuer,change_id,subject_digest,decision_id,descriptor_digest,descriptor_json,consumed_at) VALUES (?,?,?,?,?,?,?,?)`);
    for(const {record,bytes} of descriptors)insert.run(record.approval_id,record.issuer,record.change_id,record.binding.subjectDigest,record.decision_id,'sha256:'+createHash('sha256').update(bytes).digest('hex'),bytes.toString('utf8'),now);
   });
  },
  get consumedCount(){guard();return (database.query(`SELECT count(*) AS n FROM main."${table}"`).get() as any).n as number;},
 });
 ledgers.add(ledger);return ledger;
}
