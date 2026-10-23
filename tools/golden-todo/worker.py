"""Local workerd transport for the shared contract, using generated Wasm."""
from __future__ import annotations
import hashlib
import http.client
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time
from postgres import connection_parameters
from contract import MILLIS, iso

def port():
    with socket.socket() as reserve:
        reserve.bind(('127.0.0.1',0));return reserve.getsockname()[1]

def postgres_address(url):
    parameters=connection_parameters(url)
    host=parameters.get('hostaddr') or parameters['host']
    number=parameters.get('port') or '5432'
    if host.startswith('/'):
        # workerd's pinned ExternalServer accepts KJ Unix addresses; the
        # Hyperdrive binding still presents TCP-shaped settings to Wasm.
        return 'unix:'+str(Path(host)/('.s.PGSQL.'+number))
    return ('['+host+']' if ':' in host else host)+':'+number

def build(source, compiler, native, project, here, work, cache):
    overlay=work/'worker-project'
    if overlay.exists():
        for child in overlay.iterdir():
            if child.name not in ['build','.jadpo']:
                if child.is_dir():shutil.rmtree(child)
                else:child.unlink()
    shutil.copytree(project,overlay,dirs_exist_ok=True,ignore=shutil.ignore_patterns('build','.jadpo'))
    app=overlay/'app.jadpo';text=app.read_text().replace('    connections: {','    connections: {\n        limits: { driver: Driver.cloudflare_rate_limits namespace: 1101 }',1)
    text=text.replace('    revocation_delay: 5m','    revocation_delay: 5m\n    rate_limits: { store: limits }\n    deployables: { app: { serves: [main] runs: [Trigger.subscribers, Trigger.schedules] target: Target.wasm } }');app.write_text(text)
    folder=work/'workers';folder.mkdir(exist_ok=True)
    manifest='[package]\nname="golden-workers-contract"\nversion="0.0.0"\nedition="2024"\nbuild="build.rs"\n[workspace]\n[lib]\npath="lib.rs"\ncrate-type=["cdylib"]\n[features]\ntesting=[]\n[dependencies]\n'
    for name in ['jadpo-runtime','jadpo-host-workers','jadpo-drivers','jadpo-value','jadpo-std']:
        features=',features=["testing"]' if name in ['jadpo-runtime','jadpo-host-workers'] else ''
        manifest+=f'{name}={{path={json.dumps(str(source/"crates"/name))}{features}}}\n'
    manifest+='serde_json="1"\nwasm-bindgen-futures="=0.4.79"\njs-sys="=0.3.106"\n[build-dependencies]\n'
    for name in ['jadpo-db','jadpo-check','jadpo-interp','jadpo-codegen']:
        manifest+=f'{name}={{path={json.dumps(str(source/"crates"/name))}}}\n'
    manifest+='[profile.dev]\ndebug=0\nincremental=false\n';(folder/'Cargo.toml').write_text(manifest)
    if not (folder/'Cargo.lock').exists():shutil.copy(source/'Cargo.lock',folder/'Cargo.lock')
    for name in ['lib.rs','build.rs']:shutil.copy(here/'workers'/name,folder/name)
    with (folder/'build.log').open('w') as log:
        subprocess.run(['cargo','build','--offline','--target','wasm32-unknown-unknown','--target-dir',str(cache/'workers'),'--manifest-path',str(folder/'Cargo.toml')],env={**os.environ,'GOLDEN_PROJECT':str(overlay),'CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0'},stdout=log,stderr=subprocess.STDOUT,check=True)
    subprocess.run([str(native.parent/'golden-bind'),str(cache/'workers/wasm32-unknown-unknown/debug/golden_workers_contract.wasm'),str(folder/'bound')],check=True)
    with (folder/'deployable-build.log').open('w') as log:
        subprocess.run([str(compiler),'build','--json'],cwd=overlay,env={**os.environ,'CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0'},stdout=log,stderr=subprocess.STDOUT,check=True)
    return overlay

SHIM = r"""
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
"""

class Fixture:
    def __init__(self,helper):
        self.helper=helper;self.now=MILLIS;self.calls=[];self.receipts={};self.answers=[];self.mutate=None;self.issuer_outage=False;self.lock=threading.Lock()
        self.jwks=helper.call({'op':'jwks'})['result']['jwks']
        fixture=self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_GET(self):self.handle_request()
            def do_POST(self):self.handle_request()
            def handle_request(self):
                body=self.rfile.read(int(self.headers.get('content-length','0')))
                try:
                    if self.path=='/mutation':
                        fixture.helper.call({'op':'sql','sql':body.decode()});return self.respond(200,{})
                    if self.path.startswith('/identity.example/'):
                        if fixture.issuer_outage:return self.respond(503,{})
                        return self.respond(200,{'issuer':'https://identity.example/tenant/','jwks_uri':'https://keys.example/published'})
                    if self.path=='/keys.example/published':
                        return self.respond(503 if fixture.issuer_outage else 200,fixture.jwks)
                    if self.path!='/mail.example.invalid/v1/reminders':return self.respond(404,{})
                    key=self.headers.get('Idempotency-Key');payload=json.loads(body)
                    if fixture.mutate:
                        statement=fixture.mutate;fixture.mutate=None;fixture.helper.call({'op':'sql','sql':statement})
                    answer,receipt=fixture.accept(key,payload)
                    if answer=='conflict':return self.respond(422,{'error':{'code':'idempotency_key_reused'}})
                    if answer=='missing':return self.respond(422,{'error':'missing runtime key'})
                    if answer=='lost':self.close_connection=True;self.connection.shutdown(socket.SHUT_RDWR);return
                    if answer=='unavailable':return self.respond(503,{'error':{'code':'unavailable'}})
                    return self.respond(401 if answer=='unauthorized' else 503 if answer=='server_error' else 200,'private-provider-secret' if answer=='unauthorized' else receipt)
                except Exception as error:self.respond(500,{'fixtureError':str(error)})
            def respond(self,status,body):
                encoded=json.dumps(body).encode();self.send_response(status);self.send_header('content-type','application/json');self.send_header('content-length',str(len(encoded)));self.end_headers();self.wfile.write(encoded)
        self.server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);self.thread=threading.Thread(target=self.server.serve_forever,daemon=True);self.thread.start()
    def accept(self,key,payload):
        # Lookup, payload comparison and acceptance form one provider operation.
        with self.lock:
            self.calls.append({'key':key,'body':payload,'url':'https://mail.example.invalid/v1/reminders'})
            if not key:return 'missing',None
            fingerprint=hashlib.sha256(json.dumps(payload,sort_keys=True,separators=(',',':')).encode()).hexdigest()
            stored=self.receipts.get(key)
            if stored and stored[0]!=fingerprint:return 'conflict',None
            receipt=stored[1] if stored else {'message_id':f'mail_{len(self.receipts)+1}','accepted_at':iso(self.now)}
            answer=self.answers.pop(0) if self.answers else 'accepted'
            if answer in ['accepted','lost']:self.receipts[key]=(fingerprint,receipt)
            return answer,receipt
    def mail(self,command):
        with self.lock:
            if 'answers' in command:self.answers=list(command['answers'])
            self.mutate=command.get('mutate')
            if command.get('clear'):self.calls=[];self.receipts={}
            return {'calls':list(self.calls),'accepted':len(self.receipts),'receipts':[receipt for _,receipt in self.receipts.values()]}

    def stop(self):self.server.shutdown();self.server.server_close();self.thread.join()

def installed_workerd(source):
    """The pinned platform executable; terminating npm's wrapper leaks its child."""
    tools=source/'tools/workers'
    pinned=json.loads((tools/'package.json').read_text())['dependencies']['workerd']
    for package in sorted((tools/'node_modules/@cloudflare').glob('workerd-*')):
        manifest=package/'package.json'
        if not manifest.is_file() or json.loads(manifest.read_text()).get('version')!=pinned:continue
        for name in ('workerd','workerd.exe'):
            binary=package/'bin'/name
            if binary.is_file():return binary
    return None


class Worker:
    def __init__(self,helper,source,project,here,work,url):
        self.helper=helper;self.environment=helper.environment;self.source=source;self.project=project;self.here=here;self.work=work;self.url=url;self.process=None
        executable=os.environ.get('JADPO_WORKERD')
        self.executable=Path(executable) if executable else installed_workerd(source)
        if self.executable is None or not self.executable.exists():
            if executable:raise RuntimeError('JADPO_WORKERD executable does not exist')
            # Preserve the existing missing-install behavior, without invoking
            # npm's process wrapper to serve the Worker.
            with (work/'workers/npm.log').open('w') as log:
                subprocess.run(['npm','ci','--ignore-scripts'],cwd=source/'tools/workers',stdout=log,stderr=subprocess.STDOUT,check=True)
            self.executable=installed_workerd(source)
            if self.executable is None:raise RuntimeError('pinned platform workerd executable was not installed')
        self.fixture=Fixture(helper)
        try:self.start()
        except Exception:
            self.close_process();self.fixture.stop();raise
    def configuration(self,changes=None,production=False):
        folder=self.work/('workerd-production' if production else 'workerd-contract');folder.mkdir(exist_ok=True)
        out=self.work/'worker-project/build/target/app' if production else self.work/'workers/bound'
        modules=[]
        if not production:(folder/'control.mjs').write_text(SHIM);modules.append('(name="control.mjs", esModule=embed "control.mjs")')
        for path in sorted(out.iterdir()):
            kind={'.js':'esModule','.mjs':'esModule','.wasm':'wasm'}.get(path.suffix)
            if kind:shutil.copy(path,folder/path.name);modules.append(f'(name={json.dumps(path.name)}, {kind}=embed {json.dumps(path.name)})')
        # The first module is the entry module in workerd.
        if production:modules.sort(key=lambda m:0 if 'worker.mjs' in m else 1)
        bindings=[];environment=dict(self.environment)
        for name,value in (changes or {}).items():
            if value is None:environment.pop(name,None)
            else:environment[name]=value
        parameters=connection_parameters(self.url)
        address=postgres_address(self.url)
        bindings.append(f'(name="DATABASE_URL", hyperdrive=(designator="hyperdrive", database={json.dumps(parameters["dbname"])},user={json.dumps(parameters.get("user","jadpo"))},password={json.dumps(parameters.get("password",""))},scheme="postgresql"))')
        for name in ['SESSION_SIGNING_KEY','BROWSER_ORIGIN','OIDC_ISSUER','OIDC_AUDIENCE','MAIL_API_KEY','MAIL_ENVIRONMENT','SOFT_DELETE_RETENTION']:
            if name in environment:bindings.append(f'(name={json.dumps(name)},text={json.dumps(environment[name])})')
        # A malformed URL deliberately replaces Hyperdrive for startup validation.
        if changes and 'DATABASE_URL' in changes:bindings[0]=f'(name="DATABASE_URL",text={json.dumps(changes["DATABASE_URL"])})'
        bindings.append('(name="FIXTURE",service="fixture")')
        wrangler=json.loads((self.work/'worker-project/build/target/app/wrangler.json').read_text())
        shutil.copy(self.source/'tools/workers/ratelimit-binding.mjs',folder/'ratelimit-binding.mjs')
        for limit in wrangler['ratelimits']:
            bindings.append(f'(name={json.dumps(limit["name"])},wrapped=(moduleName="golden:ratelimit",innerBindings=[(name="limit",json="{limit["simple"]["limit"]}"),(name="period",json="{limit["simple"]["period"]}"),(name="log",json="false")]))')
        self.port=port()
        config=f'''using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
 services=[(name="app",worker = .app),(name="machine",network=(allow=["public","private","local"])),(name="hyperdrive",external=(address={json.dumps(address)},tcp=())),(name="fixture",external=(address="127.0.0.1:{self.fixture.server.server_port}",http=()))],
 sockets=[(name="http",address="127.0.0.1:{self.port}",http=(),service="app")],
 extensions=[(modules=[(name="golden:ratelimit",internal=true,esModule=embed "ratelimit-binding.mjs")])]);
const app :Workerd.Worker=(modules=[{','.join(modules)}],compatibilityDate="2026-10-01",bindings=[{','.join(bindings)}],globalOutbound="machine");
'''
        (folder/'config.capnp').write_text(config);return folder
    def start(self,changes=None,production=False):
        folder=self.configuration(changes,production);self.log=(folder/'workerd.log').open('w+')
        self.process=subprocess.Popen([str(self.executable),'serve','config.capnp'],cwd=folder,stdout=self.log,stderr=self.log)
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if self.process.poll() is not None:
                self.log.seek(0);raise RuntimeError('workerd startup failed: '+self.log.read())
            try:
                connection=socket.create_connection(('127.0.0.1',self.port),.1);connection.close();return
            except OSError:time.sleep(.03)
        raise RuntimeError('workerd listener did not start')
    def close_process(self):
        if self.process:
            self.process.terminate()
            try:self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
            self.process=None;self.log.close()
    def exchange(self,command):
        connection=http.client.HTTPConnection('127.0.0.1',self.port,timeout=90)
        connection.request('POST','/control',json.dumps(command),{'content-type':'application/json'});response=connection.getresponse();body=response.read();connection.close()
        answer=json.loads(body)
        if response.status!=200:raise RuntimeError(f'workerd controller failed: {answer}')
        return answer
    def call(self,command):
        op=command['op']
        if op in ['sql','digest','sign','open','jwt','jwks','policy_probe','api_record']:return self.helper.call(command)
        if op=='mail':return {'result':self.fixture.mail(command),'statements':[]}
        if op=='preflight':return {'result':self.preflight(command['environment']),'statements':[]}
        if op=='clock':self.fixture.now=command['at'];self.helper.call(command)
        if op=='outage':self.fixture.issuer_outage=command.get('issuer',False)
        if op=='worker_reset':
            self.close_process();self.start();self.exchange({'op':'clock','at':self.fixture.now})
        return self.exchange(command)
    def preflight(self,changes):
        self.close_process()
        def probe(changes):
            self.start(changes,True)
            connection=http.client.HTTPConnection('127.0.0.1',self.port,timeout=15);connection.request('GET','/health/live');response=connection.getresponse();body=response.read().decode();headers=list(response.getheaders());connection.close()
            self.close_process();return response.status,body,headers
        try:
            status,body,_=probe({})
            if status!=200 or json.loads(body)!={'status':'live'}:raise AssertionError('positive actual Worker startup control failed')
            status,body,headers=probe(changes)
            output=(self.work/'workerd-production/workerd.log').read_text();events=[]
            for line in output.splitlines():
                try:events.append(json.loads(line))
                except ValueError:pass
            problems=[problem for event in events for problem in event.get('problems',[])]
            response=json.loads(body)
            private=[value for name,value in self.environment.items() if name in ['SESSION_SIGNING_KEY','MAIL_API_KEY']]
            private += [value for name,value in changes.items() if value and name in ['DATABASE_URL','MAIL_API_KEY']]
            safe=all(value not in body+output for value in private) and 'password@' not in body+output
            return {'refused':status==503,'handlerRan':False,'status':status,'body':response,'headers':headers,'declarations':[p.get('variable') for p in problems],'diagnosticCodes':[p.get('problem') for p in problems],'safe':safe}
        finally:self.close_process();self.start();self.exchange({'op':'clock','at':self.fixture.now})
    def stop(self):self.close_process();self.fixture.stop();self.helper.stop()
