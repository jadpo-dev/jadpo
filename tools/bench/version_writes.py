#!/usr/bin/env python3
"""Guarded version-write comparison: compiler-generated-plan runtime and warm HTTP PATCH.

The runtime fixture uses interpreter dispatch in release Rust. The separate
HTTP fixture uses genuine generated native/workerd deployables and warm Bun.
All source/mode/HEAD/artifact inputs must match an immutable build label.
"""
from __future__ import annotations
import argparse
import copy
import datetime as dt
import json
import math
import os
from pathlib import Path
import resource
import secrets
import shutil
import subprocess
import sys
import time

import ai_accounting as guard
import golden
from contracts import validate_failure, validate_measurement

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / 'tools/bench/work/version-writes'
FLAG_NAMES = ('CARGO_INCREMENTAL','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','CARGO_ENCODED_RUSTFLAGS','RUSTFLAGS','RUSTUP_TOOLCHAIN')
def selected_flags(environment):
    return {name:environment.get(name) for name in sorted(set(FLAG_NAMES) | {k for k in environment if k.startswith('CARGO_PROFILE_')})}
INHERITED_FLAGS = selected_flags(os.environ)
FLAGS = {'CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0','CARGO_BUILD_JOBS':'4'}
ENV = {**os.environ, **FLAGS}


def command(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def context():
    return {'time':time.time(),'load':os.getloadavg(),
            'processes':command(['ps','-axo','pid=,ppid=,pcpu=,comm='],capture_output=True,text=True).stdout,
            'flags':selected_flags(ENV)}


def build(label):
    before = guard.source()
    directory = WORK / label
    directory.mkdir(parents=True,exist_ok=False)
    stamp = {'source':before,'start':context(),'artifacts':{},'inherited_flags':INHERITED_FLAGS}
    (directory/'build-start.json').write_text(json.dumps(stamp,indent=2)+'\n')
    with (directory/'build.log').open('w') as log:
        command(['cargo','build','--locked','--release','-p','jadpo-test','--bin','jadpo-bench-versions'],env=ENV,stdout=log,stderr=log)
        # Existing four-target 44-case conformance, fresh production builds,
        # and pinned npm install with lifecycle scripts disabled.
        command([sys.executable,str(ROOT/'tools/bench/golden.py'),'--prepare-only'],env=ENV,stdout=log,stderr=log)
    golden_stamp = json.loads(golden.STAMP.read_text())
    runtime_paths={name:str(Path(golden.spike.tool(name)).resolve()) for name in ('bun','node','psql')}
    runtime_paths['workerd']=str(golden.spike.workerd().resolve())
    stamp['runtimes']={name:{'path':path,'version':command([path,'--version'],capture_output=True,text=True).stdout.strip()} for name,path in runtime_paths.items()}
    paths = [ROOT/'target/release/jadpo-bench-versions',*map(Path,golden_stamp['artifacts']),*map(Path,runtime_paths.values())]
    artifacts = directory/'artifacts';artifacts.mkdir()
    for index,path in enumerate(dict.fromkeys(paths)):
        saved = artifacts/f'{index:03d}-{path.name}'
        shutil.copy2(path,saved)
        stamp['artifacts'][str(path)] = guard.sha(path)
        stamp['artifacts'][str(saved)] = guard.sha(saved)
        if path.name == 'jadpo-bench-versions':stamp['fixture']=str(saved)
    if guard.source()!=before:raise ValueError('source changed during build')
    stamp.update(golden=golden_stamp,end=context(),profiles={'runtime':'release Rust, compiler-generated plan, interpreter dispatch',
        'http_native':'jadpo build release','http_workers':'jadpo build release, local workerd, no wasm-opt','bun':'persistent warm Bun, identical TS build'})
    (directory/'build.json').write_text(json.dumps(stamp,indent=2)+'\n')
    guard.validate(stamp,guard.source())


def validate_runtime(result):
    if result.get('protocol')!=1 or result.get('complete') is not True or result.get('warmup')!=50:raise ValueError('incomplete runtime protocol')
    count=result['count'];runs=result['runs']
    if len(runs)!=3 or {r['round'] for r in runs}!={1,2,3}:raise ValueError('missing runtime rounds')
    for row in runs:
        samples=row['latency_ms']
        if len(samples)!=count or any(not math.isfinite(v) or v<=0 for v in samples):raise ValueError('invalid runtime samples')
        if row['p95_ms']!=sorted(samples)[math.ceil(count*.95)-1] or not math.isclose(row['calls_per_second'],count/row['seconds']):raise ValueError('invalid runtime aggregation')
        if row.get('provider_calls')!=count:raise ValueError('provider calls differ from completed work')
        if row.get('count')!=count:raise ValueError('runtime cell count differs from protocol')
        data=sum(not entry['sql'].startswith('BEGIN ') and entry['sql'] not in ('COMMIT','ROLLBACK') for entry in row['census_sql'])
        if row.get('data_statements_per_call')!=data:raise ValueError('runtime data/transaction census mismatch')
        if row['statements_per_call']!=len(row['census_sql']) or row['postgres_statements']!=len(row['census_sql'])+row['provider_control_queries']:raise ValueError('wrapper/PostgreSQL SQL mismatch')
        if not row['postgres_log'] or not all(row.get(k) is True for k in ('stale_control','refusal_control','fault_control')):raise ValueError('missing runtime controls')
        if (row['persisted_note'],row['persisted_label'])!=('Fresh','Fresh b'):raise ValueError('bad persisted write')


def instant(value):
    result=dt.datetime.fromisoformat(value.replace('Z','+00:00'))
    if result.tzinfo is None:raise ValueError('timestamp lacks timezone')
    return result.timestamp()


def validate_patch(before,after,receipts,cases,successes,started,ended):
    old={r['id']:r for r in before};new={r['id']:r for r in after}
    owned={c['expected_json']['id']:c['owner'] for c in cases}
    if len(owned)!=len(cases) or len(set(owned.values()))!=len(cases):raise ValueError('PATCH cases must have independent owners/rows')
    if len(old)!=len(before) or len(new)!=len(after) or set(old)!=set(new):raise ValueError('PATCH changed row identities')
    if len(receipts)!=successes or not receipts:raise ValueError('incomplete PATCH receipts')
    last={}
    for receipt in receipts:
        if set(receipt)!={'id','owner_id','title','status','due_at','created_at','updated_at'}:raise ValueError('incomplete PATCH response')
        identity=receipt['id']
        if identity not in owned or receipt['owner_id']!=owned[identity] or receipt['title']!='bench-patch':raise ValueError('wrong PATCH identity/owner/title')
        if any(receipt[k]!=old[identity][k] for k in ('status','due_at')) or instant(receipt['created_at'])!=instant(old[identity]['created_at']):raise ValueError('PATCH changed stable response fields')
        if not started-1<=instant(receipt['updated_at'])<=ended+1:raise ValueError('PATCH timestamp outside cell')
        last[identity]=receipt
    if set(last)!=set(owned):raise ValueError('a concurrent client has no verified response')
    for identity,row in new.items():
        if identity not in owned:
            if row!=old[identity]:raise ValueError('PATCH changed an untouched row')
            continue
        expected={**old[identity],'title':'bench-patch','updated_at':row['updated_at']}
        if row!=expected or row['owner_id']!=owned[identity] or not started-1<=instant(row['updated_at'])<=ended+1:raise ValueError('incorrect persisted PATCH')
        if instant(last[identity]['updated_at'])!=instant(row['updated_at']):raise ValueError('final PATCH response differs from persisted row')


def patch_cell(server,pg,owners,concurrency,warmup,duration,row):
    cases=golden.cases_for(server,owners,'read')
    if len(cases)!=256 or len({c['owner'] for c in cases})!=256 or len({c['path'] for c in cases})!=256:raise ValueError('256 independent PATCH owners/rows required')
    before=pg.state(server.target)
    # Refusals must preserve every column of every seeded row.
    status,body=golden.request(server,'PATCH',cases[0]['path'],owners[0]['token'],{'title':''})
    validate_failure(status,body,422,'invalid_input')
    foreign=owners[1] if len(owners)>1 else None
    if foreign is not None:
        status,body=golden.request(server,'PATCH',cases[0]['path'],foreign['token'],{'title':'forbidden'})
        validate_failure(status,body,404,'todo_not_found')
    if pg.state(server.target)!=before:raise ValueError('refused PATCH changed data')
    filename=WORK/f'cases-{secrets.token_hex(8)}.json';filename.write_text(json.dumps(cases));filename.chmod(0o600)
    args=[str(golden.spike.LOAD),'--addr',f'127.0.0.1:{server.port}','--connections',str(concurrency),
          '--warmup',str(warmup),'--duration',str(duration),'--address-pool','16384','--header',server.header,
          '--cases-file',str(filename),'--partition-cases','--method','PATCH','--body','{"title":"bench-patch"}',
          '--expect','200','--expect-updated-todo-title','bench-patch']
    started=time.time();wall=time.monotonic();process=subprocess.Popen(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    try:
        time.sleep(max(0,warmup-(time.monotonic()-wall)))
        cpu=golden.spike.cpu_s(server.process.pid);helpers=sum(golden.spike.cpu_s(p.pid) for p in server.helpers);host=golden.host_cpu_ticks()
        raw=golden.load_output(process,warmup+duration+45,row)
        cpu_after=golden.spike.cpu_s(server.process.pid);helper_after=sum(golden.spike.cpu_s(p.pid) for p in server.helpers);host_after=golden.host_cpu_ticks();ended=time.time()
        row.update(json.loads(raw));validate_measurement(row)
        after=pg.state(server.target);receipts=row.pop('updated_rows');successes=row['all_statuses'].get('200',0)
        validate_patch(before,after,receipts,cases,successes,started,ended)
        # Known faults prove the guard checks both response timestamps and
        # columns outside the changed rows, independently of the service.
        wrong=copy.deepcopy(receipts);wrong[0]['updated_at']='bad'
        try:validate_patch(before,after,wrong,cases,successes,started,ended)
        except ValueError:pass
        else:raise ValueError('timestamp fault control escaped')
        wrong=copy.deepcopy(after);untouched=next(r for r in wrong if r['id'] not in {c['expected_json']['id'] for c in cases});untouched['title']='bad'
        try:validate_patch(before,wrong,receipts,cases,successes,started,ended)
        except ValueError:pass
        else:raise ValueError('untouched-row fault control escaped')
        row.update(verified=True,refusal_control=True,fault_control=True,verified_responses=len(receipts),
            cpu_ms_per_request=(cpu_after-cpu)*1000/row['requests'],helper_cpu_ms_per_request=(helper_after-helpers)*1000/row['requests'],
            load_cpu_cores=row['load_cpu_s']/row['load_wall_s'],
            host_cpu_percent=100*(host_after[0]-host[0])/(host_after[1]-host[1]) if host and host_after and host_after[1]>host[1] else None,
            state_before=before,state_after=after,receipts=receipts,started=started,ended=ended)
    finally:
        if process.poll() is None:golden.spike.stop(process)
        filename.unlink(missing_ok=True)


def measure(label,kind,count,warmup,duration):
    directory=WORK/label;stamp=json.loads((directory/'build.json').read_text());guard.validate(stamp,guard.source())
    golden.validate_build(stamp['golden'],golden.fingerprint())
    output=directory/f'{kind}-{time.time_ns()}.json'
    result={'protocol':1,'kind':kind,'build':stamp,'start_source':guard.source(),'start':context(),'complete':False}
    before_cpu=resource.getrusage(resource.RUSAGE_CHILDREN);start=time.monotonic()
    try:
        if kind=='runtime':
            raw=output.with_suffix('.raw.json')
            with output.with_suffix('.log').open('w') as log:command([stamp['fixture'],str(raw),str(count)],env={**ENV,'JADPO_BENCH_COMPILER':str(ROOT/'target/debug/jadpo')},stdout=log,stderr=log)
            data=json.loads(raw.read_text());validate_runtime(data);result['result']=data;result['raw_sha256']=guard.sha(raw)
        else:
            result.update(rounds=3,concurrencies=[1,8],warmup=warmup,duration=duration,rows=[],starts=[])
            pg=golden.Database()
            try:
                for target in ('workers','bun'):pg.prepare(target)
                for round_ in range(1,4):
                    for target in (('workers','bun') if round_%2 else ('bun','workers')):
                        for concurrency in (1,8):
                            environment=golden.configuration(pg.url(target),secrets.token_urlsafe(32));owners,signed=golden.credentials(target,environment,256);golden.seed(pg,target,owners)
                            server=golden.Server(target,pg,environment)
                            try:
                                if time.time()*1000-signed+(warmup+duration+45)*1000>=230000:raise ValueError('credentials too old')
                                result['starts'].append({'target':target,'round':round_,'connections':concurrency,'cold_ms':server.cold_ms,'first_ms':server.first_ms})
                                row={'target':target,'round':round_,'connections':concurrency,'verified':False};result['rows'].append(row)
                                patch_cell(server,pg,owners,concurrency,warmup,duration,row)
                                print(f'verified PATCH {target} round {round_} x{concurrency}',file=sys.stderr,flush=True)
                            finally:server.stop()
            finally:pg.stop()
        result['complete']=True
    except BaseException as error:
        result['failure']=str(error);raise
    finally:
        cpu=resource.getrusage(resource.RUSAGE_CHILDREN)
        result.update(seconds=time.monotonic()-start,end=context(),whole_children_cpu_s=cpu.ru_utime+cpu.ru_stime-before_cpu.ru_utime-before_cpu.ru_stime,
            cpu_scope='whole reaped benchmark children, setup/warmup/controls included; daemonized PostgreSQL backend CPU excluded/unmeasured; not service CPU')
        try:
            guard.validate(stamp,guard.source());golden.validate_build(stamp['golden'],golden.fingerprint());result.update(fresh_at_end=True,end_source=guard.source())
        finally:output.write_text(json.dumps(result,indent=2)+'\n')
    print(output)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('action',choices=('build','runtime','patch'));parser.add_argument('--label',required=True);parser.add_argument('--calls',type=int,default=500);parser.add_argument('--warmup',type=float,default=2);parser.add_argument('--duration',type=float,default=5);args=parser.parse_args()
    if args.calls<1 or not math.isfinite(args.warmup) or args.warmup<0 or not math.isfinite(args.duration) or args.duration<=0 or args.warmup+args.duration>120:parser.error('positive bounded protocol required')
    os.environ.update(FLAGS);golden.spike.WORK=golden.WORK
    if args.action=='build':build(args.label)
    else:measure(args.label,args.action,args.calls,args.warmup,args.duration)
