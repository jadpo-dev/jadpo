// Test-only forwarding driver adapter. No result/error/auth substitution.
import {SQL as NativeSQL} from "bun";
import {Database as NativeDatabase} from "bun:sqlite";
let active: {sql: string; operation: string}[] | null = null;
function record(sql: string) {if(active)active.push({sql,operation:/^\s*([a-z]+)/iu.exec(sql)?.[1].toUpperCase()??"UNKNOWN"});}
export function beginPageObservation() {active=[];return active;}
export function endPageObservation() {active=null;}
function connection(target:any):any {
 return new Proxy(target,{apply(value,receiver,args){if(Array.isArray(args[0]))record(args[0].join("?"));return Reflect.apply(value,value,args);},get(value,key){
  if(key==="unsafe")return(sql:string,...args:any[])=>{record(sql);return value.unsafe(sql,...args);};
  const field=Reflect.get(value,key,value);return typeof field==="function"?field.bind(value):field;
 }});
}
export const SQL: typeof NativeSQL = new Proxy(NativeSQL,{construct(target,args){return connection(Reflect.construct(target,args));}});
export const Database: typeof NativeDatabase = new Proxy(NativeDatabase,{construct(target,args){const value=Reflect.construct(target,args);return new Proxy(value,{get(target,key){
 if(key==="query"||key==="prepare")return(sql:string,...args:any[])=>{record(sql);return Reflect.apply(Reflect.get(target,key,target),target,[sql,...args]);};
 const field=Reflect.get(target,key,target);return typeof field==="function"?field.bind(target):field;
 }});}});
