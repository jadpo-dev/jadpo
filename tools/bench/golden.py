#!/usr/bin/env python3
"""Compare the complete golden application through its production HTTP bindings.

All 44 cases on all four targets precede timing. Build freshness is mandatory.
Local PostgreSQL, controlled one-hop proxy, external scheduling; no live provider.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import http.client
import itertools
import json
import math
import os
from pathlib import Path
import platform
import secrets
import shutil
import statistics
import subprocess
import sys
import threading
import time
import uuid

import run as spike
from contracts import validate_failure, validate_measurement
from golden_contracts import TARGETS, validate_build, validate_conformance, validate_receipts, validate_state

ROOT = spike.ROOT
HERE = ROOT / 'tools/bench'
PROJECT = ROOT / 'examples/golden-todo'
TS = PROJECT / 'typescript'
WORK = HERE / 'work/golden'
STAMP = WORK / 'build.json'
SEED_TIME = '2026-01-15T12:00:00Z'
ROUTES = ('read', 'write', 'signed-list')
REQUESTS = itertools.count(1)
INDEX_SQL = '''DO $$ DECLARE entry record; BEGIN
  FOR entry IN SELECT i.relname AS indexname FROM pg_class i
    JOIN pg_index ix ON ix.indexrelid=i.oid WHERE ix.indrelid='public.todo'::regclass
    AND NOT EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conindid=i.oid)
  LOOP EXECUTE format('DROP INDEX %I', entry.indexname); END LOOP;
END $$;
CREATE INDEX todo_created_at_id_idx ON todo(created_at,id) WHERE deleted_at IS NULL;
CREATE INDEX todo_owner_id_created_at_id_idx ON todo(owner_id,created_at,id) WHERE deleted_at IS NULL;
CREATE INDEX todo_owner_id_idx ON todo(owner_id);
CREATE INDEX todo_status_created_at_id_idx ON todo(status,created_at,id) WHERE deleted_at IS NULL;
'''


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source_hashes():
    names = subprocess.run(['git', 'ls-files', '--cached', '--others', '--exclude-standard'],
                           cwd=ROOT, check=True, capture_output=True, text=True).stdout.splitlines()
    prefixes = ('crates/', 'examples/golden-todo/', 'tools/bench/', 'tools/golden-todo/', 'tools/workers/')
    return {name: sha(ROOT / name) for name in sorted(set(names))
            if (name.startswith(prefixes) or name in ('Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml'))
            and (ROOT / name).is_file()}


def fingerprint():
    return hashlib.sha256(json.dumps(source_hashes(), sort_keys=True).encode()).hexdigest()


def checked(arguments, **kwargs):
    return subprocess.run(arguments, check=True, **kwargs)


def contract_ids():
    return {case['id'] for case in json.loads((PROJECT / 'acceptance-v0.2.json').read_text())['cases']}


def conformance():
    """Own-source full contract, never reinterpret preview/sliced output as complete."""
    native = checked([sys.executable, str(ROOT / 'tools/golden-todo/run.py'), '--json'],
                     cwd=ROOT, capture_output=True, text=True)
    jadpo = json.loads(native.stdout)
    if (not jadpo.get('completeContract') or jadpo['sources'].get('preview')
            or not jadpo['sources'].get('compilerMatchesCheckout')):
        raise ValueError('Jadpo contract is incomplete or uses a preview compiler')
    typescript = checked([sys.executable, str(TS / 'adapter/run.py'), '--contract-source', str(ROOT)],
                         cwd=ROOT, capture_output=True, text=True)
    baseline = json.loads(typescript.stdout)
    if not baseline.get('completeContract') or baseline.get('preview', True):
        raise ValueError('TypeScript contract is incomplete or uses a preview source')
    reports = [{**row, 'target': 'workers' if row['target'] == 'workerd' else row['target']}
               for row in jadpo['cases']]
    reports += [{**row, 'target': row['runtime']} for row in baseline['cases']]
    validate_conformance(reports, contract_ids())
    return reports


def application_for(target):
    application = (PROJECT / 'app.jadpo').read_text().replace('application {', '''application {
    client_address: { source: ClientAddressSource.forwarded_for hops: 1 }''', 1)
    if target == 'workers':
        application = application.replace('ClientAddressSource.forwarded_for hops: 1', 'ClientAddressSource.cloudflare')
        application = application.replace('    connections: {', '''    connections: {
        limits: { driver: Driver.cloudflare_rate_limits namespace: 1001 }''', 1)
        application = application.replace('application {', '''application {
    rate_limits: { store: limits }
    deployables: { kiosk: { serves: [main] runs: [Trigger.subscribers, Trigger.schedules] target: Target.wasm } }''', 1)
    return application


def build():
    before = fingerprint()
    environment = {**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0'}
    checked(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '--bin', 'jadpo'], cwd=ROOT, env=environment)
    checked(['cargo', 'build', '--locked', '--release', '--manifest-path', str(HERE / 'load/Cargo.toml')], cwd=ROOT)
    checked(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=TS)
    checked(['npm', 'run', 'build'], cwd=TS)
    spike.workerd()
    cases = conformance()
    for target in ('native', 'workers'):
        directory = WORK / target
        if directory.exists():
            for child in directory.iterdir():
                if child.name not in ('build', '.jadpo'):
                    shutil.rmtree(child) if child.is_dir() else child.unlink()
        shutil.copytree(PROJECT, directory, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns('typescript', 'build', '.jadpo'))
        (directory / 'app.jadpo').write_text(application_for(target))
        checked([str(spike.JADPO), 'check'], cwd=directory)
        # jadpo build emits release native/Wasm deployables. No test-module timing.
        checked([str(spike.JADPO), 'build'], cwd=directory, env=environment)
    if before != fingerprint():
        raise ValueError('Source inputs changed during build or conformance; rebuild required')
    paths = [WORK / 'native/build/target/app/app', spike.LOAD, spike.POOL,
             *sorted((WORK / 'workers/build/target/kiosk').glob('*.wasm')),
             *sorted((WORK / 'workers/build/target/kiosk').glob('*.js')),
             *sorted((WORK / 'workers/build/target/kiosk').glob('*.mjs')),
             WORK / 'workers/build/target/kiosk/wrangler.json',
             *sorted((TS / 'build/src').rglob('*.js')),
             ROOT / 'target/jadpo-compiled/golden-todo/native/debug/golden-native-contract',
             spike.JADPO]
    if not any(path.suffix == '.wasm' for path in paths):
        raise ValueError('Production Wasm artifact is missing')
    stamp = {'inputs': before, 'sources': source_hashes(), 'artifacts': {str(path): sha(path) for path in paths},
             'build_environment': {name: value for name, value in environment.items()
                 if name.startswith('CARGO_PROFILE_') or name in ('CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS', 'RUSTFLAGS', 'RUSTUP_TOOLCHAIN')},
             'profiles': {'compiler': 'dev; not measured', 'native': 'jadpo build release',
                          'workers': 'jadpo build release; no wasm-opt', 'load': 'release'},
             'conformance': cases}
    STAMP.write_text(json.dumps(stamp, indent=2) + '\n')
    return stamp


class Database(spike.Postgres):
    def sql(self, database, statement):
        # Bulk seeds exceed command-line limits; SQL is data on psql's stdin.
        checked([spike.tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(database)],
                input=statement, text=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

    def query(self, database, statement):
        return checked([spike.tool('psql'), '-Atq', '-v', 'ON_ERROR_STOP=1', '-d', self.url(database),
                        '-c', statement], capture_output=True, text=True).stdout.strip()

    def prepare(self, database):
        self.sql('postgres', f'CREATE DATABASE {database}')
        if database in ('native', 'workers'):
            checked([str(spike.JADPO), 'migrate', 'apply', '--url', self.url(database)],
                    cwd=WORK / 'native', stdout=subprocess.DEVNULL)
        else:
            for path in sorted((TS / 'drizzle').glob('*.sql')):
                checked([spike.tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(database),
                         '-f', str(path)], stdout=subprocess.DEVNULL)
        self.sql(database, INDEX_SQL)

    def reset(self, database):
        self.sql(database, '''DO $$ DECLARE entry record; BEGIN
        FOR entry IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'jadpo_migrations'
        LOOP EXECUTE format('TRUNCATE TABLE %I CASCADE', entry.tablename); END LOOP; END $$''')

    def state(self, database):
        return json.loads(self.query(database,
            "SELECT coalesce(json_agg(t ORDER BY t.id),'[]'::json) FROM todo t"))


def configuration(database_url, key):
    return {'DATABASE_URL': database_url, 'SESSION_SIGNING_KEY': key,
            'BROWSER_ORIGIN': 'https://todo.example.test', 'OIDC_ISSUER': 'https://identity.example/tenant/',
            'OIDC_AUDIENCE': 'golden-todo', 'MAIL_API_KEY': 'local-fixture-unused-secret',
            'JWT_SIGNING_KEY': secrets.token_urlsafe(24)}


def credentials(target, environment, principals):
    """Sign fixture-owned real sessions before starting the measured HTTP process."""
    now = int(time.time() * 1000)
    owners = []
    for index in range(principals):
        owner = str(uuid.UUID(int=0x10000000000040008000000000000000 + index))
        row = str(uuid.uuid4())
        secret = 'jadpo_token_' + secrets.token_hex(32)
        owners.append({'id': owner, 'row': row, 'secret': secret, 'subject': f'owner-{index:04d}'})
    if target in ('native', 'workers'):
        binary = ROOT / 'target/jadpo-compiled/golden-todo/native/debug/golden-native-contract'
        with (WORK / 'fixture-signing.log').open('w') as log:
            process = subprocess.Popen([str(binary)], env={**os.environ, **environment}, stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, stderr=log, text=True, bufsize=1)
            def call(command):
                process.stdin.write(json.dumps(command) + '\n'); process.stdin.flush()
                line = process.stdout.readline()
                if not line:
                    raise ValueError('Fixture signing helper ended before issuing real bounded credentials')
                return json.loads(line)['result']
            try:
                for owner in owners:
                    owner['verifier'] = call({'op': 'digest', 'text': owner['secret']})['digest']
                    payload = claims(owner, now)
                    owner['token'] = owner['secret'] + '.' + call({'op': 'sign', 'payload': json.dumps(payload, separators=(',', ':'))})['signed']
            finally:
                process.stdin.close()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    spike.stop(process)
                process.stdout.close()
    else:
        script = '''import {digest,sign} from './build/src/auth.js';
let input='';for await(const part of process.stdin)input+=part;
const {owners,now}=JSON.parse(input);
for(const o of owners){o.verifier=digest(o.secret);o.token=o.secret+'.'+sign(JSON.stringify({enc:1,m:'api_token',r:o.row.replaceAll('-',''),v:o.verifier,p:o.id.replaceAll('-',''),s:'t'+o.subject,e:now+240000}),process.env.SESSION_SIGNING_KEY);}
console.log(JSON.stringify(owners));'''
        owners = json.loads(checked([spike.tool('node'), '--input-type=module', '-e', script], cwd=TS,
                                  input=json.dumps({'owners': owners, 'now': now}), text=True,
                                  capture_output=True, env={**os.environ, **environment}).stdout)
    return owners, now


def claims(owner, now):
    return {'enc': 1, 'm': 'api_token', 'r': uuid.UUID(owner['row']).hex, 'v': owner['verifier'],
            'p': uuid.UUID(owner['id']).hex, 's': 't' + owner['subject'], 'e': now + 240000}


def seed(pg, target, owners):
    pg.reset(target)
    users = [f"('{owner['id']}','{owner['subject']}','active','owner@example.test')" for owner in owners]
    other = '20000000-0000-4000-8000-000000000000'
    users.append(f"('{other}','other-owner','active','other@example.test')")
    pg.sql(target, 'INSERT INTO "user"(id,authentication_subject,status,email) VALUES ' + ','.join(users))
    pg.sql(target, 'INSERT INTO jadpo_sessions(id,principal_id,verifier,started_at,ends_at) VALUES ' + ','.join(
        f"('{owner['row']}','{owner['id']}','{owner['verifier']}',now()-interval '1 minute',now()+interval '1 day')" for owner in owners))
    values = []
    for index, owner in enumerate([*owners, {'id': other}]):
        for number in range(51):
            identity = str(uuid.UUID(int=0x30000000000040008000000000000000 + index * 100 + number))
            deleted = "'2026-01-16T00:00:00Z'" if number == 50 else 'NULL'
            values.append(f"('{identity}','{owner['id']}','task-{number:03d}','open',NULL,1,{deleted},'{SEED_TIME}','{SEED_TIME}')")
    pg.sql(target, 'INSERT INTO todo(id,owner_id,title,status,due_at,reminder_revision,deleted_at,created_at,updated_at) VALUES ' + ','.join(values))
    pg.sql(target, 'ANALYZE todo; ANALYZE "user"; ANALYZE jadpo_sessions')


class Server(spike.Server):
    def __init__(self, target, pg, environment, inspector=None):
        self.target, self.port = target, spike.free_port()
        self.helpers, self.memory_port = [], None
        self.header = 'x-forwarded-for: {ip}'
        self.log = (WORK / f'{target}.log').open('ab')
        try:
            if target == 'native':
                command = [str(WORK / 'native/build/target/app/app')]
                cwd = WORK / 'native'
            elif target in ('bun', 'node'):
                command = [spike.tool(target), str(HERE / 'golden_host.mjs')]
                cwd = TS
            else:
                pool_port = spike.free_port()
                pool = subprocess.Popen([str(spike.POOL), '--listen', f'127.0.0.1:{pool_port}',
                        '--server', f'127.0.0.1:{pg.port}', '--user', 'jadpo', '--database', target, '--size', '10'],
                        stdout=self.log, stderr=self.log, stdin=subprocess.DEVNULL)
                self.helpers.append(pool)
                spike.wait_for_port(pool_port)
                self.memory_port = spike.free_port()
                spike.EXTRA_BINDINGS = [f'(name = {json.dumps(key)}, text = {json.dumps(value)})'
                                        for key, value in environment.items() if key != 'DATABASE_URL']
                cwd = spike.workerd_config(WORK / 'workerd', self.port, f'127.0.0.1:{pool_port}',
                                           target, self.memory_port, trusted_loopback=True)
                command = [str(spike.workerd()), 'serve', 'config.capnp']
                if inspector:
                    command.append(f'--inspector-addr=127.0.0.1:{inspector}')
            self.spawned = time.monotonic()
            self.process = subprocess.Popen(command, cwd=cwd,
                env={**os.environ, **environment, 'PORT': str(self.port), 'HOST': '127.0.0.1'},
                stdout=self.log, stderr=self.log, stdin=subprocess.DEVNULL)
            self.cold_ms, self.first_ms = self.first_answer()
            self.idle_rss_mb = spike.rss_mb(self.process.pid)
        except BaseException:
            self.stop()
            raise

    def stop(self):
        if hasattr(self, 'process'):
            spike.stop(self.process)
        for helper in self.helpers:
            spike.stop(helper)
        self.log.close()

    def first_answer(self):
        while time.monotonic() < self.spawned + 120:
            if self.process.poll() is not None:
                raise ValueError(f'{self.target} stopped during startup; inspect {WORK / (self.target + ".log")}')
            connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=2)
            try:
                connection.connect()
                started = time.monotonic()
                connection.request('GET', '/health/ready', headers={'x-forwarded-for': '10.255.0.1'})
                response = connection.getresponse()
                body = json.loads(response.read())
                finished = time.monotonic()
                if response.status == 200:
                    if (body != {'status': 'ready', 'stores': {'main': 'ready'},
                                 'integrations': {'reminder_mail': 'unchecked'}}):
                        raise ValueError('Cold-start readiness returned an invalid response')
                    return (finished-self.spawned)*1000, (finished-started)*1000
            except (OSError, http.client.HTTPException):
                pass
            finally:
                connection.close()
            time.sleep(.01)
        raise TimeoutError(f'{self.target} did not become ready within 120 seconds')


def request(server, method, path, token=None, body=None):
    connection = http.client.HTTPConnection('127.0.0.1', server.port, timeout=30)
    address = next(REQUESTS) % 16384
    headers = {'x-forwarded-for': f'10.254.{address >> 8}.{address & 255}'}
    if token:
        headers['authorization'] = 'Bearer ' + token
    if body is not None:
        headers['content-type'] = 'application/json'
    try:
        connection.request(method, path, body=None if body is None else json.dumps(body), headers=headers)
        response = connection.getresponse()
        return response.status, json.loads(response.read())
    finally:
        connection.close()


def cases_for(server, owners, route):
    cases = []
    # Probe every signed identity, its foreign-owner exclusion and hidden row.
    for index, owner in enumerate(owners):
        rows = []
        for number in reversed(range(50)):
            identity = str(uuid.UUID(int=0x30000000000040008000000000000000 + index * 100 + number))
            rows.append({'id': identity, 'title': f'task-{number:03d}', 'status': 'open', 'due_at': None,
                         'created_at': SEED_TIME, 'updated_at': SEED_TIME})
        path = '/todos/' + rows[0]['id'] if route == 'read' else '/todos?page_size=100'
        expected = rows[0] if route == 'read' else {'items': rows, 'next': None}
        status, actual = request(server, 'GET', path, owner['token'])
        if status != 200 or actual != expected:
            raise ValueError('Signed response differs from exact owner/order/lifecycle fixture')
        case = {'headers': ['authorization: Bearer ' + owner['token']], 'owner': owner['id']}
        if route != 'write':
            case.update(path=path, expected_json=expected)
        cases.append(case)
    for token, code in ((None, 'authentication_required'), ('jadpo_token_' + '0' * 64 + '.invalid', 'invalid_credentials')):
        status, actual = request(server, 'GET', '/todos', token)
        validate_failure(status, actual, 401, code)
    status, actual = request(server, 'POST', '/todos', owners[0]['token'], {'title': ''})
    validate_failure(status, actual, 422, 'invalid_input')
    return cases


def host_cpu_ticks():
    """Whole-machine cumulative CPU counters; unavailable remains explicit."""
    if platform.system() == 'Linux':
        values = [int(value) for value in Path('/proc/stat').read_text().splitlines()[0].split()[1:9]]
        return sum(values)-values[3]-values[4], sum(values)
    if platform.system() == 'Darwin':
        import ctypes
        library = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
        library.mach_host_self.restype = ctypes.c_uint
        values = (ctypes.c_uint * 4)()
        size = ctypes.c_uint(4)
        if library.host_statistics(library.mach_host_self(), 3, ctypes.byref(values), ctypes.byref(size)) == 0:
            return int(values[0])+int(values[1])+int(values[3]), sum(values)
    return None


def load_output(process, timeout, row):
    def as_text(value):
        return value.decode('utf-8', errors='replace') if isinstance(value, bytes) else value or ''
    try:
        out, error = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired as failure:
        row.update(load_timeout=True, load_stdout=as_text(failure.stdout), load_stderr=as_text(failure.stderr))
        spike.stop(process)
        # Reap and finish reading its pipes after termination; communicate may
        # have captured more than TimeoutExpired exposed at the boundary.
        out, error = process.communicate(timeout=5)
        row.update(load_stdout=as_text(out) or row['load_stdout'],
                   load_stderr=as_text(error) or row['load_stderr'], load_exit_code=process.returncode)
        raise ValueError('Load generator timed out; raw output retained') from failure
    if process.returncode:
        row.update(load_stdout=as_text(out), load_stderr=as_text(error), load_exit_code=process.returncode)
        raise ValueError('Load generator failed; raw output retained')
    return out


def measure(server, pg, owners, cases, route, connections, warmup, duration, row, observer=None):
    before = pg.state(server.target)
    filename = WORK / 'credential-cases.json'
    filename.write_text(json.dumps(cases)); filename.chmod(0o600)
    command = [str(spike.LOAD), '--addr', f'127.0.0.1:{server.port}', '--connections', str(connections),
               '--warmup', str(warmup), '--duration', str(duration), '--address-pool', '16384',
               '--header', server.header, '--cases-file', str(filename)]
    if route == 'write':
        command += ['--method', 'POST', '--path', '/todos', '--body', '{"title":"bench-write"}',
                    '--expect', '201', '--expect-created-todo-title', 'bench-write']
    else:
        command += ['--path', '/todos']
    peak = [spike.rss_mb(server.process.pid)]
    done = threading.Event()
    def sample():
        while not done.wait(0.25):
            peak[0] = max(peak[0], spike.rss_mb(server.process.pid))
    sampler = threading.Thread(target=sample)
    if observer:
        observer.start()
    start = time.monotonic()
    load_average = os.getloadavg()[0]
    row['load_started_unix'] = time.time()
    try:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except BaseException:
        if observer:
            observer.close()
        raise
    sampler.start()
    try:
        # CPU spans measurement through completed slow tails. Warmup is excluded.
        time.sleep(max(0, warmup - (time.monotonic() - start)))
        cpu_before = spike.cpu_s(server.process.pid)
        helper_before = sum(spike.cpu_s(helper.pid) for helper in server.helpers)
        host_before = host_cpu_ticks()
        out = load_output(process, warmup + duration + 45, row)
        row['load_completed_unix'] = time.time()
        cpu_after = spike.cpu_s(server.process.pid)
        helper_after = sum(spike.cpu_s(helper.pid) for helper in server.helpers)
        host_after = host_cpu_ticks()
        if observer:
            observer.stop()
        row.update(json.loads(out))
        validate_measurement(row)
        after = pg.state(server.target)
        successes = row['all_statuses'].get('201', 0) if route == 'write' else 0
        validate_state(before, after, successes, {owner['id'] for owner in owners})
        receipts = row['created_rows']
        validate_receipts(receipts, after, successes)
        row.update(target=server.target, route=route, cpu_ms_per_request=(cpu_after-cpu_before)*1000/row['requests'],
                   cpu_cores=(cpu_after-cpu_before)/row['elapsed_s'], sampled_peak_rss_mb=peak[0],
                   wasm_mb=server.memory_mb(), load1=load_average,
                   load1_after=os.getloadavg()[0],
                   host_cpu_percent=(100*(host_after[0]-host_before[0])/(host_after[1]-host_before[1])
                       if host_before and host_after and host_after[1] > host_before[1] else None),
                   load_cpu_cores=row['load_cpu_s']/row['load_wall_s'], persisted_delta=len(after)-len(before),
                   complete_write_receipts_verified=len(receipts),
                   helper_cpu_ms_per_request=(helper_after-helper_before)*1000/row['requests'])
        validate_measurement(row)
        row.pop('created_rows')
        row['verified'] = True
        return row
    finally:
        if process.poll() is None:
            spike.stop(process)
        if observer:
            observer.close()
        done.set(); sampler.join()
        filename.unlink(missing_ok=True)


def metadata(stamp):
    def version(command):
        return checked(command, capture_output=True, text=True).stdout.strip()
    machine = {'processor': platform.processor(), 'logical_cpus': os.cpu_count()}
    if platform.system() == 'Darwin':
        for name in ('machdep.cpu.brand_string', 'hw.memsize', 'hw.physicalcpu', 'hw.perflevel0.physicalcpu'):
            answer = subprocess.run(['sysctl', '-n', name], capture_output=True, text=True)
            if answer.returncode == 0:
                machine[name] = answer.stdout.strip()
    return {'revision': version(['git', 'rev-parse', 'HEAD']),
            'dirty': bool(version(['git', 'status', '--porcelain'])),
            'source_sha256': stamp['sources'], 'artifact_sha256': stamp['artifacts'],
            'build_environment': stamp['build_environment'], 'profiles': stamp['profiles'],
            'platform': platform.platform(), 'architecture': platform.machine(), 'machine': machine,
            'rust': version(['rustc', '--version']), 'node': version([spike.tool('node'), '--version']),
            'bun': version([spike.tool('bun'), '--version']), 'postgres': version([spike.tool('psql'), '--version']),
            'workerd': version([str(spike.workerd()), '--version']),
            'business_index_sql': INDEX_SQL, 'business_index_sha256': hashlib.sha256(INDEX_SQL.encode()).hexdigest(),
            'database': {'synchronous_commit': 'off', 'max_connections': 400, 'shared_buffers': '256MB'},
            'transport': {'native_typescript': 'loopback one trusted proxy; forwarded_for hops 1; TS checks actual loopback peer',
                          'workers': 'Cloudflare source; loopback workerd socket overwrites CF-Connecting-IP with 127.0.0.1'},
            'scheduling': 'external on all targets; no jobs inside request timings',
            'pools': {'native': '10 open/8 idle per driver per server worker thread',
                      'workers': 'one isolate; local transaction pool cap 10',
                      'typescript': '5 general plus 5 exclusive transaction clients'},
            'auth': {'lifetime_seconds': 240, 'caller_limit': '600/min', 'address_limit': '120/min',
                     'address_pool': 16384, 'authority_reads_in_timed_path': 0}}


def summarize(rows):
    summary = []
    for target in TARGETS:
        for route in ROUTES:
            for connections in sorted({row['connections'] for row in rows}):
                group = [row for row in rows if (row['target'], row['route'], row['connections']) == (target, route, connections)]
                if not group:
                    continue
                summary.append({'target': target, 'route': route, 'connections': connections,
                    'rounds': len(group), **{field: {'median': statistics.median(row[field] for row in group),
                    'min': min(row[field] for row in group), 'max': max(row[field] for row in group)}
                    for field in ('rps', 'p95_us', 'cpu_ms_per_request', 'sampled_peak_rss_mb')}})
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--prepare-only', action='store_true', help='build and run all 176 contract checks without timing')
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--connections', default='1,16,64')
    parser.add_argument('--duration', type=float, default=10)
    parser.add_argument('--warmup', type=float, default=2)
    parser.add_argument('--principals', type=int, default=256)
    parser.add_argument('--targets', default=','.join(TARGETS))
    parser.add_argument('--routes', default=','.join(ROUTES))
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    targets, routes = args.targets.split(','), args.routes.split(',')
    try:
        connections = [int(value) for value in args.connections.split(',')]
        if (not targets or len(targets) != len(set(targets)) or set(targets)-set(TARGETS)
                or not routes or len(routes) != len(set(routes)) or set(routes)-set(ROUTES)
                or min(connections) < 1 or len(connections) != len(set(connections))
                or args.rounds < 1 or not 1 <= args.principals <= 4096
                or not math.isfinite(args.duration) or args.duration <= 0
                or not math.isfinite(args.warmup) or args.warmup < 0 or args.duration+args.warmup > 120):
            raise ValueError()
    except ValueError:
        parser.error('Use supported unique targets/routes, positive counts and a cell of at most 120 seconds')
    if args.output is None:
        args.output = WORK / ('results-' + dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S')
                              + '-' + secrets.token_hex(2) + '.json')
    if args.output.exists():
        parser.error('Output already exists; choose a new path to retain earlier and failed runs')
    WORK.mkdir(parents=True, exist_ok=True)
    spike.WORK = WORK
    stamp = json.loads(STAMP.read_text()) if args.skip_build else build()
    validate_build(stamp, fingerprint())
    validate_conformance(stamp['conformance'], contract_ids())
    if args.prepare_only:
        print('passed: golden todo · 176/176 contract cases · production artifacts ready')
        return
    result = {'schemaVersion': 1, 'method': metadata(stamp), 'conformance': stamp['conformance'],
              'options': {**vars(args), 'output': str(args.output)}, 'starts': [], 'rows': [], 'ok': False}
    database = Database()
    try:
        for target in targets:
            database.prepare(target)
        result['physical_indexes'] = {target: json.loads(database.query(target,
            "SELECT json_agg(i ORDER BY i.indexname) FROM (SELECT indexname,indexdef FROM pg_indexes WHERE schemaname='public' AND tablename='todo') i"))
            for target in targets}
        for round_ in range(args.rounds):
            order = targets[round_ % len(targets):] + targets[:round_ % len(targets)]
            for target in order:
                for route in routes:
                    for concurrency in connections:
                        # A new server per cell also resets genuine limit state;
                        # credentials and SQL fixture setup precede cold start.
                        environment = configuration(database.url(target), secrets.token_urlsafe(32))
                        owners, signed_at = credentials(target, environment, args.principals)
                        seed(database, target, owners)
                        server = Server(target, database, environment)
                        try:
                            cases = cases_for(server, owners, route)
                            if time.time()*1000-signed_at+(args.warmup+args.duration+45)*1000 >= 230000:
                                raise ValueError('Fixture setup exhausted the bounded credential measurement window')
                            result['starts'].append({'target': target, 'round': round_+1, 'route': route,
                                'connections': concurrency, 'cold_ms': server.cold_ms, 'first_ms': server.first_ms,
                                'idle_rss_mb': server.idle_rss_mb})
                            row = {'round': round_+1, 'target': target, 'route': route,
                                   'connections': concurrency, 'verified': False}
                            result['rows'].append(row)
                            try:
                                measure(server, database, owners, cases, route, concurrency, args.warmup, args.duration, row)
                            except Exception as error:
                                row['failure'] = str(error)
                                raise
                            print(f'  verified: round {round_+1} {target} {route} x{concurrency}', file=sys.stderr, flush=True)
                        finally:
                            server.stop()
        validate_build(stamp, fingerprint())
        result.update(ok=True, summary=summarize(result['rows']))
    except BaseException as error:
        result['failure'] = str(error)
        raise
    finally:
        database.stop()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(f'passed: golden todo · {len(result["rows"])} verified cells · {args.output}')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, TimeoutError) as error:
        print(f'error: Golden comparison refused\n  reason: {error}\n  next: python3 tools/bench/golden.py', file=sys.stderr)
        raise SystemExit(1)
