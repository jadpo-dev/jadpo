// Tests-only forwarding observation; native values/errors are returned unchanged.
import {SQL as NativeSQL} from 'bun';
let active:any[]|null=null;
export const nativeForm=(v:any):any=>v instanceof Date?{native:'Date',iso:Number.isNaN(v.getTime())?'invalid':v.toISOString()}:typeof v==='bigint'?{native:'bigint',decimal:v.toString()}:Array.isArray(v)?v.map(nativeForm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,nativeForm(x)])):v;
export function beginObservation(){active=[];return active;}
export function endObservation(){active=null;}
function connection(target:any):any{return new Proxy(target,{get(value,key){if(key==='unsafe')return async(sql:string,parameters:any[]=[])=>{const journal=active;const entry:any={sql,parameters:nativeForm(parameters)};if(journal)journal.push(entry);try{const rows=await value.unsafe(sql,parameters);if(journal)entry.rows=nativeForm(rows);return rows;}catch(error){if(journal)entry.error={name:(error as any).name,code:(error as any).code,sqlState:(error as any).errno};throw error;}};const field=Reflect.get(value,key,value);return typeof field==='function'?field.bind(value):field;}});}
export const SQL:typeof NativeSQL=new Proxy(NativeSQL,{construct(target,args){return connection(Reflect.construct(target,args));}});
