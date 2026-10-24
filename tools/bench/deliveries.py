#!/usr/bin/env python3
"""10,000 durable database-only deliveries through actual production ticks.

Build and measure a label before changing the runtime, then repeat with a new
label. All raw attempts (including failures) are retained; no deployment.
"""
from __future__ import annotations
import argparse
import concurrent.futures
import resource
import hashlib
import http.client
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import time

import run as spike
import golden

ROOT = spike.ROOT
HERE = ROOT / 'tools/bench'
WORK = HERE / 'work/deliveries'
FIXTURE = HERE / 'fixtures/deliveries'
ENV = {**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_BUILD_JOBS': '4'}
TARGETS = ('native', 'workers', 'bun', 'node')


def checked(command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def sources():
    return golden.source_hashes()


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def build(label):
    directory = WORK / label
    directory.mkdir(parents=True, exist_ok=True)
    before = sources()
    clean_source()
    checked(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '--bin', 'jadpo'], cwd=ROOT, env=ENV)
    checked(['npm', 'ci', '--offline', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=HERE / 'typescript')
    checked(['npm', 'run', 'build'], cwd=HERE / 'typescript')
    spike.workerd()
    checked(['cargo', 'build', '--locked', '--release', '--manifest-path', str(HERE / 'load/Cargo.toml'), '--bin', 'pool'], cwd=ROOT, env=ENV)
    # Same authored source and its generated migration on both targets.
    for target in ('native', 'workers'):
        project = directory / target
        shutil.copytree(FIXTURE, project, dirs_exist_ok=True)
        application = (project / 'app.jadpo').read_text()
        if target == 'workers':
            application = application.replace('application {', '''application {
    rate_limits: { store: limits }
    deployables: { kiosk: { serves: [main] runs: [Trigger.subscribers, Trigger.schedules] target: Target.wasm } }''', 1)
        if target == 'workers':
            application = application.replace('    connections: {', '    connections: {\n        limits: { driver: Driver.cloudflare_rate_limits namespace: 1001 }', 1)
        (project / 'app.jadpo').write_text(application)
        for command in (['check'], ['build']):
            checked([str(spike.JADPO), *command], cwd=project, env=ENV)
    compiler = directory / 'compiler-jadpo'
    shutil.copy2(spike.JADPO, compiler)
    artifacts = [directory / 'native/build/target/app/app', spike.JADPO, compiler,
                 *sorted((directory / 'workers/build/target/kiosk').glob('*')),
                 HERE / 'typescript/build/deliveries.js', spike.workerd(), spike.POOL]
    artifacts = [path for path in artifacts if path.is_file()]
    if sources() != before:
        raise ValueError('source changed during build')
    stamp = {'source': checked(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True).stdout.strip(),
             'compiler_snapshot': {'original_path': str(spike.JADPO), 'path': str(compiler), 'sha256': digest(compiler)},
             'dirty': False, 'sources': before, 'artifacts': {str(p): digest(p) for p in artifacts},
             'flags': {name: ENV[name] for name in ('CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_BUILD_JOBS')},
             'profiles': {'native': 'jadpo build release', 'workers': 'jadpo build release, no wasm-opt',
                          'compiler': 'dev, excluded from timings', 'typescript': 'pinned tsc ES2022'},
             'versions': {name: checked([spike.tool(name), '--version'], capture_output=True, text=True).stdout.strip()
                          for name in ('rustc', 'node', 'bun', 'psql')}}
    (directory / 'build.json').write_text(json.dumps(stamp, indent=2) + '\n')


def clean_source():
    dirty = checked(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=ROOT, capture_output=True, text=True).stdout
    if dirty.strip():
        raise ValueError('commit source inputs before building or measuring')


def validate_build(stamp, current):
    if stamp['sources'] != current:
        raise ValueError('source changed: rebuild required')
    if not stamp.get('artifacts'):
        raise ValueError('artifacts missing')
    for path, expected in stamp['artifacts'].items():
        if digest(path) != expected:
            raise ValueError(f'artifact changed: {path}')


def fresh(label):
    stamp = json.loads((WORK / label / 'build.json').read_text())
    clean_source()
    if stamp['source'] != checked(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True).stdout.strip():
        raise ValueError('HEAD changed: rebuild required')
    validate_build(stamp, sources())
    return stamp


def validate_backlog(state, count):
    for field in ('receipts', 'unique', 'succeeded', 'handled', 'events', 'attempts'):
        if state.get(field) != count:
            raise ValueError(f'backlog {field}: expected {count}: {state}')
    for field in ('pending', 'unexpected', 'missing', 'invalid_id'):
        if state.get(field) != 0:
            raise ValueError(f'backlog {field}: expected zero: {state}')


class Database(golden.Database):
    def prepare(self, database):
        self.sql('postgres', f'CREATE DATABASE {database}')
        checked([str(spike.JADPO), 'migrate', 'apply', '--url', self.url(database)],
                cwd=self.project, stdout=subprocess.DEVNULL)

    def state(self, database):
        return json.loads(self.query(database, '''SELECT json_build_object(
          'receipts',(SELECT count(*) FROM receipt),
          'unique',(SELECT count(DISTINCT ticket) FROM receipt),
          'unexpected',(SELECT count(*) FROM receipt WHERE ticket !~ '^ticket-[0-9]{5}$'),
          'invalid_id',(SELECT count(*) FROM receipt WHERE substring(id::text,15,1)<>'7' OR substring(id::text,20,1) NOT IN ('8','9','a','b')),
          'succeeded',(SELECT count(*) FROM jadpo_deliveries WHERE status='succeeded'),
          'pending',(SELECT count(*) FROM jadpo_deliveries WHERE status<>'succeeded'),
          'attempts',(SELECT coalesce(sum(attempts),0) FROM jadpo_deliveries),
          'handled',(SELECT count(*) FROM jadpo_handled_deliveries),
          'events',(SELECT count(*) FROM jadpo_outbox),
          'missing',(SELECT count(*) FROM jadpo_deliveries d JOIN jadpo_outbox o ON o.id=d.outbox_id
             LEFT JOIN receipt r ON r.ticket=o.payload->>'ticket' WHERE d.status='succeeded' AND r.id IS NULL))'''))

    def fingerprint(self, database):
        # Canonical joined identities, original payload/encoding/time/trace/depth
        # and subscriber mapping. Delivery status/attempts are expected to change.
        return self.query(database, '''SELECT md5(string_agg(row_to_json(t)::text,E'\\n' ORDER BY t.id))
            FROM (SELECT d.id,d.outbox_id,d.subscriber,o.event,o.payload,o.emitted_at,o.encoding,o.traceparent,o.depth
            FROM jadpo_deliveries d JOIN jadpo_outbox o ON o.id=d.outbox_id) t''')

    def seed(self, database, template, count):
        self.sql(database, 'TRUNCATE receipt,jadpo_outbox,jadpo_deliveries,jadpo_handled_deliveries CASCADE')
        # Names/encoding are captured from a real emitted event, never guessed.
        event = template['event'].replace("'", "''")
        subscriber = template['subscriber'].replace("'", "''")
        encoding = 'NULL' if template['encoding'] is None else str(int(template['encoding']))
        self.sql(database, f'''INSERT INTO jadpo_outbox(id,event,payload,emitted_at,encoding,depth)
          SELECT md5('event-'||i)::uuid,'{event}',jsonb_build_object('ticket','ticket-'||lpad(i::text,5,'0')),
          clock_timestamp()-interval '1 minute',{encoding},0 FROM generate_series(1,{count}) i;
          INSERT INTO jadpo_deliveries(id,outbox_id,subscriber,status,attempts,next_attempt_at)
          SELECT md5('delivery-'||i)::uuid,md5('event-'||i)::uuid,'{subscriber}','pending',0,
          clock_timestamp()-interval '1 minute' FROM generate_series(1,{count}) i;''')
        state = self.state(database)
        assert state['events'] == state['pending'] == count and state['attempts'] == state['receipts'] == 0, state


def request(port, path, body=None, *, method=None):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=120)
    try:
        connection.request(method or ('POST' if body else 'GET'), path, body,
                           {'content-type': 'application/json'} if body else {})
        response = connection.getresponse()
        return response.status, response.read().decode()
    finally:
        connection.close()


def worker(directory, pg, logs, owned, *, extra_bindings=(), runtime_directory=None):
    pool_port = spike.free_port()
    with logs.open('ab') as log:
        pool = subprocess.Popen([str(spike.POOL), '--listen', f'127.0.0.1:{pool_port}',
            '--server', f'127.0.0.1:{pg.port}', '--user', 'jadpo', '--database', 'workers', '--size', '4'],
            stdout=log, stderr=log)
    owned.append(pool)
    spike.wait_for_port(pool_port)
    out = directory / 'workers/build/target/kiosk'
    wrangler = json.loads((out / 'wrangler.json').read_text())
    folder = runtime_directory or directory / 'workerd'
    folder.mkdir(parents=True, exist_ok=True)
    modules = []
    for path in sorted(out.iterdir()):
        if not path.is_file():
            continue
        shutil.copy(path, folder / path.name)
        kind = {'.js': 'esModule', '.mjs': 'esModule', '.wasm': 'wasm'}.get(path.suffix)
        if kind:
            modules.append(f'(name="{path.name}",{kind}=embed "{path.name}")')
    # First module must be the generated production entrypoint.
    modules.sort(key=lambda entry: (wrangler['main'] not in entry, entry))
    (folder / 'cron.mjs').write_text('''export default { async fetch(request,env) {
      const ended=await env.WORKER.scheduled({scheduledTime:new Date(),cron:"* * * * *"});
      return Response.json(ended);
    }};''')
    shutil.copy(ROOT / 'tools/workers/ratelimit-binding.mjs', folder / 'ratelimit-binding.mjs')
    bindings = ['(name="DATABASE_URL",hyperdrive=(designator="pg",database="workers",user="jadpo",password="",scheme="postgresql"))']
    bindings.extend(extra_bindings)
    for limit in wrangler.get('ratelimits', []):
        bindings.append(f'(name="{limit["name"]}",wrapped=(moduleName="bench:ratelimit",innerBindings=[(name="limit",json="{limit["simple"]["limit"]}"),(name="period",json="{limit["simple"]["period"]}"),(name="log",json="false")]))')
    port = spike.free_port()
    (folder / 'config.capnp').write_text(f'''using Workerd=import "/workerd/workerd.capnp";
      const config:Workerd.Config=(services=[(name="kiosk",worker = .kiosk),(name="cron",worker = .cron),
      (name="pg",external=(address="127.0.0.1:{pool_port}",tcp=())),
      (name="machine",network=(allow=["public","private","local"]))],
      sockets=[(name="cron",address="127.0.0.1:{port}",http=(),service="cron")],
      extensions=[(modules=[(name="bench:ratelimit",internal=true,esModule=embed "ratelimit-binding.mjs")])]);
      const kiosk:Workerd.Worker=(modules=[{','.join(modules)}],compatibilityDate="{wrangler['compatibility_date']}",
      bindings=[{','.join(bindings)}],globalOutbound="machine");
      const cron:Workerd.Worker=(modules=[(name="cron.mjs",esModule=embed "cron.mjs")],compatibilityDate="2026-10-07",
      compatibilityFlags=["service_binding_extra_handlers"],
      bindings=[(name="WORKER",service="kiosk")]);''')
    log = logs.open('ab')
    process = subprocess.Popen([str(spike.workerd()), 'serve', '--experimental', 'config.capnp'], cwd=folder, stdout=log, stderr=log)
    log.close()
    owned.append(process)
    # Readiness runs a real empty production tick, excluded from measurement.
    for _ in range(200):
        if process.poll() is not None:
            raise RuntimeError('workerd exited: ' + logs.read_text()[-4000:])
        try:
            if request(port, '/tick')[0] == 200:
                return process, pool, port
        except OSError:
            pass
        time.sleep(.025)
    spike.stop(process)
    raise RuntimeError('workerd readiness failed')


def typescript_service(target, directory, pg, logs, owned):
    port = spike.free_port()
    with logs.open('ab') as log:
        process = subprocess.Popen([spike.tool(target), str(HERE / 'typescript/build/deliveries.js'), '--serve'],
            env={**os.environ, 'DATABASE_URL': pg.url(target), 'PORT': str(port)}, stdout=log, stderr=log)
    owned.append(process)
    # Real HTTP tick, against unavailable work, matching workerd readiness.
    for _ in range(200):
        if process.poll() is not None:
            raise RuntimeError(f'{target} exited: ' + logs.read_text()[-4000:])
        try:
            if request(port, '/tick', method='POST')[0] == 200:
                return process, port
        except OSError:
            pass
        time.sleep(.025)
    raise RuntimeError(f'{target} readiness failed')


def calibrate(directory, pg):
    port = spike.free_port()
    log = (directory / 'calibration.log').open('ab')
    process = subprocess.Popen([str(directory / 'native/build/target/app/app')],
        env={**os.environ, 'DATABASE_URL': pg.url('native'), 'HOST': '127.0.0.1', 'PORT': str(port)},
        cwd=directory / 'native', stdout=log, stderr=log)
    log.close()
    try:
        for _ in range(200):
            if process.poll() is not None:
                raise RuntimeError('native calibration server exited')
            try:
                if request(port, '/health/ready')[0] == 200:
                    break
            except OSError:
                pass
            time.sleep(.025)
        status, body = request(port, '/api/queue', '{"ticket":"ticket-00000"}')
        if status != 202 or json.loads(body) != {'ticket': 'ticket-00000'}:
            raise ValueError(f'calibration response: {status} {body}')
        for _ in range(200):
            if pg.state('native')['receipts'] == 1:
                break
            time.sleep(.025)
        assert pg.state('native')['succeeded'] == 1
        return json.loads(pg.query('native', '''SELECT json_build_object('event',o.event,'subscriber',d.subscriber,
            'encoding',o.encoding) FROM jadpo_outbox o JOIN jadpo_deliveries d ON d.outbox_id=o.id'''))
    finally:
        spike.stop(process)


class TickError(RuntimeError):
    def __init__(self, response):
        self.response = response
        super().__init__(str(response))


def tick(target, directory, pg, port=None):
    response = {'target': target}
    if target == 'workers' or (target in ('bun', 'node') and port is not None):
        try:
            status, body = request(port, '/tick', method='POST')
            response.update(status=status, body=body)
            if status != 200 or (target == 'workers' and json.loads(body)['outcome'] != 'ok'):
                raise TickError(response)
            return response
        except Exception as error:
            response.update(error_type=type(error).__name__, error=str(error))
            raise TickError(response) from error
    command = ([str(directory / 'native/build/target/app/app'), 'tick'] if target == 'native' else
               [spike.tool(target), str(HERE / 'typescript/build/deliveries.js')])
    try:
        completed = subprocess.run(command, cwd=directory / 'native',
            env={**os.environ, 'DATABASE_URL': pg.url(target)}, capture_output=True, text=True, timeout=120)
        response.update(exit_code=completed.returncode, stdout=completed.stdout, stderr=completed.stderr)
    except subprocess.TimeoutExpired as error:
        def text(value):
            return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
        # subprocess.run kills and reaps its child before raising this.
        response.update(timeout=True, timeout_seconds=120, stdout=text(error.stdout), stderr=text(error.stderr))
        raise TickError(response) from error
    if completed.returncode:
        raise TickError(response)
    return response


def controls(target, directory, pg, template, port, reports):
    for name in ('malformed', 'rollback', 'live-lease', 'expired-lease', 'duplicate-sweeps'):
        report = {'name': name, 'ok': False}
        reports.append(report)
        pg.seed(target, template, 100)
        if name == 'malformed':
            pg.sql(target, "UPDATE jadpo_outbox SET payload='{\"ticket\":42}'::jsonb")
        elif name == 'rollback':
            # Receipt is inserted first, then this handled-marker refusal must
            # roll the entire transaction back, leaving neither durable write.
            pg.sql(target, 'ALTER TABLE jadpo_handled_deliveries ADD CONSTRAINT benchmark_refusal CHECK(false) NOT VALID')
        elif name == 'live-lease':
            pg.sql(target, "UPDATE jadpo_deliveries SET status='running',next_attempt_at=clock_timestamp()+interval '1 hour'")
        elif name == 'expired-lease':
            pg.sql(target, "UPDATE jadpo_deliveries SET status='running'")
        try:
            if name == 'duplicate-sweeps':
                with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                    answers = list(pool.map(lambda _: tick(target, directory, pg, port), range(2)))
            else:
                answers = [tick(target, directory, pg, port)]
            report['responses'] = answers
            state = pg.state(target)
            report['state'] = state
            if name in ('malformed', 'rollback', 'live-lease'):
                assert state['receipts'] == state['handled'] == state['succeeded'] == 0, state
                if name == 'malformed':
                    failed = int(pg.query(target, "SELECT count(*) FROM jadpo_deliveries WHERE status='failed'"))
                    assert failed == 100, (name, state, failed)
                assert state['attempts'] == (100 if name == 'rollback' else 0), state
                if name == 'rollback':
                    retrying = int(pg.query(target, "SELECT count(*) FROM jadpo_deliveries WHERE status='pending' AND last_failure->>'code'='unavailable'"))
                    assert retrying == 100, (state, retrying)
            else:
                assert state['receipts'] == state['unique'] == state['handled'] == state['succeeded'] == state['attempts'] == 100, state
            report['ok'] = True
        except Exception as error:
            report['error'] = repr(error)
            report['failure_response'] = getattr(error, 'response', None)
            raise
        finally:
            if name == 'rollback':
                pg.sql(target, 'ALTER TABLE jadpo_handled_deliveries DROP CONSTRAINT benchmark_refusal')
    return reports


def measure(args):
    directory = WORK / args.label
    stamp = fresh(args.label)
    output = directory / f'runs-{time.time_ns()}.json'
    result = {'build': stamp, 'host': {'platform': platform.platform(), 'cores': os.cpu_count(),
              'load': os.getloadavg(), 'classification': 'busy-host exploratory'},
              'protocol': {'count': args.count, 'rounds': args.rounds, 'sweep': 100,
                           'typescript_bound': 4, 'worker_pool_size': 4, 'native_tick': 'external process incl startup',
                           'workers_tick': 'persistent production scheduled through local HTTP service binding',
                           'typescript_tick': 'persistent local HTTP service, reused postgres-js pool4' if args.mode == 'warm' else 'external process incl startup',
                           'mode': args.mode, 'warmup': 'verified100-delivery cohort then reset exact measured seed' if args.mode == 'warm' else 'empty readiness only',
                           'native_warm_service': False}, 'runs': [], 'controls': {}}
    def save():
        output.write_text(json.dumps(result, indent=2) + '\n')
    result['host']['ambient_cpu_before'] = golden.host_cpu_ticks()
    time.sleep(5)
    result['host']['ambient_cpu_after'] = golden.host_cpu_ticks()
    result['host']['ambient_sample_seconds'] = 5
    save()
    spike.WORK = directory
    pg = Database.__new__(Database)
    owned = []
    process = helper = None
    port = None
    try:
        pg.__init__()
        pg.project = directory / 'native'
        for target in TARGETS:
            pg.prepare(target)
        template = calibrate(directory, pg)
        result['template'] = template
        for round_index in range(args.rounds):
            order = TARGETS[round_index % 4:] + TARGETS[:round_index % 4]
            for target in order:
                row = {'round': round_index + 1, 'target': target, 'ok': False, 'ticks': []}
                result['runs'].append(row)
                save()
                try:
                    # Empty database before workerd's excluded readiness tick.
                    pg.seed(target, template, args.count)
                    pg.sql(target, "UPDATE jadpo_deliveries SET next_attempt_at=clock_timestamp()+interval '1 hour'")
                    if target == 'workers':
                        process, helper, port = worker(directory, pg, directory / f'workers-{round_index}.log', owned)
                    if target in ('bun', 'node') and args.mode == 'warm':
                        process, port = typescript_service(target, directory, pg, directory / f'{target}-{round_index}.log', owned)
                    if target not in result['controls']:
                        reports = result['controls'].setdefault(target, [])
                        controls(target, directory, pg, template, port, reports)
                    if args.mode == 'warm':
                        pg.seed(target, template, 100)
                        row['warmup_response'] = tick(target, directory, pg, port)
                        row['warmup_state'] = pg.state(target)
                        validate_backlog(row['warmup_state'], 100)
                    pg.seed(target, template, args.count)
                    row['process_pid'] = process.pid if process else None
                    row['seed_fingerprint'] = pg.fingerprint(target)
                    cpu_before = golden.host_cpu_ticks()
                    worker_cpu_before = spike.cpu_s(process.pid) if process else None
                    helper_cpu_before = spike.cpu_s(helper.pid) if helper else None
                    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
                    before = time.perf_counter()
                    for tick_index in range((args.count + 99) // 100):
                        began = time.perf_counter()
                        response = tick(target, directory, pg, port)
                        if target in ('bun', 'node'):
                            answer = json.loads(response['body'] if port is not None else response['stdout'])
                            if process is not None:
                                assert answer['pid'] == process.pid, 'tick process identity changed'
                            assert answer['selected'] == answer['completed'] == min(100, args.count - tick_index * 100), answer
                        row.setdefault('responses', []).append(response)
                        row['ticks'].append(time.perf_counter() - began)
                    row['seconds'] = time.perf_counter() - before
                    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
                    row['service_cpu_seconds'] = spike.cpu_s(process.pid)-worker_cpu_before if process else None
                    row['worker_cpu_seconds'] = row['service_cpu_seconds'] if target == 'workers' else None
                    row['helper_cpu_seconds'] = spike.cpu_s(helper.pid)-helper_cpu_before if helper else None
                    row['server_cpu_seconds'] = ((row['service_cpu_seconds'] or 0) + (row['helper_cpu_seconds'] or 0)) if process else None
                    row['server_cpu_scope'] = 'persistent service plus separate workerd pool; PostgreSQL and Python excluded'
                    row['service_rss_mb_at_end'] = spike.rss_mb(process.pid) if process else None
                    row['host_cpu_before'] = cpu_before
                    row['host_cpu_after'] = golden.host_cpu_ticks()
                    row['load'] = os.getloadavg()
                    row['child_cpu_seconds'] = usage_after.ru_utime + usage_after.ru_stime - usage_before.ru_utime - usage_before.ru_stime
                    row['child_cpu_scope'] = 'external tick children only; persistent services, PostgreSQL and Python excluded'
                    row['deliveries_per_second'] = args.count / row['seconds']
                    row['state'] = pg.state(target)
                    state = row['state']
                    validate_backlog(state, args.count)
                    row['final_fingerprint'] = pg.fingerprint(target)
                    assert row['final_fingerprint'] == row['seed_fingerprint'], 'immutable seed changed'
                    exact = int(pg.query(target, f"SELECT count(*) FROM receipt r JOIN generate_series(1,{args.count}) i ON r.ticket='ticket-'||lpad(i::text,5,'0')"))
                    assert exact == args.count, 'receipt ticket set differs from seed'
                    # Repeating every succeeded delivery exercises production handled replay.
                    pg.sql(target, "UPDATE jadpo_deliveries SET status='pending',next_attempt_at=clock_timestamp()-interval '1 minute'")
                    # One replay sweep proves a selected cohort does not double its effects.
                    row['replay_response'] = tick(target, directory, pg, port)
                    replay = pg.state(target)
                    assert replay['receipts'] == replay['unique'] == args.count and replay['attempts'] == args.count + min(100, args.count), replay
                    row['replay'] = replay
                    row['ok'] = True
                except Exception as error:
                    row['error'] = repr(error)
                    row['failure_response'] = getattr(error, 'response', None)
                    raise
                finally:
                    for child in reversed(owned):
                        spike.stop(child)
                    owned.clear()
                    process = helper = None
                    port = None
                    save()
                print(json.dumps({key: row[key] for key in ('round', 'target', 'seconds', 'deliveries_per_second', 'ok')}), flush=True)
    finally:
        for child in reversed(owned):
            spike.stop(child)
        if hasattr(pg, 'data'):
            pg.stop()
        try:
            fresh(args.label)
            result['fresh_at_end'] = True
        except Exception as error:
            result['freshness_error'] = repr(error)
            raise
        finally:
            save()
    result['complete'] = all(row['ok'] for row in result['runs']) and len(result['runs']) == args.rounds*len(TARGETS)
    save()
    print(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    if not __debug__:
        parser.error('benchmark assertions must remain enabled; remove -O/PYTHONOPTIMIZE')
    parser.add_argument('command', choices=('build', 'measure'))
    parser.add_argument('--label', required=True)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--mode', choices=('warm', 'external'), default='warm')
    parser.add_argument('--count', type=int, default=10000)
    args = parser.parse_args()
    if not args.label.replace('-', '').isalnum() or args.count < 100 or args.count > 10000 or args.rounds < 1:
        parser.error('label must be alphanumeric/hyphen; count100..10000; rounds positive')
    build(args.label) if args.command == 'build' else measure(args)


if __name__ == '__main__':
    main()
