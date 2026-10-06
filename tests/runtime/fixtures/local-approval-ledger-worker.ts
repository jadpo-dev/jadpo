/** Disposable native storage protocol probe, below issuer/signature validation. */
import {Database} from 'bun:sqlite';
import {writeFileSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {createLocalConsumptionLedger} from '../../../tools/local_approval_ledger';
const input=await new Response(Bun.stdin.stream()).json() as any;
const database=new Database(input.database),ledger=createLocalConsumptionLedger(database);
const payload=(value:any)=>new TextEncoder().encode(JSON.stringify(value));
writeFileSync(input.ready,'ready');
const limit=performance.now()+5000;
while(!existsSync(input.gate)){if(performance.now()>limit)throw Error('Native ledger fixture gate timed out');await Bun.sleep(5);}
if(input.mode==='precommit'){
 database.run('BEGIN IMMEDIATE');
 for(const record of input.records){const bytes=payload(record);database.query('INSERT INTO __jadpo_local_approval_consumption (approval_id,issuer,change_id,subject_digest,decision_id,descriptor_digest,descriptor_json,consumed_at) VALUES (?,?,?,?,?,?,?,?)').run(record.approval_id,record.issuer,record.change_id,record.binding.subjectDigest,record.decision_id,'sha256:'+createHash('sha256').update(bytes).digest('hex'),Buffer.from(bytes).toString('utf8'),1);}
 writeFileSync(input.cut,'native_rows_inserted_before_commit');
 // The parent actually SIGKILLs this native owning process. No product callback
 // or claimed signature verification is substituted for the database cut.
 await Bun.sleep(5000);throw Error('Expected owning-process termination');
}
try{
 ledger.consume(input.records,1,payload);
 if(input.mode==='commit_ack_lost'){writeFileSync(input.cut,'actual_ledger_consume_committed');process.kill(process.pid,'SIGKILL');await Bun.sleep(5000);throw Error('Expected committed process termination');}
 console.log(JSON.stringify({status:'consumed',count:ledger.consumedCount,release_authority:false}));
}catch(error){console.log(JSON.stringify({status:'refused',code:(error as any).code??'fixture_error',release_authority:false}));}
database.close();
