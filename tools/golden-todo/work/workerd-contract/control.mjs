
import module from './golden.wasm';
import {initSync,control} from './golden.js';
initSync({module});
let now=1768478400000;
Date.now=()=>now;
let binding;
const originalFetch=globalThis.fetch;
globalThis.fetch=async function(input,options){
 const request=new Request(input,options),url=new URL(request.url);
 if(['identity.example','keys.example','mail.example.invalid'].includes(url.hostname)){
  return binding.fetch(new Request('http://fixture/'+url.hostname+url.pathname+url.search,request));
 }
 return originalFetch(input,options);
};
globalThis.goldenFixtureMutation=async(sql)=>{
 const response=await binding.fetch('http://fixture/mutation',{method:'POST',body:sql});
 if(!response.ok)throw new Error('fixture mutation failed');
};
export default {async fetch(request,env,ctx){
 binding=env.FIXTURE;
 const command=await request.json();
 if(command.op==='clock'){now=command.at;return Response.json({result:{at:now},statements:[]});}
 try{return new Response(await control(JSON.stringify(command),env,ctx),{headers:{'content-type':'application/json'}});}
 catch(error){return Response.json({controllerError:String(error)},{status:500});}
}};
