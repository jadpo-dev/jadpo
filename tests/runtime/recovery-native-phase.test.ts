import {expect,test} from "bun:test";
import {join,resolve} from "node:path";
import {mkdirSync,mkdtempSync,writeFileSync,readFileSync} from "node:fs";
import {createHash} from "node:crypto";
const url=Bun.env.JADPO_READINESS_TEST_DATABASE_URL;
if(url){const u=new URL(url);if(!["postgres:","postgresql:"].includes(u.protocol)||!["127.0.0.1","localhost"].includes(u.hostname)||!/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(u.pathname))throw Error("Disposable local native recovery database required");}
const run=url?test:test.skip;
const app=Bun.env.JADPO_READINESS_APPLICATION??resolve("examples/persistence-seed/build/target/app.ts");
const sha=(b:Uint8Array)=>createHash("sha256").update(b).digest("hex");
for(const phase of ["begin","setup"])for(const mode of ["healthy","hold","keep-hold","omit-hold","preexecute"]){
 run(`actual native recovery ${phase} ${mode} preserves original response and owned settlement`,async()=>{
  const namespace=resolve("build/validation/rm403-recovery-phase-case.");mkdirSync(resolve("build/validation"),{recursive:true});const receipt=mkdtempSync(namespace);
  const pins:any={};for(const name of ["app.ts","persistence.ts","startup-owner.ts"]){const b=readFileSync(join(app,"..",name));pins[name]=sha(b);writeFileSync(join(receipt,name),b);}
  const child=Bun.spawn([process.execPath,"--no-install","--env-file=/dev/null",join(import.meta.dir,"fixtures/recovery-owned-phase.ts"),phase,mode],{env:{PATH:Bun.env.PATH!,JADPO_READINESS_TEST_DATABASE_URL:url!,JADPO_READINESS_APPLICATION:app},stdout:"pipe",stderr:"pipe"});
  let outerWatchdog=false;const watchdog=setTimeout(()=>{outerWatchdog=true;child.kill("SIGKILL");},18000);
  try{
   const [stdout,stderr,exit]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);writeFileSync(join(receipt,"native.stdout"),stdout);writeFileSync(join(receipt,"native.stderr"),stderr);
   const p=JSON.parse(stdout.trim());writeFileSync(join(receipt,"proof.json"),JSON.stringify(p,null,2)+"\n");writeFileSync(join(receipt,"pins.json"),JSON.stringify(pins,null,2)+"\n");console.log(JSON.stringify({kind:"native_recovery_initializer_phase_receipt",phase,mode,receipt,pins,exit,proof:p}));
   expect(outerWatchdog).toBe(false);expect(p.watchdog).toBe(false);expect(p.nativeExit).toBe(143);expect(p.protocolFailure).toBeNull();expect(p.safeNativeOutput).toBe(true);expect(p.cleanupBeforeFixtureTeardown).toBe(true);expect(p.backendRemaining).toBe(0);expect(p.downstreamClosed).toBe(p.connections);expect(p.upstreamClosed).toBe(p.connections);expect(p.connections).toBeGreaterThan(0);expect(p.originalDurable).toEqual(p.finalDurable);
   expect(p.nativeProcesses).toHaveLength(2);expect(p.formerPorts.every((v:any)=>v.refused)).toBe(true);expect(p.nativeInvalidations).toHaveLength(1);expect(p.nativeInvalidations[0]).toMatchObject({queryForwarded:false,terminationAcknowledged:true});
   const invalid=p.observations.find((v:any)=>v.phase==="genuine_native_dependency_failure");expect(invalid.response.status).toBe(503);expect(p.selected.pid).not.toBe(p.nativeInvalidations[0].pid);expect(p.selected.kind).toBe(phase);
   if(mode==="preexecute"){
    expect(exit).toBe(1);expect(p.failure).toBe("probe.selected_native_result_unqualified");expect(p.qualifiedNativeExecution).toBe(false);expect(p.selectedForwarded).toBe(0);expect(p.selectedSuppressed).toBe(1);expect(p.selectedRows).toBe(0);expect(p.selectedCommand).toBe(false);expect(p.selectedInTransaction).toBe(false);
    if(phase==="setup"){expect(p.selected.frontendTag).toBe("E");expect(p.parsedStatements.some((v:any)=>v.kind==="setup"&&v.pid===p.selected.pid)).toBe(true);expect(p.parseAcknowledgements).toBeGreaterThan(0);}
   }else{
    expect(p.qualifiedNativeExecution).toBe(true);expect(p.selectedForwarded).toBe(1);expect(p.selected.nativeCommandAcknowledged).toBe(true);expect(p.selected.readyStatus).toBe("T");expect(p.selectedRows).toBe(phase==="begin"?0:1);
    if(phase==="setup"){expect(p.selected.timeoutMilliseconds).toHaveLength(2);expect(p.selected.timeoutMilliseconds[0]).toBeGreaterThan(0);expect(p.selected.timeoutMilliseconds[0]).toBeLessThanOrEqual(1000);expect(p.selected.timeoutMilliseconds[0]).toBe(p.selected.timeoutMilliseconds[1]);}
    const first=p.observations.find((v:any)=>v.phase==="first_http_joiners");expect(first.responses).toHaveLength(32);expect(first.live.status).toBe(200);
    if(mode==="omit-hold"){expect(exit).toBe(1);expect(p.failure).toBe("probe.held_original_response_503");expect(first.responses.every((v:any)=>v.status===200)).toBe(true);expect(p.selectedResultDelivered).toBe(true);}
    else{expect(exit).toBe(0);expect(p.failure).toBeNull();}
    if(mode==="hold"||mode==="keep-hold"){
     expect(first.responses.every((v:any)=>v.status===503)).toBe(true);expect(first.elapsedMs).toBeGreaterThanOrEqual(900);expect(first.elapsedMs).toBeLessThan(1600);
     const later=p.observations.find((v:any)=>v.phase==="later_http_joiners_after_backoff");expect(later.responses).toHaveLength(128);expect(later.responses.every((v:any)=>v.status===503)).toBe(true);expect(later.executionsAfterSelected).toEqual([]);expect(later.selectedSocketOpen).toBe(true);expect(later.heldResultDelivered).toBe(false);expect(later.live.status).toBe(200);
     if(mode==="hold"){const settled=p.observations.find((v:any)=>v.phase==="original_recovery_native_rollback_ack"),fresh=p.observations.find((v:any)=>v.phase==="fresh_recovery"),begins=fresh.actualNativeRecovery.filter((v:any)=>v.kind==="begin");expect(settled.settled).toMatchObject({kind:"rollback",nativeCommandAcknowledged:true,readyStatus:"I"});expect(begins).toHaveLength(1);expect(settled.settled.readyAt).toBeLessThan(begins[0].at);expect(fresh.readiness.status).toBe(200);}
     else{const manual=p.observations.find((v:any)=>v.phase==="manual_shutdown_with_result_still_held");expect(manual.stillHeld).toBe(true);expect(manual.nativeTransportOpen).toBe(true);expect(manual.live.status).toBe(200);expect(p.selectedResultDelivered).toBe(false);}
    }
   }
  }catch(error){const e=error as any;writeFileSync(join(receipt,"fixture-error.json"),JSON.stringify({name:e?.name??null,code:e?.code??null,message:e?.message??String(error)},null,2)+"\n");console.error(JSON.stringify({kind:"native_recovery_fixture_failure",receipt,name:e?.name??null,code:e?.code??null,message:e?.message??String(error)}));throw error;}finally{clearTimeout(watchdog);if(child.exitCode===null)child.kill("SIGKILL");await child.exited;}
 },20000);
}
