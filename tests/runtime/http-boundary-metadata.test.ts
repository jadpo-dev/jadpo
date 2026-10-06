import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { permits } from "./fixtures/http-boundary-schema.ts";
const root=mkdtempSync(join(tmpdir(),"jadpo-http-boundary-metadata-"));
const compiler=Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo",import.meta.url).pathname;
const source=`
type Positive=Int {min:1}
input Body {n:Positive}
type Params=Object {n:Positive}
output Result {ok:Bool}
failure Bad {kind:InvalidValue code:"invalid_input" message:"Request body value is invalid."}
route GET /plain {auth:none output:Result action:{return Result {ok:true}}}
route GET /value/{id} {auth:none path:{id:Uuid} output:Result action:{return Result {ok:true}}}
route GET /shaped {auth:none query:Params headers:{h:Positive from "X-N"} output:Result action:{return Result {ok:true}}}
route POST /body {auth:none input:Body output:Result action:{return Result {ok:true}}}
route POST /collision {auth:none input:Body output:Result action:fails Bad {reject Bad}}
`;
writeFileSync(join(root,"app.jadpo"),source);
const result=Bun.spawnSync([compiler,"build",root],{stdout:"pipe",stderr:"pipe"});
if(result.exitCode!==0)throw new Error(`HTTP metadata fixture compilation failed: ${result.stdout}\n${result.stderr}`);
const api=JSON.parse(readFileSync(join(root,"build/openapi/openapi.json"),"utf8"));
const app=await import(pathToFileURL(join(root,"build/target/app.ts")).href);
const server=app.createApplicationServer();
await new Promise<void>((resolve,reject)=>{server.once("error",reject);server.listen(0,"127.0.0.1",resolve);});
const origin=`http://127.0.0.1:${server.address().port}`;
afterAll(async()=>{await new Promise<void>((resolve,reject)=>server.close((e:any)=>e?reject(e):resolve()));rmSync(root,{recursive:true,force:true});});
const observations:any[]=[];
async function send(path:string,method="GET",body?:string,headers:Record<string,string>={}){
 const response=await fetch(`${origin}${path}`,{method,headers,body});
 const json=await response.json();observations.push({method,path,status:response.status,body:json});return{response,json};
}
const cases=[
 {name:"query syntax",path:"/shaped?n=1&foreign=1",headers:{"X-N":"1"},status:400,message:"Request query syntax is invalid."},
 {name:"query value",path:"/shaped?n=0",headers:{"X-N":"1"},status:422,message:"Request query value is invalid."},
 {name:"header syntax",path:"/shaped?n=1",headers:{},status:400,message:"Request header syntax is invalid."},
 {name:"header value",path:"/shaped?n=1",headers:{"X-N":"0"},status:422,message:"Request header value is invalid."},
 {name:"path value",path:"/value/not-a-uuid",headers:{},status:422,message:"Request path value is invalid."},
 {name:"JSON syntax",path:"/body",method:"POST",body:"{",headers:{},status:400,message:"Request JSON syntax is invalid."},
 {name:"body value",path:"/body",method:"POST",body:'{"n":0}',headers:{},status:422,message:"Request body value is invalid."},
 {name:"declared collision",path:"/collision",method:"POST",body:'{"n":1}',headers:{},status:422,message:"Request body value is invalid."},
];
for(const item of cases)test(`native ${item.name} envelope agrees with emitted closed schema`,async()=>{
 const {response,json}=await send(item.path,item.method,item.body,item.headers);
 expect(response.status).toBe(item.status);expect(json.error.message).toBe(item.message);
 expect(json.error.request_id).toBe(response.headers.get("X-Request-ID"));
 const path=item.path.startsWith("/value/")?"/value/{id}":item.path.split("?")[0];
 const schema=api.paths[path][(item.method??"GET").toLowerCase()].responses[String(response.status)].content["application/json"].schema;
 expect(permits(schema,json,api.components.schemas)).toBe(true);
 expect(permits(schema,{...json,debug:"private-canary"},api.components.schemas)).toBe(false);
 expect(permits(schema,{error:{...json.error,debug:"private-canary"}},api.components.schemas)).toBe(false);
 expect(permits(schema,{error:{...json.error,message:"private-canary"}},api.components.schemas)).toBe(false);
 expect(permits(schema,{error:{...json.error,request_id:42}},api.components.schemas)).toBe(false);
});
test("public shape-free route ignores unrelated credentials and undeclared query",async()=>{
 const{response,json}=await send("/plain?foreign=canary","GET",undefined,{authorization:"Bearer malformed",cookie:"foreign=private-canary"});
 expect(response.status).toBe(200);expect(json).toEqual({ok:true});
 for(const status of ["400","401","403","422"])expect(api.paths["/plain"].get.responses[status]).toBeUndefined();
});
test("valid public shaped request executes without auth outcomes",async()=>{
 const{response,json}=await send("/shaped?n=1","GET",undefined,{"X-N":"1"});
 expect(response.status).toBe(200);expect(json).toEqual({ok:true});
 expect(api.paths["/shaped"].get.responses["401"]).toBeUndefined();
});
afterAll(()=>{if(Bun.env.JADPO_HTTP_BOUNDARY_RAW)writeFileSync(Bun.env.JADPO_HTTP_BOUNDARY_RAW,JSON.stringify({source,transport:"native createApplicationServer with raw incoming header counts",observations},null,2)+"\n");});
