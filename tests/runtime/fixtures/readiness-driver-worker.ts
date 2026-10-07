// Tests-only owned Bun SQL process. Only real public SELECT 1 can issue health IPC.
import {SQL} from 'bun';
const owner=Bun.env.JADPO_DRIVER_OWNER!,source=Bun.env.JADPO_DRIVER_SOURCE!;
const held=Bun.env.JADPO_DRIVER_HOLD_IPC==='1',omitted=Bun.env.JADPO_DRIVER_OMIT_IPC==='1',wrong=Bun.env.JADPO_DRIVER_WRONG_OWNER==='1';
const pool=new SQL({url:Bun.env.JADPO_DRIVER_DATABASE_URL!,max:1,prepare:false,connectionTimeout:2,connection:{application_name:Bun.env.JADPO_DRIVER_LABEL!}});
let publicResult:unknown,release:()=>void=()=>{};
process.on('message',(message:any)=>{if(message?.version===1&&message.phase==='release'&&message.owner===owner&&Object.keys(message).sort().join(',')==='owner,phase,version')release();});
process.on('disconnect',()=>{void pool.close({timeout:0});});
function send(phase:string,body:any){process.send?.({version:1,phase,owner,source,...body});}
function diagnostic(phase:string,body:any){console.log(JSON.stringify({version:1,phase,childNow:performance.now(),...body}));}
try{
 diagnostic('acquiring',{});const reserved=await pool.reserve();diagnostic('acquired',{});
 const identities=await reserved.unsafe("SELECT pg_catalog.pg_backend_pid()::integer AS pid,backend_start::text AS started,application_name AS nonce,current_user AS role,r.rolsuper AS superuser,pg_catalog.pg_has_role(current_user,'pg_signal_backend','MEMBER') AS signal_member FROM pg_catalog.pg_stat_activity a JOIN pg_catalog.pg_roles r ON r.rolname=current_user WHERE a.pid=pg_catalog.pg_backend_pid()");
 if(identities.length!==1||identities[0].superuser!==false||identities[0].signal_member!==false)throw Error('worker.identity');send('identity',{identity:{...identities[0]}});
 const rows=await reserved.unsafe('SELECT 1').simple().execute();publicResult=rows.map((row:any)=>({...row}));
 if(rows.length!==1||Object.keys(rows[0]).length!==1||Object.values(rows[0])[0]!==1)throw Error('worker.public_health');
 diagnostic('public_health_fulfilled',{publicResult});
 if(held){diagnostic('owned_ipc_receipt_held',{});await new Promise<void>(resolve=>release=resolve);diagnostic('owned_ipc_receipt_released',{});}
 if(!omitted)process.send?.({version:1,phase:'healthy',owner:wrong?'wrong-owner':owner,source,value:Object.values(rows[0])[0]});
 else diagnostic('owned_health_receipt_omitted',{});
}catch(error:any){diagnostic('public_driver_failure',{error:{name:error.name,code:/^[A-Z0-9_]+$/u.test(error.code??'')?error.code:null}});}
