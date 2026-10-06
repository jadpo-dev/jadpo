// Isolated generated-copy experiment ONLY. Not a compiler product decoder.
const pointMetadata=new WeakMap<object,Record<string,{value:unknown,type:unknown}>>();
const descriptors=[{field:'instant',value:'jadpo_probe_carrier_instant_1',type:'jadpo_probe_type_instant'},{field:'optional_instant',value:'jadpo_probe_carrier_optional_instant',type:'jadpo_probe_type_optional_instant'}];
function probeCapture(rows:readonly any[]){
 probeStage('native_before_strip',rows);
 for(const row of rows){const originalFields=Object.fromEntries(Object.entries(row).filter(([key])=>['id','scope_id','bucket','instant','optional_instant','jadpo_probe_carrier_instant'].includes(key)));const metadata:Record<string,any>={};for(const d of descriptors){if(Object.hasOwn(row,d.value)&&Object.hasOwn(row,d.type))metadata[d.field]={value:row[d.value],type:row[d.type]};delete row[d.value];delete row[d.type];}delete row.jadpo_probe_timezone;delete row.jadpo_probe_datestyle;delete row.jadpo_probe_backend_pid;
 const variant=probeOptions().variant;if(variant==='omit_metadata')delete metadata.instant;if(variant==='foreign_alias'){metadata.foreign=metadata.instant;delete metadata.instant;}if(variant==='type_tamper'&&metadata.instant)metadata.instant.type='text';if(variant==='null_type_tamper'&&metadata.optional_instant)metadata.optional_instant.type='text';pointMetadata.set(row,metadata);for(const [key,value] of Object.entries(originalFields))if(!Object.is(row[key],value))throw Error('probe changed original native field reference');}
 probeStage('raw_before_policy',rows);
}
export function probeDecodeInstant(row:Record<string,unknown>,field:string,nullable:boolean):string|null {
 probeStage('selected_decode',[row]);
 const fail=():never=>{throw new PersistenceFault('probe.decode.Instant','data' as any,new Error('stored Instant contradicts checked carrier precision/type/range or identity'));};
 const info=pointMetadata.get(row)?.[field];if(!info||info.type!=='timestamp with time zone')return fail();if(info.value===null)return nullable?null:fail();if(typeof info.value!=='string')return fail();
 const match=/^([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(?:\.([0-9]{1,6}))?([+-][0-9]{2}:[0-9]{2})$/u.exec(info.value);if(!match)return fail();if(!probeOptions().lossy&&/[1-9]/u.test((match[7]??'').slice(3)))return fail();
 const wire=info.value.replace(/\.([0-9]{1,6})(?=[+-])/u,(_,digits)=>'.'+digits.padEnd(3,'0').slice(0,3));const millis=Date.parse(wire);if(!Number.isFinite(millis)||millis < -62135596800000||millis>253402300799999)return fail();return new Date(millis).toISOString();
}
