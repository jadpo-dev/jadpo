#!/usr/bin/env python3
"""Run the shared golden contract; a missing obligation fails visibly."""
from __future__ import annotations
import argparse
import hashlib
import http.client
import socket
import importlib.util
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import threading
import worker
from postgres import postgres_uri

ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from contract import CASES, Contract, validate_registry
PROJECT=ROOT/'examples/golden-todo'
WORK=HERE/'work'
CACHE=ROOT/'target/jadpo-compiled/golden-todo'
class Unsupported(RuntimeError):pass

def checked(arguments,**kwargs):return subprocess.run(arguments,check=True,**kwargs)
def revision(path):return checked(['git','rev-parse','HEAD'],cwd=path,capture_output=True,text=True).stdout.strip()

def fingerprint(source):
    digest=hashlib.sha256()
    files=list(PROJECT.rglob('*.jadpo'))+list(PROJECT.rglob('*.jadpo_record'))+list((PROJECT/'database').rglob('*'))+list((HERE/'native').rglob('*'))+list((HERE/'workers').rglob('*'))+[HERE/'worker.py',HERE/'run.py',HERE/'contract.py',HERE/'test_contract.py',HERE/'postgres.py',HERE/'test_postgres.py']
    files += list(PROJECT.glob('acceptance*.json'))+list((source/'crates').rglob('Cargo.toml'))+list((source/'crates').rglob('*.rs'))+[source/'Cargo.lock',source/'Cargo.toml',source/'rust-toolchain.toml',source/'tools/workers/package.json',source/'tools/workers/package-lock.json',source/'tools/workers/ratelimit-binding.mjs']
    for path in sorted(set(file for file in files if file.is_file() and not {'build','.jadpo','target'}.intersection(file.parts))):
        digest.update(str(path).encode());digest.update(path.read_bytes())
    return digest.hexdigest()

def build_native(source):
    before=fingerprint(source)
    work=WORK/'native';work.mkdir(parents=True,exist_ok=True)
    manifest='[package]\nname="golden-native-contract"\nversion="0.0.0"\nedition="2024"\nbuild="build.rs"\n[workspace]\n[features]\ntesting=[]\n[dependencies]\n'
    for name in ['jadpo-runtime','jadpo-host-native','jadpo-drivers','jadpo-value','jadpo-std','jadpo-codegen']:
        manifest+=f'{name}={{path={json.dumps(str(source/"crates"/name))}'+(',features=["testing"]' if name=='jadpo-runtime' else '')+'}\n'
    manifest+='serde_json="1"\nblake3="1"\n[build-dependencies]\n'
    for name in ['jadpo-db','jadpo-check','jadpo-interp','jadpo-codegen']:
        manifest+=f'{name}={{path={json.dumps(str(source/"crates"/name))}}}\n'
    manifest+='[[bin]]\nname="golden-native-contract"\npath="main.rs"\n[[bin]]\nname="golden-bind"\npath="bind.rs"\n[profile.dev]\ndebug=0\nincremental=false\n'
    (work/'Cargo.toml').write_text(manifest)
    if not (work/'Cargo.lock').exists():shutil.copy(source/'Cargo.lock',work/'Cargo.lock')
    for name in ['main.rs','build.rs','bind.rs']:shutil.copy(HERE/'native'/name,work/name)
    with (work/'build.log').open('w') as log:
        checked(['cargo','build','--offline','--target-dir',str(CACHE/'native'),'--manifest-path',str(work/'Cargo.toml')],env={**os.environ,'GOLDEN_PROJECT':str(PROJECT),'CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0'},stdout=log,stderr=subprocess.STDOUT)
        checked(['cargo','test','--offline','--target-dir',str(CACHE/'native'),'--manifest-path',str(work/'Cargo.toml'),'--bin','golden-native-contract'],env={**os.environ,'GOLDEN_PROJECT':str(PROJECT),'CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0'},stdout=log,stderr=subprocess.STDOUT)
    if fingerprint(source)!=before:raise RuntimeError('build inputs changed during native compilation; rebuild required')
    (work/'source-fingerprint').write_text(before)
    return CACHE/'native/debug/golden-native-contract'

class Native:
    supported={'clock','worker_reset','outage','mail','sql','request','drain','deliver_one','sweep','tick','schedule','jwks','jwt','digest','sign','open','mutation_hook','concurrent_tick'}
    def __init__(self,binary,url,compiler):
        self.binary=binary;self.compiler=compiler;self.lock=threading.Lock()
        self.environment={**os.environ,'DATABASE_URL':url,'SESSION_SIGNING_KEY':secrets.token_urlsafe(32),'BROWSER_ORIGIN':'https://todo.example.test','OIDC_ISSUER':'https://identity.example/tenant/','OIDC_AUDIENCE':'golden-todo','MAIL_API_KEY':secrets.token_urlsafe(32),'JWT_SIGNING_KEY':secrets.token_urlsafe(24)}
        self.log=(WORK/'native/adapter.log').open('w+')
        self.process=subprocess.Popen([str(binary)],env=self.environment,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,bufsize=1)
    def call(self,command):
        with self.lock:return self._call(command)
    def _call(self,command):
        if command['op']=='preflight':return {'result':self.preflight(command['environment']),'statements':[]}
        if command['op']=='api_record':return {'result':{'record':(PROJECT/'api/main.jadpo_record').read_text()},'statements':[]}
        if command['op']=='policy_probe':return {'result':self.policy_probe(),'statements':[]}
        if command['op'] not in self.supported:raise Unsupported(f'native controller obligation not yet implemented: {command["op"]}')
        require_running=self.process.poll() is None
        if not require_running:raise RuntimeError(f'native adapter exited {self.process.returncode}; inspect {self.log.name}')
        self.process.stdin.write(json.dumps(command)+'\n');self.process.stdin.flush()
        line=self.process.stdout.readline()
        if not line:raise RuntimeError(f'native adapter gave no response; inspect {self.log.name}')
        return json.loads(line)
    def policy_probe(self):
        probe=WORK/'policy-probe'
        if probe.exists():shutil.rmtree(probe)
        shutil.copytree(PROJECT,probe,ignore=shutil.ignore_patterns('build','.jadpo'))
        (probe/'domains/todo/actions/reassign_todo.jadpo').write_text('''use todo/entities { Todo, TodoNotFound, User }
action reassign_todo(todo_id: Todo.id, new_owner: User.id) fails TodoNotFound -> Todo {
    var item: Todo = attempt Todo.get(todo_id)
    return Todo.update(item) { set: { owner_id: new_owner } }
}
''')
        completed=subprocess.run([str(self.compiler),'check','--json'],cwd=probe,capture_output=True,text=True)
        diagnostics=json.loads(completed.stdout)['diagnostics'];rules=[d['rule'] for d in diagnostics]
        if 'type.field_update_forbidden' not in rules:raise AssertionError(f'owner mutation probe did not fail for ownership policy: {rules}')
        return {'releaseEquivalentSuccess':False,'disposition':'requires_human_decision','rule':'type.field_update_forbidden'}
    def preflight(self,changes):
        binary=PROJECT/'build/target/app/app'
        if not binary.exists():raise Unsupported('real native deployable is required for startup/preflight controls')
        def boot(changes,positive):
            with socket.socket() as reserve:
                reserve.bind(('127.0.0.1',0));port=reserve.getsockname()[1]
            environment={**self.environment,'HOST':'127.0.0.1','PORT':str(port)}
            for name,value in changes.items():
                if value is None:environment.pop(name,None)
                else:environment[name]=value
            with tempfile.TemporaryFile(mode='w+') as log:
                process=subprocess.Popen([str(binary)],env=environment,stdout=log,stderr=log,text=True)
                listened=False;healthy=False
                try:
                    deadline=time.monotonic()+15
                    while time.monotonic()<deadline:
                        try:
                            connection=http.client.HTTPConnection('127.0.0.1',port,timeout=.2)
                            connection.request('GET','/health/live');response=connection.getresponse();body=response.read();connection.close()
                            listened=True;healthy=response.status==200 and json.loads(body)=={'status':'live'}
                            if healthy:break
                        except (OSError,http.client.HTTPException):pass
                        if process.poll() is not None:break
                        time.sleep(.03)
                    if positive:
                        if not healthy:raise AssertionError('positive control: actual native deployable did not serve liveness')
                    elif process.poll() is None:
                        raise AssertionError('invalid configuration did not terminate actual native startup')
                    code=process.poll()
                finally:
                    if process.poll() is None:
                        process.terminate()
                        try:process.wait(timeout=15)
                        except subprocess.TimeoutExpired:process.kill();process.wait()
                log.seek(0);output=log.read()
            return {'listened':listened,'healthy':healthy,'code':code,'output':output}
        if not getattr(self,'startup_verified',False):
            boot({},True);self.startup_verified=True
        actual=boot(changes,False)
        events=[]
        for line in actual['output'].splitlines():
            try:events.append(json.loads(line))
            except ValueError:pass
        problems=[problem for event in events for problem in event.get('problems',[])]
        declarations=[problem.get('variable') for problem in problems if problem.get('variable')]
        secrets_to_hide=[str(value) for name,value in changes.items() if value and name in ['DATABASE_URL','MAIL_API_KEY']]
        secrets_to_hide += [self.environment['MAIL_API_KEY'],self.environment['SESSION_SIGNING_KEY']]
        return {'refused':actual['code'] not in [None,0] and not actual['listened'],'handlerRan':actual['listened'],'declarations':declarations,'safe':all(value not in actual['output'] for value in secrets_to_hide),'diagnosticCodes':[p.get('problem') for p in problems]}
    def stop(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.log.close()

def postgres_tools():
    spec=importlib.util.spec_from_file_location('golden_local_postgres',ROOT/'tools/bench/run.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    module.WORK=WORK/'database';module.WORK.mkdir(parents=True,exist_ok=True)
    def tool(name):
        found=shutil.which(name)
        if found:return found
        candidates=[Path('/Applications/Postgres.app/Contents/Versions/16/bin')/name]
        candidates+=sorted(Path('/usr/lib/postgresql').glob('*/bin/'+name),reverse=True)
        for candidate in candidates:
            if candidate.is_file():return str(candidate)
        raise RuntimeError(name+' was not found on PATH')
    module.tool=tool
    return module

class SharedPostgres:
    """Own databases on the gate's server; never stop that shared server."""
    def __init__(self,url,tools):self.base=postgres_uri(url);self.tools=tools;self.owned=[]
    def url(self,database):return postgres_uri(self.base,database)
    def sql(self,database,statement):
        checked([self.tools.tool('psql'),'-q','-v','ON_ERROR_STOP=1','-d',self.url(database),'-c',statement],stdout=subprocess.DEVNULL)
    def create(self,database):self.sql('postgres',f'CREATE DATABASE {database}');self.owned.append(database)
    def stop(self):
        for database in self.owned:self.sql('postgres',f'DROP DATABASE {database} WITH (FORCE)')

def complete_contract(targets,reports,case_filter,skip_build,own_compiler):
    expected={(target,identifier) for target in ['native','workerd'] for identifier in CASES}
    observed=[(report['target'],report['id']) for report in reports]
    return (len(targets)==2 and set(targets)=={'native','workerd'} and not case_filter
            and not skip_build and own_compiler and len(observed)==len(expected)
            and set(observed)==expected and all(report['status']=='passed' for report in reports))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler',type=Path,default=Path(os.environ.get('JADPO_BIN',str(ROOT/'target/debug/jadpo'))))
    parser.add_argument('--runtime-source',type=Path,default=ROOT)
    parser.add_argument('--targets',default='native,workerd')
    parser.add_argument('--case',action='append',default=[])
    parser.add_argument('--skip-build',action='store_true')
    parser.add_argument('--json',action='store_true')
    args=parser.parse_args();args.compiler=args.compiler.resolve();args.runtime_source=args.runtime_source.resolve()
    contract=json.loads((PROJECT/'acceptance-v0.2.json').read_text());validate_registry(contract)
    selected=[case['id'] for case in contract['cases'] if not args.case or case['id'] in args.case]
    unknown=set(args.case)-set(CASES)
    if unknown:parser.error('unknown case: '+', '.join(sorted(unknown)))
    targets=args.targets.split(',')
    if not all(targets) or len(set(targets))!=len(targets):parser.error('targets must be nonempty and unique')
    if set(targets)-{'native','workerd'}:parser.error('targets are native,workerd')
    WORK.mkdir(exist_ok=True)
    reports=[];postgres=None;driver=None
    metadata=json.loads(checked(['cargo','metadata','--no-deps','--format-version','1'],cwd=args.runtime_source,capture_output=True,text=True).stdout)
    expected_compiler=(Path(metadata['target_directory'])/'debug/jadpo').resolve()
    own_compiler=args.runtime_source==ROOT and args.compiler==expected_compiler
    provenance={'compiler':str(args.compiler),'compilerSha256':hashlib.sha256(args.compiler.read_bytes()).hexdigest(),'compilerMatchesCheckout':own_compiler,'compilerSource':str(args.runtime_source),'runtimeRevision':revision(args.runtime_source),'applicationSource':str(PROJECT),'applicationRevision':revision(ROOT),'preview':not own_compiler,'sourceFingerprint':fingerprint(args.runtime_source),'runtimeDirty':bool(checked(['git','status','--porcelain'],cwd=args.runtime_source,capture_output=True,text=True).stdout)}
    try:
        tools=postgres_tools();shared=os.environ.get('JADPO_TEST_DATABASE_URL');postgres=SharedPostgres(shared,tools) if shared else tools.Postgres()
        if args.skip_build:
            stamp=WORK/'native/source-fingerprint'
            if not stamp.exists() or stamp.read_text()!=fingerprint(args.runtime_source):raise RuntimeError('native adapter inputs changed; rerun without --skip-build')
            binary=CACHE/'native/debug/golden-native-contract'
        else:binary=build_native(args.runtime_source)
        if 'workerd' in targets:
            before=fingerprint(args.runtime_source);stamp=WORK/'workers/source-fingerprint'
            if args.skip_build:
                if not stamp.exists() or stamp.read_text()!=before:raise RuntimeError('workerd inputs changed; rerun without --skip-build')
            else:
                worker.build(args.runtime_source,args.compiler,binary,PROJECT,HERE,WORK,CACHE)
                if fingerprint(args.runtime_source)!=before:raise RuntimeError('build inputs changed during Worker compilation; rebuild required')
                stamp.write_text(before)
        if not args.skip_build and any(identifier in selected for identifier in ['CONFIG-001','CONFIG-002']) and 'native' in targets:
            with (WORK/'native/deployable-build.log').open('w') as log:
                checked([str(args.compiler),'build','--json'],cwd=PROJECT,env={**os.environ,'CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0'},stdout=log,stderr=subprocess.STDOUT)
        for target in targets:
            database='golden_contract_'+target+'_'+secrets.token_hex(5)
            if isinstance(postgres,SharedPostgres):postgres.create(database)
            else:postgres.sql('postgres',f'CREATE DATABASE {database}')
            applied=subprocess.run([str(args.compiler),'migrate','apply','--url',postgres.url(database),'--json'],cwd=PROJECT,capture_output=True,text=True)
            if applied.returncode:raise RuntimeError(applied.stdout+applied.stderr)
            driver=Native(binary,postgres.url(database),args.compiler)
            if target=='workerd':driver=worker.Worker(driver,args.runtime_source,PROJECT,HERE,WORK,postgres.url(database))
            for identifier in selected:
                harness=Contract(driver)
                try:
                    harness.reset();CASES[identifier](harness)
                    reports.append({'target':target,'id':identifier,'status':'passed'})
                except Exception as error:
                    reports.append({'target':target,'id':identifier,'status':'failed','message':str(error)})
            driver.stop();driver=None
    except Exception as error:
        reached={(r['target'],r['id']) for r in reports}
        reports.extend({'target':t,'id':i,'status':'failed','message':str(error)} for t in targets for i in selected if (t,i) not in reached)
    finally:
        if driver:driver.stop()
        if postgres:postgres.stop()
    failed=[r for r in reports if r['status']!='passed']
    result={'schemaVersion':1,'contractVersion':contract['contractVersion'],'ok':not failed,'completeContract':complete_contract(targets,reports,args.case,args.skip_build,own_compiler),'sources':provenance,'counts':{'ran':len(reports),'passed':len(reports)-len(failed),'failed':len(failed)},'cases':reports}
    if args.json:print(json.dumps(result,indent=2))
    else:
        for item in failed:
            print(f'error: {item["target"]} {item["id"]}\n  reason: {item["message"]}\n',file=sys.stderr)
        print(f'{"failed" if failed else "passed"}: golden todo · {len(reports)-len(failed)}/{len(reports)} cases')
    return 1 if failed else 0
if __name__=='__main__':raise SystemExit(main())
