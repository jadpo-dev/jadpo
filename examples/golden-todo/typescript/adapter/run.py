#!/usr/bin/env python3
"""Private local verification: real PostgreSQL and unchanged versioned shared cases."""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile

HERE=Path(__file__).resolve().parent
PROJECT=HERE.parent
ROOT=PROJECT.parents[2]

def run(args,**kwargs):return subprocess.run(args,check=True,**kwargs)
def tool(name):
    found=shutil.which(name)
    if found:return found
    path=Path('/Applications/Postgres.app/Contents/Versions/16/bin')/name
    if path.is_file():return str(path)
    raise RuntimeError(f'{name} unavailable')
def port():
    with socket.socket() as listener:listener.bind(('127.0.0.1',0));return listener.getsockname()[1]

class Postgres:
    def __init__(self,directory):
        self.directory=directory;self.data=directory/'pg';self.port=port();self.started=False
        run([tool('initdb'),'-D',str(self.data),'-U','golden','--auth=trust','-E','UTF8'],stdout=subprocess.DEVNULL)
        options=f'-p {self.port} -c unix_socket_directories= -c listen_addresses=127.0.0.1 -c max_connections=100 -c shared_buffers=32MB'
        run([tool('pg_ctl'),'-D',str(self.data),'-l',str(directory/'postgres.log'),'-o',options,'-w','start'],stdout=subprocess.DEVNULL);self.started=True
    def url(self,name):return f'postgresql://golden@127.0.0.1:{self.port}/{name}'
    def sql(self,name,text):run([tool('psql'),'-q','-v','ON_ERROR_STOP=1','-d',self.url(name),'-c',text],stdout=subprocess.DEVNULL)
    def prepare(self,name):
        self.sql('postgres',f'CREATE DATABASE {name}')
        for migration in sorted((PROJECT/'drizzle').glob('*.sql')):run([tool('psql'),'-q','-v','ON_ERROR_STOP=1','-d',self.url(name),'-f',str(migration)],stdout=subprocess.DEVNULL)
    def stop(self):
        if self.started:run([tool('pg_ctl'),'-D',str(self.data),'-m','fast','-w','stop'],stdout=subprocess.DEVNULL);self.started=False

class Controller:
    def __init__(self,runtime,environment,directory):
        self.log=(directory/f'controller-{runtime}.log').open('w+')
        self.process=subprocess.Popen([tool(runtime),str(HERE/'controller.ts')],cwd=PROJECT,env=environment,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,bufsize=1)
    def call(self,command):
        self.process.stdin.write(json.dumps(command)+'\n');self.process.stdin.flush();line=self.process.stdout.readline()
        if not line:
            self.log.flush();self.log.seek(0);raise RuntimeError('controller stopped: '+self.log.read()[-6000:])
        answer=json.loads(line)
        if 'error' in answer:raise RuntimeError(answer['error'])
        return answer
    def stop(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.log.close()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--contract-source',type=Path,required=True)
    parser.add_argument('--runtimes',default='node,bun')
    parser.add_argument('--case',action='append',default=[])
    parser.add_argument('--unit',action='store_true')
    args=parser.parse_args();source=args.contract_source.resolve()
    specification=source/'examples/golden-todo/acceptance-v0.2.json';contract=source/'tools/golden-todo/contract.py'
    if not specification.is_file() or not contract.is_file():parser.error('explicit shared contract checkpoint unavailable')
    spec=importlib.util.spec_from_file_location('golden_shared_contract',contract);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    document=json.loads(specification.read_text());module.validate_registry(document)
    runtimes=args.runtimes.split(',')
    if set(runtimes)-{'node','bun'}:parser.error('runtimes are node,bun')
    selected=[case['id'] for case in document['cases'] if not args.case or case['id'] in args.case]
    if set(args.case)-module.CASES.keys():parser.error('unknown case')
    revision=run(['git','rev-parse','HEAD'],cwd=source,capture_output=True,text=True).stdout.strip()
    reports=[];server=None;controller=None;(PROJECT/'build').mkdir(exist_ok=True)
    temporary=tempfile.TemporaryDirectory(prefix='local-',dir=PROJECT/'build')
    try:
        directory=Path(temporary.name);server=Postgres(directory)
        env={**os.environ,'SESSION_SIGNING_KEY':secrets.token_urlsafe(32),'BROWSER_ORIGIN':'https://todo.example.test','OIDC_ISSUER':'https://identity.example/tenant/','OIDC_AUDIENCE':'golden-todo','MAIL_API_KEY':secrets.token_urlsafe(32),'CONTROL_DATABASE_URL':server.url('postgres')}
        if args.unit:
            server.prepare('golden_tests');run(['npm','run','test:all'],cwd=PROJECT,env={**env,'TEST_DATABASE_URL':server.url('golden_tests')})
        for runtime in runtimes:
            name='golden_'+runtime;server.prepare(name);controller=Controller(runtime,{**env,'DATABASE_URL':server.url(name)},directory)
            for identifier in selected:
                try:
                    harness=module.Contract(controller);harness.reset();module.CASES[identifier](harness);reports.append({'runtime':runtime,'id':identifier,'status':'passed'})
                except Exception as error:reports.append({'runtime':runtime,'id':identifier,'status':'failed','message':str(error)})
            controller.stop();controller=None
        server.stop();server=None
    finally:
        if controller:controller.stop()
        if server:server.stop()
        temporary.cleanup()
    failed=[report for report in reports if report['status']!='passed']
    print(json.dumps({'contractVersion':document['contractVersion'],'preview':True,'completeContract':False,'sources':{'sharedContractSource':str(source),'sharedContractRevision':revision,'sharedContractSha256':hashlib.sha256(contract.read_bytes()).hexdigest(),'acceptanceSha256':hashlib.sha256(specification.read_bytes()).hexdigest()},'counts':{'ran':len(reports),'passed':len(reports)-len(failed),'failed':len(failed)},'cases':reports},indent=2))
    return bool(failed)
if __name__=='__main__':raise SystemExit(main())
