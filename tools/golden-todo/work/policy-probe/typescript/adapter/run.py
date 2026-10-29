#!/usr/bin/env python3
"""Private real-PostgreSQL verification against the versioned shared contract."""
from __future__ import annotations

import argparse
import hashlib
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
from urllib.parse import quote, urlencode

from support import dependency_lock

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent
ROOT = PROJECT.parents[2]
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
        specification = importlib.util.spec_from_file_location('golden_postgres', ROOT / 'tools/golden-todo/postgres.py')
        normalizer = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(normalizer)
        self.normalize = normalizer.postgres_uri
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
    def __init__(self, runtime, environment, directory):
        self.log = (directory / f'controller-{runtime}.log').open('w+')
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


def git_state(path):
    return {'revision': run(['git', 'rev-parse', 'HEAD'], cwd=path, capture_output=True, text=True).stdout.strip(),
            'dirty': bool(run(['git', 'status', '--porcelain'], cwd=path, capture_output=True, text=True).stdout)}


def inputs(source):
    digest = hashlib.sha256()
    files = [path for path in PROJECT.rglob('*') if path.is_file()
             and not {'node_modules', 'build', '__pycache__'}.intersection(path.relative_to(PROJECT).parts)]
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
        return run([tool(name), '--version'], capture_output=True, text=True).stdout.strip()
    except (RuntimeError, subprocess.SubprocessError):
        return 'unavailable'


def read_runtimes(value):
    runtimes = value.split(',')
    if not all(runtimes) or len(runtimes) != len(set(runtimes)) or set(runtimes) - {'node', 'bun'}:
        raise ValueError('runtimes must be nonempty, unique and selected from node,bun')
    return runtimes


def complete_contract(source, runtimes, selected, reports):
    expected = {(runtime, identifier) for runtime in ['node', 'bun'] for identifier in CASE_IDS}
    actual = {(report['runtime'], report['id']) for report in reports}
    return (source.resolve() == ROOT and set(runtimes) == {'node', 'bun'}
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
            if unit:
                name = 'golden_host_' + runtime + '_' + tag
                server.prepare(name)
                run([tool(runtime), str(HERE / 'host-negatives.ts')], cwd=PROJECT,
                    env={**env, 'DATABASE_URL': server.url(name)})
            name = 'golden_' + runtime + '_' + tag
            server.prepare(name)
            controller = Controller(runtime, {**env, 'DATABASE_URL': server.url(name)}, directory)
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
    parser.add_argument('--runtimes', default='node,bun')
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
