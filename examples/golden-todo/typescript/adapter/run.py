#!/usr/bin/env python3
"""Private real-PostgreSQL verification against the versioned shared contract."""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import http.client
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import shlex
import socket
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote, urlencode

from support import dependency_lock

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent
ROOT = PROJECT.parents[2]
WORKERS = ROOT / 'tools/workers'
RUNTIMES = ('node', 'bun', 'workerd')
# Diagnostic inventory only. Executable expectations always come from contract.py;
# absent/broken shared input must still show every obligation as failed.
CASE_IDS = (
    'AUTH-001', 'AUTH-002', 'AUTH-003', 'AUTH-004', 'AUTH-005', 'AUTH-006',
    'AUTH-007', 'AUTH-008', 'AUTH-009', 'AUTH-010', 'AUTH-011', 'AUTH-012',
    'PUBLIC-001', 'CREATE-001', 'CREATE-002', 'CREATE-003', 'CREATE-004',
    'READ-001', 'READ-002', 'READ-003', 'LIST-001', 'LIST-002', 'LIST-003',
    'LIST-004', 'REL-001', 'REL-002', 'REL-003', 'REL-004', 'PATCH-001',
    'PATCH-002', 'PATCH-003', 'PATCH-004', 'PATCH-005', 'DELETE-001',
    'DELETE-002', 'USER-001', 'JOB-001', 'JOB-002', 'JOB-003', 'JOB-004',
    'CONFIG-001', 'CONFIG-002', 'CONFIG-003', 'BOUNDARY-001',
)


def run(args, **kwargs):
    if not kwargs.get('capture_output') and 'stdout' not in kwargs:
        kwargs['stdout'] = sys.stderr
    return subprocess.run(args, check=True, **kwargs)


def tool(name):
    found = shutil.which(name)
    if found:
        return found
    candidates = [Path('/Applications/Postgres.app/Contents/Versions/16/bin') / name,
                  Path.home() / '.bun/bin' / name]
    candidates += sorted(Path('/usr/lib/postgresql').glob('*/bin/' + name), reverse=True)
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    raise RuntimeError(f'{name} unavailable')


def port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


def shared_postgres_module():
    specification = importlib.util.spec_from_file_location('golden_postgres', ROOT / 'tools/golden-todo/postgres.py')
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def workerd_tools():
    """The workerd and Wrangler tools/workers/package.json pins, installed there
    under the lock the Jadpo Workers tests take (crates/jadpo-test/src/workerd_install.rs)."""
    def pinned(name):
        return json.loads((WORKERS / 'package.json').read_text())['dependencies'][name]

    def version(folder):
        try:
            return json.loads((folder / 'package.json').read_text()).get('version')
        except (OSError, ValueError):
            return None

    def installed():
        if version(WORKERS / 'node_modules/wrangler') != pinned('wrangler'):
            return None
        for package in sorted((WORKERS / 'node_modules/@cloudflare').glob('workerd-*')):
            binary = package / 'bin/workerd'
            if version(package) == pinned('workerd') and binary.is_file():
                return binary
        return None

    with (WORKERS / '.install.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        binary = installed()
        if binary is None:
            run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=WORKERS)
            binary = installed()
        if binary is None:
            raise RuntimeError('npm ci gave no pinned workerd for this platform')
    return binary


def bundle(out, entry=None):
    """The Worker as `wrangler deploy` uploads it (wrangler.jsonc, its defaults:
    esbuild, no minification), kept off the network by tools/workers/offline.mjs."""
    workerd_tools()
    command = [tool('node'), '--import', str(WORKERS / 'offline.mjs'),
               str(WORKERS / 'node_modules/wrangler/bin/wrangler.js'), 'deploy',
               *([entry] if entry else []), '--dry-run', '--outdir', str(out)]
    try:
        run(command, cwd=PROJECT, env={**os.environ, 'WRANGLER_SEND_METRICS': 'false', 'CI': '1'})
    finally:
        shutil.rmtree(PROJECT / '.wrangler', ignore_errors=True)
    modules = sorted(out.glob('*.js'))
    if len(modules) != 1:
        raise RuntimeError('wrangler wrote no single Worker module')
    return modules[0]


class DatabaseHost:
    def sql(self, name, statement):
        run([tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(name), '-c', statement],
            stdout=subprocess.DEVNULL)

    def prepare(self, name):
        if not re.fullmatch('[a-z_][a-z0-9_]{0,62}', name):
            raise ValueError('Invalid private database name')
        self.sql('postgres', f'CREATE DATABASE {name}')
        self.created(name)
        for migration in sorted((PROJECT / 'drizzle').glob('*.sql')):
            run([tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(name), '-f', str(migration)],
                stdout=subprocess.DEVNULL)

    def created(self, name):
        pass


class Postgres(DatabaseHost):
    def __init__(self, directory, socket_directory=None):
        self.data = directory / 'pg'
        self.socket_directory = socket_directory
        self.port = port()
        self.started = False
        run([tool('initdb'), '-D', str(self.data), '-U', 'golden', '--auth=trust', '-E', 'UTF8'],
            stdout=subprocess.DEVNULL)
        socket_value = '' if socket_directory is None else '"' + str(socket_directory).replace('"', '""') + '"'
        listen = '127.0.0.1' if socket_directory is None else ''
        options = shlex.join(['-p', str(self.port), '-c', 'unix_socket_directories=' + socket_value,
                              '-c', 'listen_addresses=' + listen, '-c', 'max_connections=100', '-c', 'shared_buffers=32MB'])
        try:
            run([tool('pg_ctl'), '-D', str(self.data), '-l', str(directory / 'postgres.log'),
                 '-o', options, '-w', 'start'], stdout=subprocess.DEVNULL)
            self.started = True
        except BaseException:
            # pg_ctl may have started its exact server before a wait was interrupted.
            subprocess.run([tool('pg_ctl'), '-D', str(self.data), '-m', 'fast', '-w', 'stop'],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            raise

    def url(self, name):
        uri = f'postgresql://golden@127.0.0.1:{self.port}/{name}'
        if self.socket_directory is not None:
            uri += '?' + urlencode({'host': str(self.socket_directory)}, quote_via=quote)
        return uri

    def stop(self):
        if self.started:
            run([tool('pg_ctl'), '-D', str(self.data), '-m', 'fast', '-w', 'stop'],
                stdout=subprocess.DEVNULL)
            self.started = False


class SharedPostgres(DatabaseHost):
    """Own random databases on the gate's server; never stop the shared server."""
    def __init__(self, url):
        self.normalize = shared_postgres_module().postgres_uri
        self.base = self.normalize(url)
        self.owned = []

    def url(self, name):
        return self.normalize(self.base, name)

    def created(self, name):
        # Register before migration: even a failed migration owns a database.
        self.owned.append(name)

    def stop(self):
        for name in list(self.owned):
            self.sql('postgres', f'DROP DATABASE {name} WITH (FORCE)')
            self.owned.remove(name)


class Controller:
    def __init__(self, runtime, environment, directory, name=None):
        self.log = (directory / f'controller-{name or runtime}.log').open('w+')
        try:
            self.process = subprocess.Popen([tool(runtime), str(HERE / 'controller.ts')], cwd=PROJECT,
                                            env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=self.log, text=True, bufsize=1)
        except BaseException:
            self.log.close()
            raise

    def call(self, command):
        self.process.stdin.write(json.dumps(command) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            self.log.flush()
            self.log.seek(0)
            raise RuntimeError('controller stopped: ' + self.log.read()[-6000:])
        answer = json.loads(line)
        if 'error' in answer:
            raise RuntimeError(answer['error'])
        return answer

    def stop(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.process.stdout.close()
        self.log.close()


WORKERD_SETTINGS = ('SESSION_SIGNING_KEY', 'BROWSER_ORIGIN', 'OIDC_ISSUER', 'OIDC_AUDIENCE', 'MAIL_API_KEY')


class WorkerdController:
    """The contract on workerd. Application commands run in the fixture Worker
    (adapter/workerd-controller.ts), which drives src/worker.ts's entry with its
    per-request Store over a Hyperdrive binding straight to PostgreSQL; SQL
    fixtures, signing and the static probes come from a Node controller on the
    same database. Preflight serves the production bundle itself."""
    HELPER = {'sql', 'digest', 'sign', 'open', 'policy_probe', 'api_record'}

    def __init__(self, environment, directory):
        self.environment = environment
        self.directory = directory
        self.executable = workerd_tools()
        self.production = bundle(directory / 'workerd-production')
        self.contract = bundle(directory / 'workerd-contract', 'adapter/workerd-controller.ts')
        self.process = self.log = None
        self.helper = Controller('node', environment, directory, 'workerd-helper')
        try:
            self.port = self.start(self.contract, 'contract')
        except BaseException:
            self.stop()
            raise

    def configuration(self, module, name, changes):
        folder = self.directory / ('workerd-' + name + '-config')
        folder.mkdir(exist_ok=True)
        shutil.copy(module, folder / 'worker.js')
        settings = dict(self.environment)
        for key, value in changes.items():
            if value is None:
                settings.pop(key, None)
            else:
                settings[key] = value
        parameters = shared_postgres_module().connection_parameters(self.environment['DATABASE_URL'])
        host = parameters.get('hostaddr') or parameters.get('host') or '127.0.0.1'
        number = parameters.get('port') or '5432'
        if host.startswith('/'):
            address = 'unix:' + str(Path(host) / ('.s.PGSQL.' + number))
        else:
            address = ('[' + host + ']' if ':' in host else host) + ':' + number
        bindings = []
        if 'DATABASE_URL' not in changes:
            bindings.append('(name = "DATABASE_URL", hyperdrive = (designator = "hyperdrive", '
                            f'database = {json.dumps(parameters["dbname"])}, user = {json.dumps(parameters.get("user", "golden"))}, '
                            f'password = {json.dumps(parameters.get("password", ""))}, scheme = "postgresql"))')
        elif changes['DATABASE_URL'] is not None:
            bindings.append(f'(name = "DATABASE_URL", text = {json.dumps(changes["DATABASE_URL"])})')
        for key in WORKERD_SETTINGS:
            if key in settings:
                bindings.append(f'(name = {json.dumps(key)}, text = {json.dumps(settings[key])})')
        listen = port()
        date = json.loads(re.sub(r'^\s*//.*$', '', (PROJECT / 'wrangler.jsonc').read_text(), flags=re.M))['compatibility_date']
        (folder / 'config.capnp').write_text(f'''using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [(name = "app", worker = .app), (name = "hyperdrive", external = (address = {json.dumps(address)}, tcp = ()))],
  sockets = [(name = "http", address = "127.0.0.1:{listen}", service = "app",
    http = (injectRequestHeaders = [(name = "CF-Connecting-IP", value = "127.0.0.1")]))],
);
const app :Workerd.Worker = (
  modules = [(name = "worker.js", esModule = embed "worker.js")],
  compatibilityDate = {json.dumps(date)},
  bindings = [{", ".join(bindings)}],
);
''')
        return folder, listen

    def start(self, module, name, changes=None):
        folder, listen = self.configuration(module, name, changes or {})
        self.log = (folder / 'workerd.log').open('w+')
        self.process = subprocess.Popen([str(self.executable), 'serve', 'config.capnp'], cwd=folder,
                                        stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log)
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                self.log.seek(0)
                raise RuntimeError('workerd startup failed: ' + self.log.read()[-6000:])
            try:
                socket.create_connection(('127.0.0.1', listen), .1).close()
                return listen
            except OSError:
                time.sleep(.03)
        raise RuntimeError('workerd listener did not start')

    def close_process(self):
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            self.process = None
        if self.log:
            self.log.close()
            self.log = None

    def exchange(self, command):
        connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=90)
        try:
            connection.request('POST', '/control', json.dumps(command), {'content-type': 'application/json'})
            response = connection.getresponse()
            body = response.read()
        finally:
            connection.close()
        if response.status != 200:
            raise RuntimeError('workerd controller failed: ' + body.decode(errors='replace')[-2000:])
        answer = json.loads(body)
        if 'error' in answer:
            raise RuntimeError(answer['error'])
        return answer

    def call(self, command):
        op = command['op']
        if op in self.HELPER:
            return self.helper.call(command)
        if op == 'preflight':
            return {'result': self.preflight(command['environment']), 'statements': []}
        if op == 'clock':
            self.helper.call(command)
        if op == 'outage':
            if 'database' in command:
                self.helper.call({'op': 'outage', 'database': command['database']})
            command = {'op': 'outage', **({'issuer': command['issuer']} if 'issuer' in command else {})}
        return self.exchange(command)

    def probe(self, changes):
        self.port = self.start(self.production, 'production', changes)
        try:
            connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=15)
            try:
                connection.request('GET', '/health/live')
                response = connection.getresponse()
                body = response.read().decode()
                headers = list(response.getheaders())
            finally:
                connection.close()
            time.sleep(.1)
            self.log.flush()
            self.log.seek(0)
            return response.status, body, headers, self.log.read()
        finally:
            self.close_process()

    def preflight(self, changes):
        self.close_process()
        try:
            status, body, _, _ = self.probe({})
            if status != 200 or json.loads(body) != {'status': 'live'}:
                raise AssertionError('positive production Worker startup control failed')
            status, body, headers, output = self.probe(changes)
            events = []
            for line in output.splitlines():
                try:
                    events.append(json.loads(line))
                except ValueError:
                    pass
            problems = [problem for event in events if isinstance(event, dict) for problem in event.get('problems', [])]
            private = [value for name, value in self.environment.items() if name in ('SESSION_SIGNING_KEY', 'MAIL_API_KEY')]
            private += [value for name, value in changes.items() if value and name in ('DATABASE_URL', 'MAIL_API_KEY')]
            safe = all(value not in body + output for value in private) and 'password@' not in body + output
            return {'refused': status == 503, 'handlerRan': False, 'status': status, 'body': json.loads(body),
                    'headers': headers, 'declarations': [problem.get('variable') for problem in problems],
                    'diagnosticCodes': [problem.get('problem') for problem in problems], 'safe': safe}
        finally:
            self.port = self.start(self.contract, 'contract')

    def stop(self):
        try:
            self.close_process()
        finally:
            self.helper.stop()


def git_state(path):
    return {'revision': run(['git', 'rev-parse', 'HEAD'], cwd=path, capture_output=True, text=True).stdout.strip(),
            'dirty': bool(run(['git', 'status', '--porcelain'], cwd=path, capture_output=True, text=True).stdout)}


def inputs(source):
    digest = hashlib.sha256()
    files = [path for path in PROJECT.rglob('*') if path.is_file()
             and not {'node_modules', 'build', '__pycache__', '.wrangler'}.intersection(path.relative_to(PROJECT).parts)]
    files += [source / 'tools/golden-todo/contract.py',
              source / 'tools/golden-todo/test_contract.py',
              source / 'examples/golden-todo/acceptance-v0.2.json',
              ROOT / 'tools/golden-todo/postgres.py', ROOT / 'tools/golden-todo/test_postgres.py',
              ROOT / 'tools/verify.py', ROOT / '.github/workflows/validation.yml']
    for path in sorted(set(files)):
        digest.update(str(path).encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


def runtime_version(name):
    try:
        executable = workerd_tools() if name == 'workerd' else tool(name)
        return run([str(executable), '--version'], capture_output=True, text=True).stdout.strip()
    except (RuntimeError, OSError, subprocess.SubprocessError):
        return 'unavailable'


def read_runtimes(value):
    runtimes = value.split(',')
    if not all(runtimes) or len(runtimes) != len(set(runtimes)) or set(runtimes) - set(RUNTIMES):
        raise ValueError('runtimes must be nonempty, unique and selected from ' + ','.join(RUNTIMES))
    return runtimes


def complete_contract(source, runtimes, selected, reports):
    expected = {(runtime, identifier) for runtime in RUNTIMES for identifier in CASE_IDS}
    actual = {(report['runtime'], report['id']) for report in reports}
    return (source.resolve() == ROOT and set(runtimes) == set(RUNTIMES)
            and set(selected) == set(CASE_IDS) and len(reports) == len(expected) and actual == expected
            and all(report['status'] == 'passed' for report in reports))


def load_registry(source):
    specification = source / 'examples/golden-todo/acceptance-v0.2.json'
    contract = source / 'tools/golden-todo/contract.py'
    if not specification.is_file() or not contract.is_file():
        raise RuntimeError('shared golden contract unavailable in the selected checkout')
    spec = importlib.util.spec_from_file_location('golden_shared_contract', contract)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    document = json.loads(specification.read_text())
    module.validate_registry(document)
    if (len(document['cases']) != len(CASE_IDS)
            or {case['id'] for case in document['cases']} != set(CASE_IDS)
            or set(module.CASES) != set(CASE_IDS) or document['contractVersion'] != 'todo-v0.2'):
        raise RuntimeError('shared golden case inventory differs from todo-v0.2')
    return document, module


def unavailable_report(source, runtimes, selected, message):
    cases = [{'runtime': runtime, 'id': identifier, 'status': 'failed', 'message': message}
             for runtime in runtimes for identifier in selected]
    return report(source, runtimes, selected, cases, {})


def report(source, runtimes, selected, cases, provenance):
    failed = [case for case in cases if case['status'] != 'passed']
    return {'contractVersion': 'todo-v0.2', 'preview': source.resolve() != ROOT,
            'completeContract': complete_contract(source, runtimes, selected, cases), 'sources': provenance,
            'counts': {'ran': len(cases), 'passed': len(cases) - len(failed), 'failed': len(failed)}, 'cases': cases}


def verify_contract(source, runtimes, selected=None, unit=False):
    source = source.resolve()
    selected = list(CASE_IDS) if selected is None else selected
    try:
        _, module = load_registry(source)
    except Exception as error:
        return unavailable_report(source, runtimes, selected, str(error))
    before = inputs(source)
    provenance = {'typescriptSource': str(PROJECT), 'typescriptGit': git_state(ROOT),
                  'sharedContractSource': str(source), 'sharedContractGit': git_state(source),
                  'beforeFingerprint': before, 'runtimeVersions': {runtime: runtime_version(runtime) for runtime in runtimes}}
    reports = []
    server = controller = None
    (PROJECT / 'build').mkdir(exist_ok=True)
    temporary = tempfile.TemporaryDirectory(prefix='local-', dir=PROJECT / 'build')
    try:
        directory = Path(temporary.name)
        shared = os.environ.get('JADPO_TEST_DATABASE_URL', '').strip()
        server = SharedPostgres(shared) if shared else Postgres(directory)
        env = {**os.environ, 'GOLDEN_PSQL': tool('psql'), 'SESSION_SIGNING_KEY': secrets.token_urlsafe(32),
               'BROWSER_ORIGIN': 'https://todo.example.test', 'OIDC_ISSUER': 'https://identity.example/tenant/',
               'OIDC_AUDIENCE': 'golden-todo', 'MAIL_API_KEY': secrets.token_urlsafe(32),
               'CONTROL_DATABASE_URL': server.url('postgres')}
        tag = secrets.token_hex(5)
        if unit:
            name = 'golden_tests_' + tag
            server.prepare(name)
            run(['npm', 'run', 'test:all'], cwd=PROJECT, env={**env, 'TEST_DATABASE_URL': server.url(name)})
        for runtime in runtimes:
            if provenance['runtimeVersions'][runtime] == 'unavailable':
                reports.extend({'runtime': runtime, 'id': identifier, 'status': 'failed',
                                'message': runtime + ' executable unavailable'} for identifier in selected)
                continue
            if unit and runtime != 'workerd':
                name = 'golden_host_' + runtime + '_' + tag
                server.prepare(name)
                run([tool(runtime), str(HERE / 'host-negatives.ts')], cwd=PROJECT,
                    env={**env, 'DATABASE_URL': server.url(name)})
            name = 'golden_' + runtime + '_' + tag
            server.prepare(name)
            database = {**env, 'DATABASE_URL': server.url(name)}
            controller = (WorkerdController(database, directory) if runtime == 'workerd'
                          else Controller(runtime, database, directory))
            for identifier in selected:
                try:
                    harness = module.Contract(controller)
                    harness.reset()
                    module.CASES[identifier](harness)
                    reports.append({'runtime': runtime, 'id': identifier, 'status': 'passed'})
                except Exception as error:
                    reports.append({'runtime': runtime, 'id': identifier, 'status': 'failed', 'message': str(error)})
            controller.stop()
            controller = None
    except Exception as error:
        reached = {(item['runtime'], item['id']) for item in reports}
        reports.extend({'runtime': runtime, 'id': identifier, 'status': 'failed', 'message': str(error)}
                       for runtime in runtimes for identifier in selected if (runtime, identifier) not in reached)
    finally:
        # Await children before dropping only owned DBs, then remove their files.
        try:
            if controller:
                controller.stop()
        finally:
            try:
                if server:
                    server.stop()
            except Exception as error:
                reports.append({'runtime': 'all', 'id': 'CLEANUP', 'status': 'failed', 'message': str(error)})
            finally:
                temporary.cleanup()
    after = inputs(source)
    provenance.update({'afterFingerprint': after, 'inputsUnchanged': before == after,
                       'sharedContractSha256': hashlib.sha256((source / 'tools/golden-todo/contract.py').read_bytes()).hexdigest(),
                       'acceptanceSha256': hashlib.sha256((source / 'examples/golden-todo/acceptance-v0.2.json').read_bytes()).hexdigest()})
    if before != after:
        reports.append({'runtime': 'all', 'id': 'INPUTS', 'status': 'failed', 'message': 'source inputs changed during verification'})
    return report(source, runtimes, selected, reports, provenance)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--contract-source', type=Path, default=ROOT,
                        help='explicit other checkout is a preview; default is this checkout')
    parser.add_argument('--runtimes', default=','.join(RUNTIMES))
    parser.add_argument('--case', action='append', default=[])
    parser.add_argument('--unit', action='store_true')
    args = parser.parse_args()
    try:
        runtimes = read_runtimes(args.runtimes)
        if set(args.case) - set(CASE_IDS):
            raise ValueError('unknown case')
    except ValueError as error:
        parser.error(str(error))
    selected = [identifier for identifier in CASE_IDS if not args.case or identifier in args.case]
    with dependency_lock(PROJECT):
        result = verify_contract(args.contract_source, runtimes, selected, args.unit)
    print(json.dumps(result, indent=2))
    return bool(result['counts']['failed'])


if __name__ == '__main__':
    raise SystemExit(main())
