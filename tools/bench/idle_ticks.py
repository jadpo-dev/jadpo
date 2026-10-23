#!/usr/bin/env python3
"""Production idle ticks: SQL census separately from latency (no Bun equivalent).

Uses deliveries.py's clean-source/compiler/artifact guards and real generated
native/Workers deployables. Never runs against a configured external database.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import resource
import time

import deliveries as d
import golden
import run as spike


def measure(args):
    stamp = d.fresh(args.label)
    directory = d.WORK / args.label
    output = directory / f'idle-{time.time_ns()}.json'
    result = {'build': stamp, 'protocol': {'ticks': args.ticks, 'rounds': args.rounds,
        'warmup': 10, 'counts': 'separate PostgreSQL log_statement=all census; logging off for latency',
        'bun': 'N/A: specialized delivery fixture has no generic housekeeping',
        'native': 'fresh external tick process per sample',
        'workers': 'persistent local workerd + pool4, production scheduled handler'},
        'host': {'platform': platform.platform(), 'cores': os.cpu_count(), 'load': os.getloadavg()}, 'runs': []}
    def save():
        output.write_text(json.dumps(result, indent=2) + '\n')
    save()
    spike.WORK = directory
    pg = d.Database.__new__(d.Database)
    owned = []
    try:
        pg.__init__()
        pg.project = directory / 'native'
        for target in ('native','workers'):
            pg.prepare(target)
        for round_index in range(args.rounds):
            for target in (('workers','native') if round_index % 2 == 0 else ('native','workers')):
                process = helper = port = None
                row = {'target': target, 'round': round_index+1, 'ok': False}
                result['runs'].append(row)
                save()
                try:
                    if target == 'workers':
                        process, helper, port = d.worker(directory,pg,directory/f'idle-workers-{round_index}.log',owned)
                    for _ in range(10):
                        d.tick(target,directory,pg,port)
                    pg.sql('postgres', "ALTER SYSTEM SET log_statement='all'")
                    pg.sql('postgres', "SELECT pg_reload_conf()")
                    time.sleep(.1)
                    log = directory/'pg.log'
                    start = log.stat().st_size
                    row['census_response'] = d.tick(target,directory,pg,port)
                    time.sleep(.1)
                    census = log.read_bytes()[start:].decode()
                    row['sql_log'] = census
                    row['statement_count'] = len(re.findall(r'LOG:  (?:statement:|execute [^:]*:)',census))
                    assert row['statement_count'] > 0, census
                    pg.sql('postgres', "ALTER SYSTEM SET log_statement='none'")
                    pg.sql('postgres', "SELECT pg_reload_conf()")
                    time.sleep(.1)
                    row['host_cpu_before'] = golden.host_cpu_ticks()
                    before = resource.getrusage(resource.RUSAGE_CHILDREN)
                    service_before = spike.cpu_s(process.pid) if process else 0
                    helper_before = spike.cpu_s(helper.pid) if helper else 0
                    row['latency_ms'] = []
                    began = time.perf_counter()
                    for _ in range(args.ticks):
                        at = time.perf_counter()
                        d.tick(target,directory,pg,port)
                        row['latency_ms'].append((time.perf_counter()-at)*1000)
                    row['seconds'] = time.perf_counter()-began
                    row['ticks_per_second'] = args.ticks/row['seconds']
                    after = resource.getrusage(resource.RUSAGE_CHILDREN)
                    row['external_children_cpu_s'] = after.ru_utime+after.ru_stime-before.ru_utime-before.ru_stime
                    row['service_cpu_s'] = spike.cpu_s(process.pid)-service_before if process else None
                    row['helper_cpu_s'] = spike.cpu_s(helper.pid)-helper_before if helper else None
                    row['server_cpu_scope'] = 'workerd+pool helper, or native external child; excludes PostgreSQL and Python'
                    row['host_cpu_after'] = golden.host_cpu_ticks()
                    row['load'] = os.getloadavg()
                    row['ok'] = True
                finally:
                    for child in (process,helper):
                        if child: spike.stop(child)
                    save()
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        for child in reversed(owned): spike.stop(child)
        if hasattr(pg,'data'): pg.stop()
        try:
            d.fresh(args.label)
            result['fresh_at_end'] = True
        finally: save()
    result['complete'] = len(result['runs']) == args.rounds*2 and all(r['ok'] for r in result['runs'])
    save()
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--label',required=True)
    parser.add_argument('--ticks',type=int,default=100)
    parser.add_argument('--rounds',type=int,default=3)
    args=parser.parse_args()
    if not __debug__ or args.ticks<1 or args.rounds<1: parser.error('positive samples; assertions enabled')
    measure(args)
