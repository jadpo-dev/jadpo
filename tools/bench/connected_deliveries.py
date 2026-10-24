#!/usr/bin/env python3
"""Guarded connected and connectionless delivery comparisons; local workerd, warm Bun.

The connected cohort performs one real package-driver safe GET per committed
receipt. Labels pin the complete tracked tree, modes, HEAD and runtime bytes.
The connectionless 10k control retains its original application and service.
"""
from __future__ import annotations
import argparse
from collections import Counter
import datetime as dt
import http.server
import json
import math
import os
from pathlib import Path
import re
import resource
import shutil
import subprocess
import threading
import time

import ai_accounting as guard
import deliveries as base
import golden
import version_writes as flags

ROOT = base.ROOT
HERE = base.HERE
WORK = HERE / 'work/connected-deliveries'
ENV = {**flags.ENV, 'CHECKS_API_KEY':'benchmark-key', 'CHECKS_ENVIRONMENT':'sandbox'}
TARGETS = ('workers','bun','native','node')
PORT = 18765


def command(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def context():
    value = {'time':time.time(),'load':os.getloadavg(),'host_cpu':golden.host_cpu_ticks(),
             'flags':flags.selected_flags(ENV),'platform':os.uname().sysname}
    for name,args in [('memory',['vm_stat']),('swap',['sysctl','vm.swapusage']),
                      ('pressure',['memory_pressure','-Q']),('processes',['ps','-axo','pid=,ppid=,pcpu=,comm='])]:
        answer = command(args,capture_output=True,text=True)
        value[name] = answer.stdout
    return value


class Provider:
    """A local HTTP provider, never a runtime fake or replayed capability."""
    def __init__(self):
        self.calls = []
        self.status = 200
        self.lock = threading.Lock()
        owner = self
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'
            def do_GET(self):
                with owner.lock:
                    owner.calls.append({'method':'GET','path':self.path,'authorization':self.headers.get('authorization')})
                    status = owner.status
                ticket = self.path.removeprefix('/tickets/')
                answer = {'ticket':ticket} if status == 200 else {'error':{'code':'ticket_unknown'}}
                body = json.dumps(answer).encode()
                self.send_response(status)
                self.send_header('content-type','application/json')
                self.send_header('content-length',str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def log_message(self,*args): pass
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1',PORT),Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever,daemon=True)
        self.thread.start()
    def reset(self,status=200):
        with self.lock:
            self.calls.clear();self.status=status
    def snapshot(self):
        with self.lock:return list(self.calls)
    def close(self):
        self.server.shutdown();self.server.server_close();self.thread.join()


def validate_provider(calls,count):
    expected = Counter(f'/tickets/ticket-{i:05d}' for i in range(1,count+1))
    if len(calls)!=count or Counter(row.get('path') for row in calls)!=expected:
        raise ValueError('provider sends missing, duplicated or outside seeded cohort')
    if any(row.get('method')!='GET' or row.get('authorization')!='Bearer benchmark-key' for row in calls):
        raise ValueError('provider method or credential differs')


def validate_rows(before,after,count,started,ended):
    if len(before)!=count or len(after)!=count:raise ValueError('missing delivery rows')
    old={r['id']:r for r in before};new={r['id']:r for r in after}
    if len(old)!=count or set(old)!=set(new):raise ValueError('delivery identities changed')
    allowed = {'status','attempts','last_tried_at','next_attempt_at'}
    for identity,row in old.items():
        final=new[identity]
        if set(row)!=set(final) or any(row[k]!=final[k] for k in row if k not in allowed):
            raise ValueError('unexpected stored delivery column changed')
        if final['status']!='succeeded' or final['attempts']!=1 or final['next_attempt_at'] is not None:
            raise ValueError('delivery did not settle exactly once')
        tried=flags.instant(final['last_tried_at'])
        if not started-1 <= tried <= ended+1:raise ValueError('claim clock not in measured interval')


def validate_handled(deliveries,handled,started,ended):
    if len(handled)!=len(deliveries) or {r['id'] for r in handled}!={r['id'] for r in deliveries}:
        raise ValueError('handled marker identities differ from deliveries')
    for row in handled:
        if set(row)!={'id','handled_at'} or not started-1<=flags.instant(row['handled_at'])<=ended+1:
            raise ValueError('handled marker columns or clock differ')


def validate_census(census,connected):
    sql=census.get('sql',[])
    if not sql or not census.get('postgres_log'):raise ValueError('missing actual PostgreSQL SQL evidence')
    data=[s for s in sql if not re.match(r'^(BEGIN|COMMIT|ROLLBACK|SAVEPOINT|RELEASE)(\s|$)',s,re.I)]
    if census.get('data_statements')!=len(data) or census.get('transaction_statements')!=len(sql)-len(data):
        raise ValueError('data/transaction census mismatch')
    claims=[s for s in sql if '"attempts" + 1' in s or 'attempts=attempts+1' in s]
    if len(claims)!=1:raise ValueError('census must observe one actual claim')
    independent=sum(s.startswith('SELECT "connection", "state", "failures"') or
                    ('FROM "jadpo_circuit_breakers" WHERE "connection" = ANY' in s and not s.startswith('WITH')) for s in sql)
    joined=sum(s.startswith('WITH "claimed" AS') for s in claims)
    if connected and (independent,joined) not in ((1,0),(0,1)):
        raise ValueError('connected claim must read breakers exactly once')
    if not connected and (independent or joined):raise ValueError('connectionless control gained breaker work')
    if census.get('independent_breaker_reads')!=independent or census.get('joined_claims')!=joined:
        raise ValueError('breaker census aggregation differs')


def build(label):
    before=guard.source();directory=WORK/label;directory.mkdir(parents=True,exist_ok=False)
    stamp={'source':before,'start':context(),'inherited_flags':flags.INHERITED_FLAGS,'artifacts':{},'complete':False}
    (directory/'build.json').write_text(json.dumps(stamp,indent=2)+'\n')
    command(['cargo','build','--locked','-p','jadpo-cli','--bin','jadpo'],env=ENV)
    command(['npm','ci','--offline','--ignore-scripts','--no-audit','--no-fund'],cwd=HERE/'typescript')
    command(['npm','run','build'],cwd=HERE/'typescript')
    command(['npm','ci','--offline','--ignore-scripts','--no-audit','--no-fund'],cwd=ROOT/'tools/workers')
    command(['cargo','build','--locked','--release','--manifest-path',str(HERE/'load/Cargo.toml'),'--bin','pool'],env=ENV)
    paths=[base.spike.JADPO,base.spike.POOL]
    for workload in ('connected','backlog'):
        fixture=HERE/'fixtures'/('connected-deliveries' if workload=='connected' else 'deliveries')
        for target in ('native','workers'):
            project=directory/workload/target;shutil.copytree(fixture,project)
            app=(project/'app.jadpo').read_text()
            if target=='workers':
                app=app.replace('application {','''application {
    rate_limits: { store: limits }
    deployables: { kiosk: { serves: [main] runs: [Trigger.subscribers, Trigger.schedules] target: Target.wasm } }''',1)
                app=app.replace('    connections: {','    connections: {\n        limits: { driver: Driver.cloudflare_rate_limits namespace: 1001 }',1)
            (project/'app.jadpo').write_text(app)
            command([str(base.spike.JADPO),'check'],cwd=project,env=ENV)
            command([str(base.spike.JADPO),'build'],cwd=project,env=ENV)
            out=project/'build/target'/('app' if target=='native' else 'kiosk')
            paths.extend(p for p in out.iterdir() if p.is_file())
    stamp['runtimes']={}
    for name in ('workerd','bun','node','psql','rustc','cargo'):
        path=Path(base.spike.workerd() if name=='workerd' else base.spike.tool(name)).resolve()
        stamp['runtimes'][name]={'path':str(path),'version':command([str(path),'--version'],capture_output=True,text=True).stdout.strip(),'sha256':guard.sha(path)}
        paths.append(path)
    paths.extend((HERE/'typescript/build').glob('*.js'))
    paths.extend(p for p in (HERE/'typescript/node_modules').rglob('*') if p.is_file() and not p.is_symlink())
    saved=directory/'artifacts';saved.mkdir()
    for index,path in enumerate(dict.fromkeys(paths)):
        copy=saved/f'{index:05d}-{path.name}';shutil.copy2(path,copy)
        stamp['artifacts'][str(path)]=guard.sha(path);stamp['artifacts'][str(copy)]=guard.sha(copy)
    if guard.source()!=before:raise ValueError('source changed during build')
    stamp.update(end=context(),complete=True,profiles={'native':'production release; external tick includes process startup',
        'workers':'production release Wasm; local persistent workerd; no wasm-opt',
        'bun/node':'persistent warm service and postgres-js max4; same authored code across reference/candidate'},
        flags=flags.selected_flags(ENV))
    (directory/'build.json').write_text(json.dumps(stamp,indent=2)+'\n')
    guard.validate(stamp,guard.source())


class Database(base.Database):
    def rows(self,target):
        return json.loads(self.query(target,'SELECT coalesce(json_agg(row_to_json(d) ORDER BY id),\'[]\') FROM jadpo_deliveries d'))
    def handled(self,target):
        return json.loads(self.query(target,"SELECT coalesce(json_agg(row_to_json(h) ORDER BY id),'[]') FROM jadpo_handled_deliveries h"))
    def breakers(self,target):
        return json.loads(self.query(target,"SELECT coalesce(json_agg(row_to_json(b) ORDER BY connection),'[]') FROM jadpo_circuit_breakers b"))
    def closed(self,target):
        self.sql(target,"INSERT INTO jadpo_circuit_breakers(connection,state,failures,pauses) VALUES('checks','closed',0,0)")
    def reset(self,target,template,count):
        self.seed(target,template,count)
        self.sql(target,'TRUNCATE jadpo_circuit_breakers')


def service(target,folder,pg,owned,connected):
    if target=='workers':
        extra=['(name="CHECKS_API_KEY",text="benchmark-key")','(name="CHECKS_ENVIRONMENT",text="sandbox")'] if connected else []
        process,helper,port=base.worker(folder,pg,folder/f'{target}.log',owned,extra_bindings=extra)
        return process,helper,port
    if target in ('bun','node'):
        port=base.spike.free_port()
        script='connected_deliveries.js' if connected else 'deliveries.js'
        with (folder/f'{target}.log').open('ab') as log:
            process=subprocess.Popen([base.spike.tool(target),str(HERE/'typescript/build'/script),'--serve'],env={**ENV,
                'DATABASE_URL':pg.url(target),'PORT':str(port),'PROVIDER_URL':f'http://127.0.0.1:{PORT}'},stdout=log,stderr=log)
        owned.append(process)
        for _ in range(200):
            if process.poll() is not None:raise ValueError(f'{target} exited: '+(folder/f'{target}.log').read_text()[-4000:])
            try:
                if base.request(port,'/tick',method='POST')[0]==200:return process,None,port
            except OSError:pass
            time.sleep(.025)
        raise ValueError('service readiness failed')
    return None,None,None


def parse_log(path,offset,target):
    text=path.read_bytes()[offset:].decode()
    entries=[]
    prefix=f'BENCH|{target}|'
    active=False
    for line in text.splitlines():
        if line.startswith('BENCH|'):
            active=False
            if not line.startswith(prefix):continue
            match=re.search(r'LOG:  (?:execute [^:]*:|statement:) (.*)',line)
            if match:
                entries.append(match.group(1));active=True
        elif active:
            entries[-1]+='\n'+line.lstrip('\t')
    return text,entries


def census(target,folder,pg,port,provider,template,connected):
    pg.reset(target,template,1);provider.reset()
    if connected:pg.closed(target)
    pg.sql('postgres',"ALTER SYSTEM SET log_statement='all'")
    pg.query('postgres','SELECT pg_reload_conf(); SELECT pg_sleep(0.1)')
    log=Path(base.spike.WORK)/'pg.log';offset=log.stat().st_size
    response=base.tick(target,folder,pg,port)
    # Reading the local log is independent of the driver's statement wrappers.
    time.sleep(.05)
    text,sql=parse_log(log,offset,target)
    pg.sql('postgres',"ALTER SYSTEM SET log_statement='none'")
    pg.query('postgres','SELECT pg_reload_conf(); SELECT pg_sleep(0.1)')
    base.validate_backlog(pg.state(target),1)
    if connected:validate_provider(provider.snapshot(),1)
    elif provider.snapshot():raise ValueError('connectionless census called provider')
    data=[s for s in sql if not re.match(r'^(BEGIN|COMMIT|ROLLBACK|SAVEPOINT|RELEASE)(\s|$)',s,re.I)]
    result={'response':response,'postgres_log':text,'sql':sql,'provider_control_queries':0,
        'data_statements':len(data),'transaction_statements':len(sql)-len(data),
        'independent_breaker_reads':sum(s.startswith('SELECT "connection", "state", "failures"') or ('FROM "jadpo_circuit_breakers" WHERE "connection" = ANY' in s and not s.startswith('WITH')) for s in sql),
        'joined_claims':sum(s.startswith('WITH "claimed" AS') for s in sql)}
    validate_census(result,connected)
    return result


def controls(target,folder,pg,port,provider,template,connected):
    reports=[]
    for name in ('replay','live-lease','expired-lease','malformed','rollback','claim-error','provider-refused','provider-unavailable','open-hold','closed-breaker'):
        if not connected and name.startswith('provider-'):continue
        if not connected and name in ('open-hold','closed-breaker'):continue
        pg.reset(target,template,1);provider.reset();report={'name':name}
        if name=='replay':
            base.tick(target,folder,pg,port);provider.reset()
            pg.sql(target,"UPDATE jadpo_deliveries SET status='pending',next_attempt_at=clock_timestamp()-interval '1 minute'")
        elif name=='live-lease':pg.sql(target,"UPDATE jadpo_deliveries SET status='running',next_attempt_at=clock_timestamp()+interval '1 hour'")
        elif name=='expired-lease':pg.sql(target,"UPDATE jadpo_deliveries SET status='running'")
        elif name=='malformed':pg.sql(target,"UPDATE jadpo_outbox SET payload='{\"ticket\":42}'")
        elif name=='rollback':pg.sql(target,'ALTER TABLE jadpo_handled_deliveries ADD CONSTRAINT benchmark_refusal CHECK(false) NOT VALID')
        elif name=='claim-error':pg.sql(target,"ALTER TABLE jadpo_deliveries ADD CONSTRAINT benchmark_claim_refusal CHECK(status<>'running') NOT VALID")
        elif name=='provider-refused':provider.reset(409)
        elif name=='provider-unavailable':provider.reset(503)
        elif name=='closed-breaker':pg.closed(target)
        elif name=='open-hold':pg.sql(target,"INSERT INTO jadpo_circuit_breakers(connection,state,failures,pauses,probe_at) VALUES('checks','open',5,1,clock_timestamp()+interval '1 hour')")
        try:
            report['response']=base.tick(target,folder,pg,port)
        except base.TickError:
            if name!='claim-error':raise
            report['response']='claim rejected'
        finally:
            if name=='rollback':pg.sql(target,'ALTER TABLE jadpo_handled_deliveries DROP CONSTRAINT benchmark_refusal')
            if name=='claim-error':pg.sql(target,'ALTER TABLE jadpo_deliveries DROP CONSTRAINT benchmark_claim_refusal')
        state=pg.state(target);rows=pg.rows(target);calls=provider.snapshot()
        report.update(state=state,rows=rows,provider_calls=calls)
        if name in ('replay','expired-lease','closed-breaker'):
            if state['receipts']!=1 or state['handled']!=1 or state['succeeded']!=1:raise ValueError(report)
        elif state['receipts'] or state['handled'] or state['succeeded']:raise ValueError(report)
        expected=1 if connected and name in ('expired-lease','rollback','provider-refused','provider-unavailable','closed-breaker') else 0
        if len(calls)!=expected:raise ValueError(report)
        expected_attempt=2 if name=='replay' else (1 if name in ('expired-lease','rollback','provider-refused','provider-unavailable','closed-breaker') else 0)
        if rows[0]['attempts']!=expected_attempt:raise ValueError(report)
        if name=='open-hold' and (rows[0]['status']!='held' or rows[0]['held_for']!='checks' or rows[0]['held_since'] is None):raise ValueError(report)
        if name=='provider-unavailable' and (rows[0]['status']!='pending' or rows[0]['last_failure']!={'code':'unavailable','kind':'unavailable'}):raise ValueError(report)
        if name=='provider-refused' and rows[0]['status']!='failed':raise ValueError(report)
        report['breakers']=pg.breakers(target)
        if name=='provider-unavailable':
            b=report['breakers']
            if len(b)!=1 or b[0]['state']!='closed' or b[0]['failures']!=1:raise ValueError(report)
            # Retry the same persisted identity after the provider recovers.
            provider.reset();pg.sql(target,"UPDATE jadpo_deliveries SET next_attempt_at=clock_timestamp()-interval '1 second'")
            report['retry_response']=base.tick(target,folder,pg,port)
            report['retry_state']=pg.state(target);report['retry_rows']=pg.rows(target);report['retry_calls']=provider.snapshot()
            base.validate_backlog({**report['retry_state'],'attempts':1},1)
            if report['retry_rows'][0]['attempts']!=2:raise ValueError(report)
            validate_provider(report['retry_calls'],1)
            if pg.breakers(target)[0]['failures']!=0:raise ValueError(report)
        report['ok']=True;reports.append(report)
    return reports


def measure(label,count,rounds,targets):
    directory=WORK/label;stamp=json.loads((directory/'build.json').read_text())
    if stamp.get('complete') is not True:raise ValueError('incomplete build')
    guard.validate(stamp,guard.source())
    output=directory/f'runs-{time.time_ns()}.json'
    result={'protocol':1,'build':stamp,'start_source':guard.source(),'start':context(),'count':count,'rounds':rounds,
            'targets':targets,'runs':[],'controls':{},'complete':False}
    def save():output.write_text(json.dumps(result,indent=2)+'\n')
    save();previous=dict(os.environ);os.environ.update(ENV)
    base.spike.WORK=directory;pg=None;provider=None;owned=[]
    try:
        provider=Provider();pg=Database();pg.project=directory/'connected/native'
        for target in TARGETS:pg.prepare(target)
        pg.sql('postgres',"ALTER SYSTEM SET log_line_prefix='BENCH|%d|%a|%p| '")
        pg.query('postgres','SELECT pg_reload_conf(); SELECT pg_sleep(0.1)')
        for workload in ('connected','backlog'):
            folder=directory/workload;connected=workload=='connected'
            pg.project=folder/'native'
            template=base.calibrate(folder,pg)
            for round_index in range(rounds):
                order=targets[round_index%len(targets):]+targets[:round_index%len(targets)]
                for target in order:
                    pg.reset(target,template,1)
                    pg.sql(target,"UPDATE jadpo_deliveries SET next_attempt_at=clock_timestamp()+interval '1 hour'")
                    process,helper,port=service(target,folder,pg,owned,connected)
                    row={'workload':workload,'round':round_index+1,'target':target,'ok':False}
                    result['runs'].append(row);save()
                    try:
                        key=f'{workload}:{target}'
                        if key not in result['controls']:
                            result['controls'][key]=controls(target,folder,pg,port,provider,template,connected)
                        # Match lifetimes and pool/provider warmup on every target.
                        pg.reset(target,template,100);provider.reset();base.tick(target,folder,pg,port)
                        base.validate_backlog(pg.state(target),100)
                        if connected:validate_provider(provider.snapshot(),100)
                        row['census']=census(target,folder,pg,port,provider,template,connected)
                        pg.reset(target,template,count);provider.reset()
                        if connected:pg.closed(target)
                        row['breakers_before']=pg.breakers(target)
                        before_rows=pg.rows(target);row['before_rows']=before_rows
                        row['seed_fingerprint']=pg.fingerprint(target)
                        row['process_pid']=process.pid if process else None
                        row['host_before']=context();cpu=base.spike.cpu_s(process.pid) if process else None
                        helper_cpu=base.spike.cpu_s(helper.pid) if helper else None
                        controller_cpu=time.process_time();child_before=resource.getrusage(resource.RUSAGE_CHILDREN)
                        started=time.time();began=time.perf_counter();ticks=[];responses=[]
                        for _ in range(math.ceil(count/100)):
                            tick_start=time.perf_counter();responses.append(base.tick(target,folder,pg,port));ticks.append((time.perf_counter()-tick_start)*1000)
                        seconds=time.perf_counter()-began;ended=time.time()
                        row.update(seconds=seconds,deliveries_per_second=count/seconds,tick_ms=ticks,
                            tick_p95_ms=sorted(ticks)[math.ceil(len(ticks)*.95)-1],responses=responses,
                            service_cpu_seconds=base.spike.cpu_s(process.pid)-cpu if process else None,
                            helper_cpu_seconds=base.spike.cpu_s(helper.pid)-helper_cpu if helper else None,
                            controller_provider_cpu_seconds=time.process_time()-controller_cpu,
                            rss_mb_at_end=base.spike.rss_mb(process.pid) if process else None,host_after=context(),
                            timing_started=started,timing_ended=ended)
                        child_after=resource.getrusage(resource.RUSAGE_CHILDREN)
                        row['reaped_child_cpu_seconds']=child_after.ru_utime+child_after.ru_stime-child_before.ru_utime-child_before.ru_stime
                        row['cpu_scope']='service and workerd pool separately; Python controller/local provider separate; PostgreSQL excluded; native child includes startup'
                        row['state']=pg.state(target);base.validate_backlog(row['state'],count)
                        row['after_rows']=pg.rows(target);validate_rows(before_rows,row['after_rows'],count,started,ended)
                        row['handled_rows']=pg.handled(target)
                        validate_handled(row['after_rows'],row['handled_rows'],started,ended)
                        row['final_fingerprint']=pg.fingerprint(target)
                        if row['seed_fingerprint']!=row['final_fingerprint']:raise ValueError('immutable payload/trace/depth/identity changed')
                        row['breakers_after']=pg.breakers(target)
                        if row['breakers_before']!=row['breakers_after']:raise ValueError('normal cohort changed closed breaker columns')
                        row['provider_calls']=provider.snapshot()
                        if connected:validate_provider(row['provider_calls'],count)
                        elif row['provider_calls']:raise ValueError('connectionless workload called provider')
                        if process is not None and process.poll() is not None:raise ValueError('service died or restarted')
                        row['ok']=True
                        print(json.dumps({k:row[k] for k in ('workload','round','target','deliveries_per_second','tick_p95_ms','ok')}),flush=True)
                    finally:
                        for child in reversed(owned):base.spike.stop(child)
                        owned.clear();save()
    finally:
        for child in reversed(owned):base.spike.stop(child)
        if provider is not None:provider.close()
        if pg is not None:pg.stop()
        os.environ.clear();os.environ.update(previous)
        try:
            guard.validate(stamp,guard.source());result['end_source']=guard.source();result['fresh_at_end']=True
            result['end']=context()
        finally:save()
    result['complete']=all(r['ok'] for r in result['runs']) and len(result['runs'])==2*rounds*len(targets)
    save();print(output,flush=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    if not __debug__:parser.error('benchmark assertions must remain enabled')
    parser.add_argument('command',choices=('build','measure'));parser.add_argument('--label',required=True)
    parser.add_argument('--count',type=int,default=10000);parser.add_argument('--rounds',type=int,default=3)
    parser.add_argument('--targets',default=','.join(TARGETS));args=parser.parse_args()
    targets=tuple(args.targets.split(','))
    if not args.label.replace('-','').isalnum() or not 100<=args.count<=10000 or args.rounds<1 or not targets or len(set(targets))!=len(targets) or not set(targets)<=set(TARGETS):parser.error('invalid label/count/rounds/targets')
    build(args.label) if args.command=='build' else measure(args.label,args.count,args.rounds,targets)


if __name__=='__main__':main()
