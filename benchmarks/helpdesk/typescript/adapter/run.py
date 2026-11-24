#!/usr/bin/env python3
"""Private PostgreSQL and controller lifecycle for the hidden tests and the
parity runner: `SharedPostgres(url).prepare(name)` makes a migrated database
on a server someone else runs, `Postgres(folder)` starts one of its own, and
`Controller('node', environment, folder)` drives adapter/controller.ts."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import re
import shlex
import shutil
import socket
import subprocess
import sys

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent
ROOT = PROJECT.parents[2]


def run(args, **kwargs):
    if not kwargs.get('capture_output') and 'stdout' not in kwargs:
        kwargs['stdout'] = sys.stderr
    return subprocess.run(args, check=True, **kwargs)


def tool(name):
    found = shutil.which(name)
    if found:
        return found
    candidates = [Path('/Applications/Postgres.app/Contents/Versions/16/bin') / name]
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
    specification = importlib.util.spec_from_file_location('desk_postgres', ROOT / 'benchmarks/contract/postgres.py')
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


class DatabaseHost:
    def sql(self, name, statement):
        run([tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url(name), '-c', statement],
            stdout=subprocess.DEVNULL)

    def prepare(self, name):
        """A new database `name` with every migration in drizzle/ applied."""
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
    """A disposable PostgreSQL cluster under `directory`, on 127.0.0.1."""
    def __init__(self, directory):
        self.data = directory / 'pg'
        self.port = port()
        self.started = False
        run([tool('initdb'), '-D', str(self.data), '-U', 'golden', '--auth=trust', '-E', 'UTF8'],
            stdout=subprocess.DEVNULL)
        options = shlex.join(['-p', str(self.port), '-c', 'unix_socket_directories=',
                              '-c', 'listen_addresses=127.0.0.1', '-c', 'max_connections=100'])
        try:
            run([tool('pg_ctl'), '-D', str(self.data), '-l', str(directory / 'postgres.log'),
                 '-o', options, '-w', 'start'], stdout=subprocess.DEVNULL)
            self.started = True
        except BaseException:
            subprocess.run([tool('pg_ctl'), '-D', str(self.data), '-m', 'fast', '-w', 'stop'],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            raise

    def url(self, name):
        return f'postgresql://golden@127.0.0.1:{self.port}/{name}'

    def stop(self):
        if self.started:
            run([tool('pg_ctl'), '-D', str(self.data), '-m', 'fast', '-w', 'stop'], stdout=subprocess.DEVNULL)
            self.started = False


class SharedPostgres(DatabaseHost):
    """Own databases on a server someone else runs; never stop that server."""
    def __init__(self, url):
        self.normalize = shared_postgres_module().postgres_uri
        self.base = self.normalize(url)
        self.owned = []

    def url(self, name):
        return self.normalize(self.base, name)

    def created(self, name):
        # Owned from creation: even a failed migration leaves it to drop.
        self.owned.append(name)

    def stop(self):
        for name in list(self.owned):
            self.sql('postgres', f'DROP DATABASE {name} WITH (FORCE)')
            self.owned.remove(name)


class Controller:
    """adapter/controller.ts under `runtime`, one JSON command per line."""
    def __init__(self, runtime, environment, directory, name=None):
        self.log = (Path(directory) / f'controller-{name or runtime}.log').open('w+')
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
