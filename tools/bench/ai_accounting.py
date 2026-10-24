#!/usr/bin/env python3
"""Guarded native-runtime AI accounting microfixture (not HTTP/Workers/Bun).

Build each clean reference/candidate using the SAME final harness. A stamp
pins the complete tracked tree, modes, HEAD and all executable bytes. Timing
fails closed after a source/HEAD/artifact change; no historical bypass.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / 'tools/bench/work/ai-accounting'
ENV = {**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_BUILD_JOBS': '4'}


def command(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def tracked_entry(path, mode):
    # Git records a symlink's literal target, not the destination's bytes.
    # With core.symlinks=false Git materializes that payload as a file.
    if mode == '120000':
        payload = os.fsencode(os.readlink(path)) if path.is_symlink() else path.read_bytes()
    elif mode in ('100644', '100755'):
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'tracked regular file changed type: {path}')
        actual_mode = '100755' if path.stat().st_mode & 0o111 else '100644'
        if actual_mode != mode:
            raise ValueError(f'tracked executable mode changed: {path}')
        payload = path.read_bytes()
    else:
        raise ValueError(f'unsupported tracked mode {mode}: {path}')
    return {'mode':mode, 'sha256':hashlib.sha256(payload).hexdigest()}


def source():
    dirty = command(['git', 'status', '--porcelain', '--untracked-files=normal'], capture_output=True, text=True).stdout
    if dirty.strip():
        raise ValueError('commit all source inputs before building or measuring')
    names = command(['git', 'ls-files', '-s', '-z'], capture_output=True).stdout.split(b'\0')
    files = {}
    for entry in filter(None, names):
        identity, raw_name = entry.split(b'\t', 1)
        raw_mode, _, stage = identity.split()
        if stage != b'0': raise ValueError('unmerged index')
        mode, name = raw_mode.decode('ascii'), os.fsdecode(raw_name)
        path = ROOT / name
        # A mode mutation can leave contents unchanged; status catches it,
        # and retaining the tree/index modes makes that proof reviewable.
        files[name] = tracked_entry(path, mode)
    return {'head': command(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip(),
            'tree': command(['git', 'write-tree'], capture_output=True, text=True).stdout.strip(), 'files': files}


def validate(stamp, current):
    if stamp['source'] != current:
        raise ValueError('HEAD, complete tracked source tree or mode changed: rebuild required')
    for path, digest in stamp['artifacts'].items():
        if sha(path) != digest: raise ValueError(f'artifact changed: {path}')
    if not stamp['artifacts']: raise ValueError('artifacts missing')


def validate_results(result):
    if result.get('protocol') != 2 or result.get('complete') is not True:
        raise ValueError('incomplete/faulted benchmark result')
    count = result.get('count')
    if not isinstance(count,int) or count <= 0 or result.get('warmup') != 50:
        raise ValueError('invalid protocol count')
    expected = {(r,s) for r in range(1,4) for s in ('app','caller','retry-caller')}
    runs = result.get('runs',[])
    if len(runs)!=9 or {(r.get('round'),r.get('scenario')) for r in runs} != expected:
        raise ValueError('incomplete round/scenario matrix')
    for run in runs:
        latency=run.get('latency_ms',[])
        if len(latency)!=count or any(not isinstance(v,(int,float)) or not math.isfinite(v) or v<=0 for v in latency):
            raise ValueError('invalid latency samples')
        ordered=sorted(latency)
        if run.get('p50_ms') != ordered[(count-1)//2] or run.get('p95_ms') != ordered[math.ceil(count*.95)-1]:
            raise ValueError('incorrect quantiles')
        seconds=run.get('seconds',0)
        if not math.isfinite(seconds) or seconds<=0 or not math.isclose(run.get('calls_per_second',0),count/seconds):
            raise ValueError('incorrect throughput')
        sql=run.get('census_sql',[])
        if not sql or run.get('statements_per_call') != len(sql) or run.get('postgres_statements') != len(sql):
            raise ValueError('SQL wrapper/server census mismatch')
        if not run.get('postgres_log') or any(not row.get('sql') or not row.get('params') for row in sql):
            raise ValueError('SQL census evidence missing')
        if run.get('fault_control') is not True or run.get('refusal_control') is not True:
            raise ValueError('missing known-fault/refusal controls')
        caller=run['scenario']!='app'
        for name,n in [('census_before',50),('census_after',51),('before',51),('after',51+count)]:
            rows=run.get(name,[])
            scopes=['app','caller','prompt'] if caller else ['app','prompt']
            if [r.get('scope') for r in rows] != scopes:
                raise ValueError('missing accounting window')
            for row in rows:
                expected_values={'calls':n,'input_tokens':n*(13 if run['scenario']=='retry-caller' else 6),
                                 'output_tokens':n,'cached_tokens':2*n,'reserved_tokens':0,
                                 'cost_micros':0,'reserved_micros':0,'currency':None}
                if any(row.get(k)!=v for k,v in expected_values.items()):
                    raise ValueError('incorrect persisted accounting')
                key=row.get('key')
                if row['scope']=='app' and key!='':raise ValueError('incorrect APP key')
                if row['scope']=='prompt' and key!='answer local/model':raise ValueError('incorrect prompt key')
                if row['scope']=='caller' and (not isinstance(key,str) or len(key)!=32 or any(c not in '0123456789abcdef' for c in key)):
                    raise ValueError('incorrect caller digest')


def build(label):
    before = source()
    directory = WORK / label
    directory.mkdir(parents=True, exist_ok=False)
    # Compiler is provenance/supporting context, not in the timed path.
    # The measured executable is release Rust using the production runtime
    # with its testing-only AiFake, never a hand-written SQL replacement.
    with (directory / 'build.log').open('w') as log:
        command(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '--bin', 'jadpo'], env=ENV, stdout=log, stderr=log)
        command(['cargo', 'build', '--locked', '--release', '-p', 'jadpo-test', '--bin', 'jadpo-bench-ai'], env=ENV, stdout=log, stderr=log)
    paths = []
    for original, name in [(ROOT/'target/debug/jadpo', 'compiler-jadpo'), (ROOT/'target/release/jadpo-bench-ai', 'fixture')]:
        saved = directory / name
        shutil.copy2(original, saved)
        paths.extend([original, saved])
    if before != source(): raise ValueError('source changed during build')
    stamp = {'source': before, 'artifacts': {str(p):sha(p) for p in paths},
             'flags': {k:ENV[k] for k in ('CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_BUILD_JOBS')},
             'profiles': {'fixture':'release, production runtime + testing-only AiFake', 'compiler':'dev, excluded from timing'},
             'versions': {name:command([name, '--version'], capture_output=True, text=True).stdout.strip() for name in ('rustc','python3')},
             'host': {'platform':platform.platform(), 'cores':os.cpu_count()}}
    (directory/'build.json').write_text(json.dumps(stamp, indent=2)+'\n')
    validate(stamp, source())


def measure(label, count):
    directory = WORK / label
    stamp = json.loads((directory/'build.json').read_text())
    validate(stamp, source())
    output = directory/f'runs-{time.time_ns()}.json'
    metadata = {'build':stamp, 'start_source':source(), 'host_start':{'load':os.getloadavg(), 'time':time.time()},
                'workers_bun':'N/A: no matched AI runtime workload; actual compiled Worker refusal correctness tested separately'}
    meta = output.with_suffix('.meta.json')
    meta.write_text(json.dumps(metadata,indent=2)+'\n')
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    began = time.perf_counter()
    try:
        with output.with_suffix('.log').open('w') as log:
            command([str(directory/'fixture'), str(output), str(count)], env=ENV, stdout=log, stderr=log)
        validate_results(json.loads(output.read_text()))
        metadata['completed'] = True
    finally:
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        metadata.update({'seconds':time.perf_counter()-began,
                         'whole_fixture_children_cpu_s':after.ru_utime+after.ru_stime-before.ru_utime-before.ru_stime,
                         'cpu_scope':'whole fixture process and reaped setup/watchdog/client children, including warmup/controls/census; daemonized PostgreSQL backend CPU excluded/unmeasured; not per-call/service CPU',
                         'host_end':{'load':os.getloadavg(),'time':time.time()}})
        try:
            validate(stamp, source())
            metadata['fresh_at_end'] = True
            metadata['end_source'] = source()
        finally:
            metadata['raw_sha256'] = sha(output) if output.exists() else None
            meta.write_text(json.dumps(metadata,indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='action',required=True)
    for action in ('build','measure'):
        p=sub.add_parser(action);p.add_argument('--label',required=True)
        if action=='measure':p.add_argument('--calls',type=int,default=500)
    args=parser.parse_args()
    if args.action=='build':build(args.label)
    else:measure(args.label,args.calls)
