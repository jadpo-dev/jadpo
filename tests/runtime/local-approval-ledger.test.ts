/** Native ledger persistence/atomicity evidence, not an issuer or approval oracle. */
import {test,expect} from 'bun:test';
import {Database} from 'bun:sqlite';
import {mkdtempSync,writeFileSync,existsSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {createLocalConsumptionLedger} from '../../tools/local_approval_ledger';
const records=['first','second'].map(id=>({approval_id:id,issuer:'native-storage-fixture',change_id:'storage-only',decision_id:id,binding:{subjectDigest:'sha256:'+'0'.repeat(64)}})) as any;
const payload=(value:any)=>new TextEncoder().encode(JSON.stringify(value));
async function observed(path:string){const limit=performance.now()+5000;while(!existsSync(path)){if(performance.now()>limit)throw Error('Native ledger fixture observation timed out');await Bun.sleep(5);}}
function start(config:any){const child=Bun.spawn([process.execPath,'--no-install','--env-file=/dev/null',resolve('tests/runtime/fixtures/local-approval-ledger-worker.ts')],{stdin:'pipe',stdout:'pipe',stderr:'pipe',env:{}});child.stdin.write(JSON.stringify({...config,records}));child.stdin.end();return child;}
test('native separate processes consume one complete durable descriptor set',async()=>{
 const root=mkdtempSync(join(tmpdir(),'jadpo-ledger-race-')),path=join(root,'ledger.sqlite'),database=new Database(path);createLocalConsumptionLedger(database);database.close();
 const gate=join(root,'go'),ready=[join(root,'one'),join(root,'two')],children=ready.map(value=>start({database:path,ready:value,gate,mode:'race'}));
 try{
  await Promise.all(ready.map(observed));writeFileSync(gate,'go');
  const result=await Promise.all(children.map(async child=>({exit:await child.exited,out:await new Response(child.stdout).text(),err:await new Response(child.stderr).text()})));
  expect(result.every(value=>value.exit===0&&value.err==='')).toBe(true);
  const values=result.map(value=>JSON.parse(value.out));expect(values.filter(value=>value.status==='consumed').length).toBe(1);
  expect(values.filter(value=>value.status==='refused'&&['consumed_approval','ledger_unavailable'].includes(value.code)).length).toBe(1);
  expect(values.every(value=>value.release_authority===false)).toBe(true);
  const fresh=new Database(path);try{const ledger=createLocalConsumptionLedger(fresh);expect(ledger.consumedCount).toBe(2);expect(()=>ledger.consume(records,1,payload)).toThrow(expect.objectContaining({code:'consumed_approval'}));}finally{fresh.close();}
 }finally{for(const child of children)child.kill();await Promise.all(children.map(child=>child.exited));rmSync(root,{recursive:true,force:true});}
},15000);
test('native owning-process SIGKILL before SQLite COMMIT restores an empty consumable ledger',async()=>{
 const root=mkdtempSync(join(tmpdir(),'jadpo-ledger-precommit-')),path=join(root,'ledger.sqlite'),database=new Database(path);createLocalConsumptionLedger(database);database.close();
 const config={database:path,ready:join(root,'ready'),gate:join(root,'go'),cut:join(root,'cut'),mode:'precommit'},child=start(config);
 try{
  await observed(config.ready);writeFileSync(config.gate,'go');await observed(config.cut);child.kill('SIGKILL');expect(await child.exited).toBe(137);
  const fresh=new Database(path);try{const ledger=createLocalConsumptionLedger(fresh);expect(ledger.consumedCount).toBe(0);ledger.consume(records,1,payload);expect(ledger.consumedCount).toBe(2);}finally{fresh.close();}
 }finally{child.kill();await child.exited;rmSync(root,{recursive:true,force:true});}
},15000);
test('actual committed ledger consumption survives SIGKILL before process acknowledgement and fresh replay refuses',async()=>{
 const root=mkdtempSync(join(tmpdir(),'jadpo-ledger-commit-')),path=join(root,'ledger.sqlite'),database=new Database(path);createLocalConsumptionLedger(database);database.close();
 const config={database:path,ready:join(root,'ready'),gate:join(root,'go'),cut:join(root,'cut'),mode:'commit_ack_lost'},child=start(config);
 try{
  await observed(config.ready);writeFileSync(config.gate,'go');await observed(config.cut);expect(await child.exited).toBe(137);
  const fresh=new Database(path);try{const ledger=createLocalConsumptionLedger(fresh);expect(ledger.consumedCount).toBe(2);expect(()=>ledger.consume(records,1,payload)).toThrow(expect.objectContaining({code:'consumed_approval'}));}finally{fresh.close();}
 }finally{child.kill();await child.exited;rmSync(root,{recursive:true,force:true});}
},15000);
