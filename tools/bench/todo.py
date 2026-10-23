#!/usr/bin/env python3
"""Validated local generic microbenchmark scaffold; see tools/bench/README.md.

Builds native and workerd deployables, then probes and exercises four focused routes on
native, workerd, Bun and Node, one server at a time. No account or deployment.
"""
import argparse
import hashlib
import http.client
import itertools
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import threading
import textwrap
import time
from pathlib import Path
from uuid import UUID

import run as spike
from contracts import validate_list, validate_measurement, validate_sample, validate_write_count, validate_failure

ROOT = spike.ROOT
BENCH = ROOT / 'tools/bench'
FIXTURE = BENCH / 'fixtures/todo'
TS = BENCH / 'typescript'
WORK = BENCH / 'work/todo'
KEY = 'local-benchmark-only-signing-key-0123456789'
SMALL = '11111111-1111-4111-8111-111111111111'
LARGE = '22222222-2222-4222-8222-222222222222'
REQUESTS = itertools.count(1)
ROUTES = ('read', 'write', 'signed-list', 'large')
# Reuse the spike's process/socket bridge tools, in a separate output tree.
spike.WORK = WORK
spike.EXTRA_ENV = {'SIGNING_KEY': KEY}
spike.EXTRA_BINDINGS = ['(name = "SIGNING_KEY", text = "' + KEY + '")']


def build():
    WORK.mkdir(parents=True, exist_ok=True)
    spike.run(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '--bin', 'jadpo'], cwd=ROOT)
    spike.run(['cargo', 'build', '--locked', '--release'], cwd=BENCH / 'load')
    spike.run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=TS)
    spike.run(['npm', 'run', 'build'], cwd=TS)
    spike.workerd()
    app = (FIXTURE / 'app.jadpo').read_text()
    for target in ('native', 'workers'):
        project = WORK / target
        if project.exists():
            for entry in project.iterdir():
                if entry.name != 'build':
                    if entry.is_dir() and not entry.is_symlink():
                        shutil.rmtree(entry)
                    else:
                        entry.unlink()
        shutil.copytree(FIXTURE, project, dirs_exist_ok=True, ignore=shutil.ignore_patterns('build'))
        target_app = app
        if target == 'workers':
            target_app = app.replace('connections: {', 'connections: { limits: { driver: Driver.cloudflare_rate_limits namespace: 1001 }', 1)
            target_app = re.sub(r'client_address:\s*\{[^}]+\}', 'client_address: { source: ClientAddressSource.cloudflare }', target_app)
            target_app = target_app.replace('application {', 'application { rate_limits: { store: limits }', 1)
            target_app = target_app.replace('application {', '''application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.wasm
        }
    }''', 1)
        (project / 'app.jadpo').write_text(target_app)
    # Validate both target configurations before paying for either build.
    for target in ('native', 'workers'):
        spike.run([str(spike.JADPO), 'check'], cwd=WORK/target)
    for target in ('native', 'workers'):
        print(f'  Building {target}', file=sys.stderr, flush=True)
        spike.run([str(spike.JADPO), 'build'], cwd=WORK/target)


class Database(spike.Postgres):
    def __init__(self):
        super().__init__()
        try:
            self.sql('postgres', "ALTER SYSTEM SET log_statement = 'all'")
            self.sql('postgres', 'SELECT pg_reload_conf()')
        except BaseException:
            self.stop()
            raise

    def sql(self, database, statement):
        spike.run([spike.tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(database)], input=statement, text=True, stdout=subprocess.DEVNULL)

    def query(self, database, sql):
        return spike.run([spike.tool('psql'), '-Atq', '-v', 'ON_ERROR_STOP=1', '-d', self.url(database), '-c', sql], capture_output=True, text=True).stdout.strip()

    def prepare(self, database):
        self.sql('postgres', f'CREATE DATABASE {database}')
        spike.run([str(spike.JADPO), 'migrate', 'apply', '--url', self.url(database)], cwd=WORK / 'native', stdout=subprocess.DEVNULL)

    def reset(self, database):
        self.sql(database, 'TRUNCATE todo, sample, "user", jadpo_sessions CASCADE')

    def logging(self, enabled):
        self.sql('postgres', "ALTER SYSTEM SET log_statement = '" + ('all' if enabled else 'none') + "'")
        self.sql('postgres', 'SELECT pg_reload_conf()')
        time.sleep(0.1)


class Server(spike.Server):
    def __init__(self, target, pg, logs):
        if target in ('native', 'workers'):
            try:
                super().__init__(target, pg, logs)
            except BaseException:
                if hasattr(self, 'process'):
                    spike.stop(self.process)
                for helper in getattr(self, 'helpers', []):
                    spike.stop(helper)
                raise
            return
        self.target, self.port = target, spike.free_port()
        self.helpers, self.memory_port = [], None
        self.header = 'x-forwarded-for: {ip}'
        self.log = open(logs / f'{target}.log', 'ab')
        command = [spike.tool('bun'), 'server.ts'] if target == 'bun' else [spike.tool('node'), 'build/server.js']
        self.spawned = time.monotonic()
        self.process = subprocess.Popen(command, cwd=TS, env={**os.environ, 'DATABASE_URL': pg.url(target), 'SIGNING_KEY': KEY, 'PORT': str(self.port)}, stdout=self.log, stderr=self.log, stdin=subprocess.DEVNULL)
        try:
            self.cold_ms, self.first_ms = self.first_answer()
            self.idle_rss_mb = spike.rss_mb(self.process.pid)
        except BaseException:
            self.stop()
            raise

    def stop(self):
        super().stop()
        if hasattr(self, 'log'):
            self.log.close()


def request(server, method, path, body=None, token=None):
    conn = http.client.HTTPConnection('127.0.0.1', server.port, timeout=30)
    n = next(REQUESTS) % 16384
    headers = {'content-type': 'application/json', server.header.split(':')[0]: f'172.{(n >> 16) & 255}.{(n >> 8) & 255}.{n & 255}'}
    if token:
        headers['authorization'] = 'Bearer ' + token
    try:
        conn.request(method, path, body=None if body is None else json.dumps(body), headers=headers)
        response = conn.getresponse()
        data = response.read()
        return response.status, json.loads(data), dict(response.getheaders())
    finally:
        conn.close()


def log_since(offset):
    with (WORK / 'pg.log').open('rb') as stream:
        stream.seek(offset)
        return stream.read().decode('utf-8')


def registered(server, handle):
    status, data, _ = request(server, 'POST', '/api/register', {'handle': handle})
    if status != 201 or set(data) != {'id', 'token'} or not isinstance(data['token'], str):
        raise ValueError('Registration did not issue a user and token')
    UUID(data['id'])
    return data


def prepare(server, pg, principals):
    """Same immutable seed before every cell; setup is outside all timings."""
    database = server.target
    pg.logging(True)
    pg.reset(database)
    alice, bob = registered(server, 'alice'), registered(server, 'bob')
    # The runtime initially issues an opaque token. Its first authenticated
    # request reads authority and returns the bounded signed token in a header.
    status, data, headers = request(server, 'GET', '/api/todos', token=alice['token'])
    if status != 200 or data != []:
        raise ValueError('Initial credential did not resolve the empty owner list')
    token = next((v for k, v in headers.items() if k.lower() == 'x-jadpo-token'), alice['token'])
    if server.target in ('native', 'workers') and token == alice['token']:
        raise ValueError('Signed-token renewal header was not captured')
    users = [(alice, token)]
    first_signed_at = time.monotonic()
    for i in range(1, principals):
        user = registered(server, f'owner-{i:04d}')
        status, empty, renewed = request(server, 'GET', '/api/todos', token=user['token'])
        if status != 200 or empty != []:
            raise ValueError('A pool credential was not admitted')
        signed = next((v for k, v in renewed.items() if k.lower() == 'x-jadpo-token'), user['token'])
        if server.target in ('native', 'workers') and signed == user['token']:
            raise ValueError('A pool credential was not renewed')
        users.append((user, signed))
    cases, seeded = [], []
    for index, (user, signed) in enumerate(users):
        expected = [{'id': f'10000000-{index:04x}-4000-8000-{i:012d}', 'title': f'task-{i:03d}'} for i in range(50)]
        seeded.extend(f"('{r['id']}', '{user['id']}', '{r['title']}')" for r in expected)
        cases.append({'headers': ['authorization: Bearer ' + signed], 'expected_json': expected})
    expected = cases[0]['expected_json']
    seeded.extend(f"('20000000-0000-4000-8000-{i:012d}', '{bob['id']}', 'other-{i:03d}')" for i in range(50))
    pg.sql(database, 'INSERT INTO todo (id, owner_id, title) VALUES ' + ','.join(seeded))
    pg.sql(database, f"INSERT INTO sample (id,title,payload) VALUES ('{SMALL}','small',repeat('x',100)),('{LARGE}','large',repeat('x',65536))")
    for credential, code in ((None, 'authentication_required'), ('forged.invalid.credential', 'invalid_credentials')):
        status, data, _ = request(server, 'GET', '/api/todos', token=credential)
        validate_failure(status, data, 401, code)
    # Expiring an opaque issued session must fail. A still-bounded signed
    # credential may remain valid until refresh, as area 8 explicitly decides.
    if server.target in ('native', 'workers'):
        pg.sql(database, f"UPDATE jadpo_sessions SET ends_at = now() - interval '1 second' WHERE principal_id = '{bob['id']}'")
        status, expired_body, _ = request(server, 'GET', '/api/todos', token=bob['token'])
        validate_failure(status, expired_body, 401, 'invalid_credentials')
    else:
        # The TypeScript baseline's idiomatic JOSE credential has the same
        # bounded lifetime. Produce an expired fixture outside the HTTP app.
        mint = "import { SignJWT } from 'jose'; const key=new TextEncoder().encode(process.env.SIGNING_KEY); console.log(await new SignJWT({handle:'bob'}).setProtectedHeader({alg:'HS256'}).setSubject(process.env.BENCH_SUBJECT).setIssuer('jadpo-bench').setAudience('jadpo-bench').setIssuedAt(Math.floor(Date.now()/1000)-301).setExpirationTime(Math.floor(Date.now()/1000)-1).sign(key));"
        expired = spike.run([spike.tool('node'), '--input-type=module', '-e', mint], cwd=TS, env={**os.environ, 'SIGNING_KEY': KEY, 'BENCH_SUBJECT': bob['id']}, capture_output=True, text=True).stdout.strip()
        status, expired_body, _ = request(server, 'GET', '/api/todos', token=expired)
        validate_failure(status, expired_body, 401, 'invalid_credentials')
    # Every owner must see exactly its own list before timing starts.
    for case in cases[1:]:
        credential = case['headers'][0].removeprefix('authorization: Bearer ')
        status, rows, _ = request(server, 'GET', '/api/todos', token=credential)
        if status != 200:
            raise ValueError('An owner pool list failed')
        validate_list(rows, case['expected_json'])
    # Inspect only this request's SQL, after renewal: one business SELECT,
    # zero credential/principal reads. Logging is off during measurement.
    offset = (WORK / 'pg.log').stat().st_size
    status, data, _ = request(server, 'GET', '/api/todos', token=token)
    validate_list(data, expected)
    if status != 200:
        raise ValueError('Signed list failed')
    time.sleep(0.05)
    logged = log_since(offset)
    statements = re.findall(r'(?:statement:|execute [^:]*:)\s+(SELECT[^\n]*)', logged, re.IGNORECASE)
    if len(statements) != 1 or any('jadpo_sessions' in s or 'FROM "user"' in s for s in statements):
        raise ValueError(f'Signed list must have one business query and no authority reads; observed {len(statements)} SELECTs')
    # A real write must commit to the caller, with the promised output;
    # collect statement shapes without recording parameter/verifier values.
    offset = (WORK / 'pg.log').stat().st_size
    status, created, _ = request(server, 'POST', '/api/todos', {'title': 'probe-write'}, token=token)
    if status != 201 or set(created) != {'id', 'title'} or created['title'] != 'probe-write' or UUID(created['id']).version != 7:
        raise ValueError('Write contract probe failed')
    time.sleep(0.05)
    write_log = log_since(offset)
    write_statements = re.findall(r'(?:statement:|execute [^:]*:)\s+([^\n]+)', write_log, re.IGNORECASE)
    saved = pg.query(database, f"SELECT owner_id::text || ':' || title FROM todo WHERE id='{created['id']}'")
    if saved != alice['id'] + ':probe-write':
        raise ValueError('A write did not persist to its signed owner')
    pg.sql(database, f"DELETE FROM todo WHERE id='{created['id']}'")
    for identity, title, payload in ((SMALL, 'small', 'x' * 100), (LARGE, 'large', 'x' * 65536)):
        status, data, _ = request(server, 'GET', '/api/samples/' + identity)
        if status != 200:
            raise ValueError('Sample read failed')
        validate_sample(data, identity, title, payload)
    status, data, _ = request(server, 'POST', '/api/todos', {'title': ''}, token=token)
    validate_failure(status, data, 422, 'invalid_input')
    if int(pg.query(database, 'SELECT count(*) FROM todo')) != 50 * principals + 50:
        raise ValueError('Setup or invalid input changed the seeded row count')
    pg.logging(False)
    return alice['id'], token, expected, cases, first_signed_at, {'signed_list_selects': len(statements), 'write_statements': write_statements}


def measure(server, pg, route, connections, warmup, duration, principals, address_pool):
    owner, token, expected, cases, signed_at, obligations = prepare(server, pg, principals if route in ('write', 'signed-list') else 1)
    before = int(pg.query(server.target, 'SELECT count(*) FROM todo'))
    if time.monotonic() - signed_at + warmup + duration + 45 > 290:
        raise ValueError('Credential setup took too long for the signed measurement window')
    command = [str(spike.LOAD), '--addr', f'127.0.0.1:{server.port}', '--connections', str(connections), '--warmup', str(warmup), '--duration', str(duration), '--address-pool', str(address_pool)]
    if route == 'write':
        command += ['--method', 'POST', '--path', '/api/todos', '--body', '{"title":"bench-write"}', '--expect', '201', '--expect-created-title', 'bench-write']
    else:
        if route == 'signed-list':
            body, path = expected, '/api/todos'
        else:
            large = route == 'large'
            identity, title, payload = (LARGE, 'large', 'x' * 65536) if large else (SMALL, 'small', 'x' * 100)
            body, path = {'id': identity, 'title': title, 'payload': payload}, '/api/samples/' + identity
        expected_file = WORK / 'expected.json'
        expected_file.write_text(json.dumps(body))
        command += ['--path', path, '--expect-json-file', str(expected_file)]
    command += ['--header', server.header]
    if route in ('write', 'signed-list'):
        if route == 'write':
            cases = [{'headers': case['headers']} for case in cases]
        cases_file = WORK / 'credential-cases.json'
        cases_file.write_text(json.dumps(cases))
        cases_file.chmod(0o600)
        command += ['--cases-file', str(cases_file)]
    peak = [spike.rss_mb(server.process.pid)]
    done = threading.Event()
    def sample():
        while not done.wait(0.25):
            peak[0] = max(peak[0], spike.rss_mb(server.process.pid))
    sampler = threading.Thread(target=sample)
    load1 = os.getloadavg()[0]
    load_started = time.monotonic()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    sampler.start()
    try:
        time.sleep(max(0, warmup - (time.monotonic() - load_started)))
        cpu_before = spike.cpu_s(server.process.pid)
        out, _ = process.communicate(timeout=warmup + duration + 45)
        cpu_after = spike.cpu_s(server.process.pid)
        if process.returncode:
            raise ValueError('Load generator failed; no timing result accepted')
        row = json.loads(out)
        validate_measurement(row)
        after = int(pg.query(server.target, 'SELECT count(*) FROM todo'))
        if route == 'write':
            successes = row['all_statuses'].get('201', 0)
            validate_write_count(before=before, after=after, successes=successes)
            wrong = int(pg.query(server.target, "SELECT count(*) FROM todo WHERE title NOT LIKE 'task-%' AND title NOT LIKE 'other-%' AND title <> 'bench-write'"))
            if wrong:
                raise ValueError('Writes persisted incorrect fields')
            outside_pool = int(pg.query(server.target, "SELECT count(*) FROM todo AS t JOIN \"user\" AS u ON u.id=t.owner_id WHERE t.title='bench-write' AND u.handle <> 'alice' AND u.handle NOT LIKE 'owner-%'"))
            if outside_pool:
                raise ValueError('Writes escaped the signed owner pool')
        elif before != after:
            raise ValueError('Read workload changed stored rows')
        row.update(target=server.target, route=route, cpu_ms_per_request=(cpu_after - cpu_before) * 1000 / row['requests'], cpu_cores=(cpu_after - cpu_before) / row['elapsed_s'], peak_rss_mb=peak[0], wasm_mb=server.memory_mb(), load1=load1, persisted_delta=after-before, load_cpu_cores=row['load_cpu_s']/row['load_wall_s'], obligations=obligations)
        return row
    finally:
        if process.poll() is None:
            spike.stop(process)
        done.set()
        sampler.join()


def metadata():
    def version(command):
        return spike.run(command, capture_output=True, text=True).stdout.strip()
    sources = {}
    for path in sorted([*[p for p in FIXTURE.rglob('*') if p.is_file() and 'build' not in p.relative_to(FIXTURE).parts], *TS.glob('*.ts'), TS/'package-lock.json', TS/'tsconfig.json', BENCH/'contracts.py', BENCH/'run.py', BENCH/'load/Cargo.lock', BENCH/'load/src/bin/load.rs', Path(__file__)]):
        sources[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {'schemaVersion': 1, 'revision': version(['git', 'rev-parse', 'HEAD']), 'dirty': bool(version(['git', 'status', '--porcelain'])), 'platform': platform.platform(), 'machine': platform.machine(), 'cpus': os.cpu_count(), 'rust': version(['rustc', '--version']), 'bun': version([spike.tool('bun'), '--version']), 'node': version([spike.tool('node'), '--version']), 'postgres': version([spike.tool('psql'), '--version']), 'workerd': version([str(spike.workerd()), '--version']), 'source_sha256': sources, 'artifact_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [WORK/'native/build/target/app/app', *sorted((WORK/'workers/build/target/kiosk').glob('*.wasm'))] if p.is_file()}, 'seed': {'todos_per_principal': 50, 'other_owner_todos': 50, 'principals': 'options.principals for signed-list/write, 1 for read/large', 'small_payload_bytes': 100, 'large_payload_bytes': 65536}, 'authentication': {'bounded_seconds': 300, 'signed_list_authority_queries': 0, 'signed_list_business_queries': 1}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--address-pool', type=int, default=16384, help='bounded client addresses; each keeps the normal120/minute limit')
    parser.add_argument('--principals', type=int, default=256, help='signed principal pool; each keeps the normal 600/minute limit')
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--duration', type=float, default=10)
    parser.add_argument('--warmup', type=float, default=2)
    parser.add_argument('--connections', default='1,16,64')
    parser.add_argument('--targets', default='native,workers,bun,node')
    parser.add_argument('--routes', default=','.join(ROUTES))
    parser.add_argument('--output', type=Path, default=WORK/'results.json')
    args = parser.parse_args()
    import math
    targets, routes = args.targets.split(','), args.routes.split(',')
    try:
        connections = [int(c) for c in args.connections.split(',')]
        if not 1 <= args.address_pool <= 16777216 or not 1 <= args.principals <= 4096 or args.rounds < 1 or not connections or min(connections) < 1:
            raise ValueError()
        if not math.isfinite(args.duration) or args.duration <= 0 or not math.isfinite(args.warmup) or args.warmup < 0 or args.duration + args.warmup + 45 > 240:
            raise ValueError()
        if not set(targets) <= {'native', 'workers', 'bun', 'node'} or not set(routes) <= set(ROUTES):
            raise ValueError()
    except ValueError:
        parser.error('Use valid targets/routes and positive counts; each cell must fit 240 seconds including its 45-second drain allowance')
    WORK.mkdir(parents=True, exist_ok=True)
    logs = WORK/'logs'
    logs.mkdir(exist_ok=True)
    print('Running focused todo benchmarks', file=sys.stderr, flush=True)
    if not args.skip_build:
        build()
    result = {'method': metadata(), 'options': {**vars(args), 'output': str(args.output)}, 'starts': [], 'rows': [], 'ok': False}
    pg = Database()
    try:
        for target in targets:
            pg.prepare(target)
        for round_ in range(args.rounds):
            for target in targets[round_ % len(targets):] + targets[:round_ % len(targets)]:
                server = Server(target, pg, logs)
                result['starts'].append({'round': round_+1, 'target': target, 'cold_ms': server.cold_ms, 'first_ms': server.first_ms, 'idle_rss_mb': server.idle_rss_mb})
                try:
                    for route in routes:
                        for conns in connections:
                            row = measure(server, pg, route, conns, args.warmup, args.duration, args.principals, args.address_pool)
                            row['round'] = round_+1
                            result['rows'].append(row)
                            print(f'  Round {round_+1}: {target} {route} x{conns} verified', file=sys.stderr, flush=True)
                finally:
                    server.stop()
        result['ok'] = True
    finally:
        pg.stop()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(f'Verified {len(result["rows"])} benchmark cells; wrote {args.output}')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError, TimeoutError) as error:
        print('error: Benchmark verification failed', file=sys.stderr)
        print(textwrap.fill(str(error), width=80, initial_indent='  reason: ', subsequent_indent='          ', break_long_words=False, break_on_hyphens=False), file=sys.stderr)
        print('  next: Review tools/bench/work/todo/logs.\n        Increase a pool if a 600/minute caller or 120/minute address limit\n        was reached.', file=sys.stderr)
        sys.exit(1)
