#!/usr/bin/env python3
"""The performance spike (roadmap chunk 8.3; tools/bench/README.md).

Teaching example 23's booking route, `POST /repairs`, and its readiness
check, `GET /health/ready`, served four ways on this machine against one
local PostgreSQL, each with a database of its own:

  native          `jadpo build`'s native binary
  workers         `jadpo build`'s Workers module under the pinned workerd,
                  its Hyperdrive binding given to `pool`, a stand-in for
                  Hyperdrive's pooling
  workers-direct  the same, its Hyperdrive binding given straight to
                  PostgreSQL, as workerd and `wrangler dev` do: a new
                  connection, and backend, for every request
  bun             the same route hand-written in Bun and TypeScript
                  (tools/bench/bun/server.ts)

Each round starts every server afresh (its cold start), then drives each
route at each concurrency with `load` (tools/bench/load), sampling the
server's CPU time and memory. Rounds rotate the order; the summary gives
each measure's median over the rounds and its range.

  python3 tools/bench/run.py                    # build, then 3 rounds
  python3 tools/bench/run.py --rounds 5 --duration 15
  python3 tools/bench/run.py --skip-build --targets native,bun
  python3 tools/bench/run.py --skip-build --soak 600   # memory over a long run

Everything it builds, installs and starts stays under tools/bench/work/
(and tools/workers/node_modules for workerd); it stops what it starts.
"""

import argparse
import http.client
import json
import os
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BENCH = ROOT / 'tools' / 'bench'
WORK = BENCH / 'work'
EXAMPLE = ROOT / 'examples' / '23-repair-cafe'
JADPO = ROOT / 'target' / 'debug' / 'jadpo'
LOAD = BENCH / 'load' / 'target' / 'release' / 'load'
POOL = BENCH / 'load' / 'target' / 'release' / 'pool'
WORKERD = ROOT / 'tools' / 'workers' / 'node_modules' / '@cloudflare'
EXTRA_ENV = {}
EXTRA_BINDINGS = []
BOOKING = '{"item":"bike","symptom":"the chain slips on every hill"}'

NATIVE_APP = """application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    events: {
        store: main
    }
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 1
    }
}
"""

WORKERS_APP = """application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    events: {
        store: main
    }
    rate_limits: {
        store: limits
    }
    deployables: {
        kiosk: {
            serves: [main]
            runs: [Trigger.subscribers, Trigger.schedules]
            target: Target.wasm
        }
    }
}
"""

ROUTES = {
    'book': ('POST', '/repairs', BOOKING, '201'),
    'ready': ('GET', '/health/ready', '', '200'),
}


def run(command, **kwargs):
    kwargs.setdefault('check', True)
    return subprocess.run(command, **kwargs)


def free_port() -> int:
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def tool(name: str) -> str:
    found = shutil.which(name)
    if found:
        return found
    for candidate in [Path('/Applications/Postgres.app/Contents/Versions/16/bin') / name,
                      Path.home() / '.bun' / 'bin' / name]:
        if candidate.is_file():
            return str(candidate)
    sys.exit(f'{name} was not found on PATH')


def workerd() -> Path:
    for entry in sorted(WORKERD.glob('workerd-*')):
        binary = entry / 'bin' / 'workerd'
        if binary.is_file():
            return binary
    run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=ROOT / 'tools' / 'workers')
    return workerd()


def build():
    env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0'}
    run(['cargo', 'build', '-p', 'jadpo-cli', '--bin', 'jadpo'], cwd=ROOT, env=env)
    run(['cargo', 'build', '--release'], cwd=BENCH / 'load')
    run([tool('bun'), 'install', '--frozen-lockfile'], cwd=BENCH / 'bun')
    workerd()
    for name, app in [('native', NATIVE_APP), ('workers', WORKERS_APP)]:
        project = WORK / name
        if project.exists():
            for entry in project.iterdir():
                if entry.name != 'build':
                    shutil.rmtree(entry) if entry.is_dir() else entry.unlink()
        shutil.copytree(EXAMPLE, project, dirs_exist_ok=True, ignore=shutil.ignore_patterns('build'))
        (project / 'app.jadpo').write_text(app)
        print(f'building {name} with jadpo build', flush=True)
        run([str(JADPO), 'build'], cwd=project, stdout=subprocess.DEVNULL)


class Postgres:
    """A disposable server: trusted on 127.0.0.1, no Unix socket,
    `synchronous_commit` off so commits don't wait for the disk."""

    def __init__(self, diagnostics=False):
        self.data = WORK / 'pg'
        shutil.rmtree(self.data, ignore_errors=True)
        run([tool('initdb'), '-D', str(self.data), '-U', 'jadpo', '--auth=trust', '-E', 'UTF8'],
            stdout=subprocess.DEVNULL)
        self.port = free_port()
        options = (f'-p {self.port} -c unix_socket_directories= -c listen_addresses=127.0.0.1 '
                   '-c max_connections=400 -c synchronous_commit=off -c shared_buffers=256MB')
        if diagnostics:
            # Process-lifetime diagnostics only; never persist server settings.
            options += ' -c log_duration=on -c log_parameter_max_length=0 -c log_parameter_max_length_on_error=0'
        run([tool('pg_ctl'), '-D', str(self.data), '-l', str(WORK / 'pg.log'), '-o', options, '-w', 'start'],
            stdout=subprocess.DEVNULL)

    def url(self, database: str) -> str:
        return f'postgresql://jadpo@127.0.0.1:{self.port}/{database}'

    def sql(self, database: str, statement: str):
        run([tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(database), '-c', statement],
            stdout=subprocess.DEVNULL)

    def prepare(self, database: str):
        self.sql('postgres', f'CREATE DATABASE {database}')
        run([str(JADPO), 'migrate', 'apply', '--url', self.url(database)], cwd=WORK / 'native',
            stdout=subprocess.DEVNULL)
        self.sql(database, "INSERT INTO bench (id, item, fixer) VALUES "
                           "('11111111-1111-4111-8111-111111111111', 'bike', 'Ada'), "
                           "('22222222-2222-4222-8222-222222222222', 'lamp', 'Grace')")

    def reset(self, database: str):
        self.sql(database, 'TRUNCATE repair, thank_you_note, jadpo_outbox, jadpo_deliveries, '
                           'jadpo_handled_deliveries CASCADE')

    def stop(self):
        run([tool('pg_ctl'), '-D', str(self.data), '-m', 'fast', 'stop'], stdout=subprocess.DEVNULL,
            check=False)


def stop(process: subprocess.Popen):
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
        try:
            process.wait(15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


# The kiosk's own main module with one entrypoint more, which only the
# harness's `memory` Worker reaches: the size of the module's linear memory,
# from the exports wasm-bindgen's `initSync` hands back once the module is
# instantiated. Requests reach the kiosk's own handlers unchanged.
MEMORY_MAIN = """export {{ default, JadpoHandOn }} from "./{main}";
import {{ WorkerEntrypoint }} from "cloudflare:workers";
import {{ initSync }} from "./{glue}";

export class JadpoMemory extends WorkerEntrypoint {{
  bytes() {{
    return initSync().memory.buffer.byteLength;
  }}
}}
"""

# A Worker of the harness's own on a port of its own, answering with the
# kiosk's linear memory in bytes.
MEMORY_WORKER = """export default {
  async fetch(request, env) {
    return Response.json({ bytes: await env.KIOSK.bytes() });
  },
};
"""


LOOPBACK_WORKER_HTTP = '(injectRequestHeaders = [(name = "CF-Connecting-IP", value = "127.0.0.1")])'


def workerd_config(folder: Path, port: int, database_host: str, database: str, memory_port: int,
                   trusted_loopback: bool = False) -> Path:
    """The test harness's config (crates/jadpo-cli/tests/workers.rs) for the
    built kiosk: its modules, Hyperdrive binding and stand-in rate-limiting
    binding, without the cron Worker, and the harness's `memory` Worker on
    `memory_port`."""
    out = WORK / 'workers' / 'build' / 'target' / 'kiosk'
    wrangler = json.loads((out / 'wrangler.json').read_text())
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir(parents=True)
    # wasm-bindgen's JavaScript, which the shim instantiates the module
    # through: the one `.js` beside it.
    [glue] = [path.name for path in out.iterdir() if path.suffix == '.js']
    (folder / 'bench-main.mjs').write_text(MEMORY_MAIN.format(main=wrangler['main'], glue=glue))
    (folder / 'bench-memory.mjs').write_text(MEMORY_WORKER)
    modules = ['(name = "bench-main.mjs", esModule = embed "bench-main.mjs")']
    for path in sorted(out.iterdir()):
        if path.is_file():
            shutil.copy(path, folder / path.name)
            kind = {'.js': 'esModule', '.mjs': 'esModule', '.wasm': 'wasm'}.get(path.suffix)
            if kind:
                modules.append(f'(name = "{path.name}", {kind} = embed "{path.name}")')
    shutil.copy(ROOT / 'tools' / 'workers' / 'ratelimit-binding.mjs', folder / 'ratelimit-binding.mjs')
    bindings = ['(name = "DATABASE_URL", hyperdrive = (designator = "hyperdrive", '
                f'database = "{database}", user = "jadpo", password = "", scheme = "postgresql"))']
    bindings.extend(EXTRA_BINDINGS)
    for limit in wrangler.get('ratelimits', []):
        bindings.append(
            f'(name = "{limit["name"]}", wrapped = (moduleName = "bench:ratelimit", innerBindings = ['
            f'(name = "limit", json = "{limit["simple"]["limit"]}"), '
            f'(name = "period", json = "{limit["simple"]["period"]}"), (name = "log", json = "false")]))')
    http = LOOPBACK_WORKER_HTTP if trusted_loopback else '()'
    config = f'''using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [
    (name = "kiosk", worker = .kiosk),
    (name = "memory", worker = .memory),
    (name = "machine", network = (allow = ["public", "private", "local"])),
    (name = "hyperdrive", external = (address = "{database_host}", tcp = ())),
  ],
  sockets = [
    (name = "http", address = "127.0.0.1:{port}", http = {http}, service = "kiosk"),
    (name = "memory", address = "127.0.0.1:{memory_port}", http = (), service = "memory"),
  ],
  extensions = [(modules = [(name = "bench:ratelimit", internal = true, esModule = embed "ratelimit-binding.mjs")])],
);

const kiosk :Workerd.Worker = (
  modules = [{", ".join(modules)}],
  compatibilityDate = "{wrangler["compatibility_date"]}",
  bindings = [{", ".join(bindings)}],
  globalOutbound = "machine",
);

const memory :Workerd.Worker = (
  modules = [(name = "bench-memory.mjs", esModule = embed "bench-memory.mjs")],
  compatibilityDate = "{wrangler["compatibility_date"]}",
  bindings = [(name = "KIOSK", service = (name = "kiosk", entrypoint = "JadpoMemory"))],
);
'''
    (folder / 'config.capnp').write_text(config)
    return folder


class Server:
    """One target, started and answering its readiness check."""

    def __init__(self, target: str, pg: Postgres, logs: Path, inspector: int | None = None):
        self.target = target
        self.port = free_port()
        self.helpers = []
        # Where the harness reads a Worker's linear memory (`memory_mb`).
        self.memory_port = None
        log = open(logs / f'{target}.log', 'ab')
        if target == 'native':
            command = [str(WORK / 'native' / 'build' / 'target' / 'app' / 'app')]
            env = {'DATABASE_URL': pg.url('native'), 'HOST': '127.0.0.1', 'PORT': str(self.port)}
            cwd = WORK / 'native'
            self.header = 'x-forwarded-for: {ip}'
        elif target == 'bun':
            command = [tool('bun'), 'server.ts']
            env = {'DATABASE_URL': pg.url('bun'), 'HOST': '127.0.0.1', 'PORT': str(self.port),
                   'BUN_POOL': '20', 'PATH': os.environ['PATH']}
            cwd = BENCH / 'bun'
            self.header = 'x-forwarded-for: {ip}'
        else:
            database_host = f'127.0.0.1:{pg.port}'
            if target == 'workers':
                pool_port = free_port()
                pool = subprocess.Popen(
                    [str(POOL), '--listen', f'127.0.0.1:{pool_port}', '--server', database_host,
                     '--user', 'jadpo', '--database', 'workers', '--size', '20'],
                    stdout=log, stderr=log, stdin=subprocess.DEVNULL)
                self.helpers.append(pool)
                wait_for_port(pool_port)
                database_host = f'127.0.0.1:{pool_port}'
            self.memory_port = free_port()
            cwd = workerd_config(WORK / f'workerd-{target}', self.port, database_host, 'workers',
                                 self.memory_port)
            command = [str(workerd()), 'serve', 'config.capnp']
            if inspector:
                command.append(f'--inspector-addr=127.0.0.1:{inspector}')
            env = {}
            self.header = 'cf-connecting-ip: {ip}'
        env.update(EXTRA_ENV)
        self.spawned = time.monotonic()
        self.process = subprocess.Popen(command, cwd=cwd, env=env, stdout=log, stderr=log,
                                        stdin=subprocess.DEVNULL)
        self.cold_ms, self.first_ms = self.first_answer()
        self.idle_rss_mb = rss_mb(self.process.pid)

    def first_answer(self):
        """Milliseconds from the spawn to the first readiness answer, and
        that request's own."""
        deadline = self.spawned + 120
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                sys.exit(f'{self.target} ended as it started; see tools/bench/work/logs/{self.target}.log')
            try:
                connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=60)
                connection.connect()
            except OSError:
                time.sleep(0.002)
                continue
            sent = time.monotonic()
            connection.request('GET', '/health/ready', headers={self.header.split(':')[0]: '10.255.0.1'})
            status = connection.getresponse().status
            done = time.monotonic()
            connection.close()
            if status == 200:
                return (done - self.spawned) * 1000, (done - sent) * 1000
            time.sleep(0.01)
        sys.exit(f'{self.target} never answered its readiness check')

    def memory_mb(self) -> float | None:
        """A Worker's linear memory now, in MB; `None` for the others."""
        if self.memory_port is None:
            return None
        connection = http.client.HTTPConnection('127.0.0.1', self.memory_port, timeout=60)
        connection.request('GET', '/')
        answer = json.loads(connection.getresponse().read())
        connection.close()
        return round(answer['bytes'] / 1024 / 1024, 1)

    def stop(self):
        stop(self.process)
        for helper in self.helpers:
            stop(helper)


def wait_for_port(port: int):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            socket.create_connection(('127.0.0.1', port), 1).close()
            return
        except OSError:
            time.sleep(0.01)
    sys.exit(f'nothing listened on {port}')


def rss_mb(pid: int) -> float:
    out = subprocess.run(['ps', '-o', 'rss=', '-p', str(pid)], capture_output=True, text=True).stdout
    return round(int(out.strip() or 0) / 1024, 1)


def cpu_s(pid: int) -> float:
    out = subprocess.run(['ps', '-o', 'time=', '-p', str(pid)], capture_output=True, text=True).stdout.strip()
    total = 0.0
    for part in out.split(':'):
        total = total * 60 + float(part)
    return total


def measure(server: Server, route: str, connections: int, warmup: float, duration: float) -> dict:
    method, path, body, expect = ROUTES[route]
    command = [str(LOAD), '--addr', f'127.0.0.1:{server.port}', '--method', method, '--path', path,
               '--header', server.header, '--connections', str(connections), '--warmup', str(warmup),
               '--duration', str(duration), '--expect', expect]
    if body:
        command += ['--body', body]
    peak = [0.0]
    finished = threading.Event()

    def sample():
        while not finished.wait(0.25):
            peak[0] = max(peak[0], rss_mb(server.process.pid))

    sampler = threading.Thread(target=sample)
    load_before = os.getloadavg()[0]
    started = time.monotonic()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, text=True)
    sampler.start()
    time.sleep(max(0.0, warmup - (time.monotonic() - started)))
    cpu_before = cpu_s(server.process.pid)
    time.sleep(max(0.0, warmup + duration - (time.monotonic() - started)))
    out, _ = process.communicate()
    cpu_after = cpu_s(server.process.pid)
    finished.set()
    sampler.join()
    result = json.loads(out)
    result.update(target=server.target, route=route, load1=round(load_before, 2),
                  cpu_ms_per_request=round((cpu_after - cpu_before) * 1000 / max(result['requests'], 1), 3),
                  cpu_cores=round((cpu_after - cpu_before) / result.get('elapsed_s', duration), 2), peak_rss_mb=peak[0],
                  wasm_mb=server.memory_mb())
    return result


def soak(pg: Postgres, logs: Path, target: str, seconds: float, step: float, connections: int) -> list:
    """Bookings on one server for `seconds`, a window of `step` seconds
    at a time: each window's throughput and CPU, the server's peak
    resident memory in it, and a Worker's linear memory at its end."""
    pg.reset('workers' if target.startswith('workers') else target)
    server = Server(target, pg, logs)
    rows = []
    started = time.monotonic()
    booked = 0
    print(f'{target}: {server.memory_mb()} MB of linear memory at its start', flush=True)
    try:
        while time.monotonic() - started < seconds:
            row = measure(server, 'book', connections, 0, step)
            booked += row['requests']
            row.update(elapsed_s=round(time.monotonic() - started), booked=booked)
            rows.append(row)
            print(f"  {row['elapsed_s']:>5} s  {booked:>8,} bookings  {row['rps']:,.0f} req/s  "
                  f"{row['cpu_ms_per_request']:.3f} CPU ms/booking  RSS {row['peak_rss_mb']} MB  "
                  f"linear {row['wasm_mb']} MB  load {row['load1']}", flush=True)
    finally:
        server.stop()
    return rows


def profile(server: Server, inspector: int, route: str, connections: int, warmup: float, duration: float):
    """workerd's V8 CPU profile while `route` is driven, and
    where its samples fell (tools/bench/bun/profile.ts)."""
    out = WORK / f'{server.target}-{route}.cpuprofile'
    method, path, body, expect = ROUTES[route]
    load = subprocess.Popen([str(LOAD), '--addr', f'127.0.0.1:{server.port}', '--method', method,
                             '--path', path, '--header', server.header, *(['--body', body] if body else []),
                             '--connections', str(connections), '--warmup', str(warmup),
                             '--duration', str(duration), '--expect', expect], stdout=subprocess.PIPE, text=True)
    time.sleep(warmup)
    sampled = run([tool('bun'), 'profile.ts', str(inspector), str(duration - 1), str(out)],
                  cwd=BENCH / 'bun', capture_output=True, text=True)
    measured, _ = load.communicate()
    print(f'{server.target}, {route} x{connections} under the inspector: {measured.strip()}')
    print(sampled.stdout)


def wait_for_quiet(threshold: float, longest: float):
    deadline = time.monotonic() + longest
    while os.getloadavg()[0] > threshold and time.monotonic() < deadline:
        print(f'  load average {os.getloadavg()[0]:.1f} > {threshold}: waiting', flush=True)
        time.sleep(20)


def summary(rows: list, starts: list) -> str:
    def spread(values, digits=0):
        values = sorted(values)
        med = statistics.median(values)
        fmt = (lambda v: f'{v:,.{digits}f}')
        return f'{fmt(med)} ({fmt(values[0])}–{fmt(values[-1])})' if len(values) > 1 else fmt(med)

    lines = ['| target | route | conns | req/s | p50 ms | p99 ms | server CPU ms/req | peak RSS MB | Wasm MB | errors |',
             '|---|---|---:|---:|---:|---:|---:|---:|---:|---:|']
    keys = sorted({(r['route'], r['connections'], r['target']) for r in rows})
    for route, connections, target in keys:
        group = [r for r in rows if (r['route'], r['connections'], r['target']) == (route, connections, target)]
        linear = [r['wasm_mb'] for r in group if r.get('wasm_mb') is not None]
        wasm = spread(linear, 1) if linear else '–'
        lines.append(
            f"| {target} | {route} | {connections} | {spread([r['rps'] for r in group])} | "
            f"{spread([r['p50_us'] / 1000 for r in group], 2)} | {spread([r['p99_us'] / 1000 for r in group], 2)} | "
            f"{spread([r['cpu_ms_per_request'] for r in group], 3)} | {spread([r['peak_rss_mb'] for r in group], 0)} | "
            f"{wasm} | {sum(r['unexpected'] + r['errors'] for r in group)} |")
    lines += ['', '| target | cold start to first answer, ms | that first request, ms | idle RSS MB |',
              '|---|---:|---:|---:|']
    for target in sorted({s['target'] for s in starts}):
        group = [s for s in starts if s['target'] == target]
        lines.append(f"| {target} | {spread([s['cold_ms'] for s in group], 0)} | "
                     f"{spread([s['first_ms'] for s in group], 1)} | {spread([s['idle_rss_mb'] for s in group], 0)} |")
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--duration', type=float, default=10)
    parser.add_argument('--warmup', type=float, default=2)
    parser.add_argument('--connections', default='1,16,64')
    parser.add_argument('--targets', default='native,workers,workers-direct,bun')
    parser.add_argument('--routes', default='book,ready')
    parser.add_argument('--quiet-load', type=float, default=3.0,
                        help='wait before each round until the 1-minute load average is below this')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--profile', action='store_true',
                        help="instead of measuring, profile each workerd target's routes through V8's inspector")
    parser.add_argument('--soak', type=float, metavar='SECONDS',
                        help='instead of measuring, book on each target for this long, a window at a time, '
                             'recording memory as it goes')
    parser.add_argument('--soak-step', type=float, default=30, metavar='SECONDS')
    args = parser.parse_args()
    WORK.mkdir(exist_ok=True)
    logs = WORK / 'logs'
    logs.mkdir(exist_ok=True)
    if not args.skip_build:
        build()
    targets = args.targets.split(',')
    pg = Postgres()
    rows, starts = [], []
    try:
        for database in ['native', 'workers', 'bun']:
            pg.prepare(database)
        if args.profile:
            for target in [t for t in targets if t.startswith('workers')]:
                inspector = free_port()
                server = Server(target, pg, logs, inspector)
                try:
                    for route in args.routes.split(','):
                        profile(server, inspector, route, 16, args.warmup, args.duration)
                finally:
                    server.stop()
            return
        if args.soak:
            soaked = {}
            for target in targets:
                wait_for_quiet(args.quiet_load, 600)
                soaked[target] = soak(pg, logs, target, args.soak, args.soak_step, 16)
            (WORK / 'soak.json').write_text(json.dumps(soaked, indent=1))
            return
        for round_ in range(args.rounds):
            wait_for_quiet(args.quiet_load, 600)
            order = targets[round_ % len(targets):] + targets[:round_ % len(targets)]
            for target in order:
                pg.reset('bun' if target == 'bun' else 'native' if target == 'native' else 'workers')
                server = Server(target, pg, logs)
                starts.append({'target': target, 'cold_ms': round(server.cold_ms, 1),
                               'first_ms': round(server.first_ms, 2), 'idle_rss_mb': server.idle_rss_mb})
                print(f'round {round_ + 1} {target}: cold start {server.cold_ms:.0f} ms', flush=True)
                try:
                    for route in args.routes.split(','):
                        for connections in [int(c) for c in args.connections.split(',')]:
                            row = measure(server, route, connections, args.warmup, args.duration)
                            row['round'] = round_ + 1
                            rows.append(row)
                            print(f"  {route} x{connections}: {row['rps']:,.0f} req/s, "
                                  f"p50 {row['p50_us'] / 1000:.2f} ms, p99 {row['p99_us'] / 1000:.2f} ms, "
                                  f"{row['cpu_ms_per_request']:.3f} CPU ms/req, "
                                  f"{row['unexpected'] + row['errors']} bad, load {row['load1']}", flush=True)
                finally:
                    server.stop()
    finally:
        pg.stop()
    (WORK / 'results.json').write_text(json.dumps({'rows': rows, 'starts': starts}, indent=1))
    print()
    print(summary(rows, starts))


if __name__ == '__main__':
    main()
