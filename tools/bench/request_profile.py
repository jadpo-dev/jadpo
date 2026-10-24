#!/usr/bin/env python3
"""Guarded whole-request CPU sampling of the existing production HTTP harnesses.

The inspector is diagnostic: its CPU samples are not wall-time attribution.
Build/run commands and current results are documented in tools/bench/README.md.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import secrets
import shutil
import socket
import struct
import subprocess
import sys
import time
import re
import threading
from urllib.parse import urlsplit

import golden
import run as spike

ROOT = spike.ROOT
HERE = ROOT / 'tools/bench'
WORK = HERE / 'work/request-profile'


def validate_profile(profile):
    """Reject truncated or internally inconsistent CDP profiles before use."""
    def number(value):
        return type(value) in (int, float) and math.isfinite(value)
    if not isinstance(profile, dict): raise ValueError('Inspector profile is not an object')
    nodes, samples, deltas = (profile.get(key) for key in ('nodes', 'samples', 'timeDeltas'))
    if not all(isinstance(value, list) and value for value in (nodes, samples, deltas)):
        raise ValueError('Inspector profile has no sampled nodes or time deltas')
    start, end = profile.get('startTime'), profile.get('endTime')
    if not number(start) or not number(end) or start < 0 or end <= start:
        raise ValueError('Inspector profile has invalid start/end times')
    ids = set()
    children = {}
    for node in nodes:
        if not isinstance(node, dict) or type(node.get('id')) is not int or node['id'] in ids:
            raise ValueError('Inspector profile has missing or duplicate node identities')
        frame = node.get('callFrame')
        if not isinstance(frame, dict) or not all(isinstance(frame.get(k), str) for k in ('functionName', 'url', 'scriptId')):
            raise ValueError('Inspector profile has an invalid call frame')
        ids.add(node['id']); children[node['id']] = node.get('children', [])
    if len(samples) != len(deltas) or any(type(s) is not int or s not in ids for s in samples):
        raise ValueError('Inspector sample identities/deltas do not match')
    if any(not number(d) or d < 0 for d in deltas) or not 0 < sum(deltas) <= end-start:
        raise ValueError('Inspector sample deltas do not fit its time window')
    parents = {}
    for parent, below in children.items():
        if not isinstance(below, list): raise ValueError('Inspector children are not a list')
        for child in below:
            if type(child) is not int or child not in ids or child in parents:
                raise ValueError('Inspector profile has missing or repeated children')
            parents[child] = parent
    roots = ids-set(parents)
    if len(roots) != 1: raise ValueError('Inspector profile has no unique root')
    seen = set(); pending = list(roots)
    while pending:
        node = pending.pop()
        if node in seen: raise ValueError('Inspector profile has a cycle')
        seen.add(node); pending.extend(children[node])
    if seen != ids: raise ValueError('Inspector profile has disconnected nodes')
    return profile


class Inspector:
    """A bounded sequential CDP client, attached only to the named kiosk."""
    def __init__(self, port, output, timeout=10, target='kiosk'):
        self.output = Path(output)
        if self.output.exists(): raise FileExistsError(self.output)
        self.port, self.timeout, self.target = port, timeout, target
        self.socket = self.reader = None
        self.identity = None
        self.sequence = 0
        self.profile = None

    def connect(self):
        connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=self.timeout)
        timer = None
        try:
            connection.connect()
            peer = connection.sock
            def expire():
                try: peer.shutdown(socket.SHUT_RDWR)
                except OSError: pass
            timer = threading.Timer(self.timeout, expire); timer.start()
            connection.request('GET', '/json/list')
            response = connection.getresponse()
            if response.status != 200: raise ValueError('Inspector target listing failed')
            targets = json.loads(response.read(1024*1024))
        finally:
            if timer:
                timer.cancel(); timer.join()
            connection.close()
        if not isinstance(targets, list): raise ValueError('Inspector targets are not a list')
        matches = [t for t in targets if isinstance(t, dict) and t.get('id') == self.target]
        if len(matches) != 1: raise ValueError(f'Inspector must expose exactly one {self.target} target')
        target = matches[0]
        url = urlsplit(target.get('webSocketDebuggerUrl', ''))
        if (url.scheme != 'ws' or url.hostname != '127.0.0.1' or url.port != self.port
                or url.username or url.password or not url.path or url.fragment):
            raise ValueError('Inspector target does not belong to this loopback process')
        self.identity = {k: target.get(k) for k in ('id', 'title', 'type', 'url', 'webSocketDebuggerUrl')}
        self.socket = socket.create_connection(('127.0.0.1', self.port), timeout=self.timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        self.socket.sendall((f'GET {url.path}{"?"+url.query if url.query else ""} HTTP/1.1\r\n'
            f'Host: 127.0.0.1:{self.port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n'
            f'Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n\r\n').encode())
        header = bytearray(); deadline = time.monotonic()+self.timeout
        while not header.endswith(b'\r\n\r\n'):
            remaining = deadline-time.monotonic()
            if remaining <= 0: raise TimeoutError('Inspector handshake timed out')
            self.socket.settimeout(remaining)
            part = self.socket.recv(1)
            if not part: raise EOFError('Inspector closed during WebSocket handshake')
            header.extend(part)
            if len(header) > 65536: raise ValueError('Inspector handshake is too large')
        lines = header.decode('ascii').split('\r\n')
        fields = {line.split(':', 1)[0].lower(): line.split(':', 1)[1].strip() for line in lines[1:] if ':' in line}
        expected = base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
        if (lines[0].split()[1] != '101' or fields.get('sec-websocket-accept') != expected
                or fields.get('upgrade', '').lower() != 'websocket'):
            raise ValueError('Inspector WebSocket handshake did not match')

    def read_exact(self, count, deadline):
        data = bytearray()
        while len(data) < count:
            remaining = deadline-time.monotonic()
            if remaining <= 0: raise TimeoutError('Inspector frame timed out')
            self.socket.settimeout(remaining)
            part = self.socket.recv(count-len(data))
            if not part: raise EOFError('Inspector closed before answering')
            data.extend(part)
        return bytes(data)

    def send_frame(self, payload, opcode=1):
        mask = os.urandom(4); size = len(payload)
        header = bytes([128 | opcode, 128 | size]) if size < 126 else bytes([128 | opcode, 254])+struct.pack('!H', size)
        self.socket.sendall(header+mask+bytes(c ^ mask[i % 4] for i,c in enumerate(payload)))

    def message(self, deadline):
        parts = bytearray(); started = False
        while True:
            remaining = deadline-time.monotonic()
            if remaining <= 0: raise TimeoutError('Inspector command timed out')
            self.socket.settimeout(remaining)
            first, size = self.read_exact(2, deadline)
            opcode = first & 15
            if first & 112 or size & 128: raise ValueError('Inspector sent an invalid WebSocket frame')
            size &= 127
            if size == 126: size = struct.unpack('!H', self.read_exact(2, deadline))[0]
            elif size == 127: size = struct.unpack('!Q', self.read_exact(8, deadline))[0]
            if size > 128*1024*1024 or len(parts)+size > 128*1024*1024:
                raise ValueError('Inspector message is too large')
            raw = self.read_exact(size, deadline)
            if opcode == 8: raise EOFError('Inspector closed before answering')
            if opcode == 9:
                if size > 125: raise ValueError('Inspector ping is too large')
                self.send_frame(raw, 10); continue
            if opcode == 10: continue
            if (opcode == 1 and started) or (opcode == 0 and not started) or opcode not in (0, 1):
                raise ValueError('Inspector sent an invalid fragmented message')
            parts.extend(raw); started = True
            if first & 128: return json.loads(parts)

    def command(self, method, params=None):
        self.sequence += 1
        self.send_frame(json.dumps({'id': self.sequence, 'method': method, 'params': params or {}}).encode())
        deadline = time.monotonic()+self.timeout
        while True:
            message = self.message(deadline)
            if not isinstance(message, dict): raise ValueError('Inspector answer is not an object')
            if 'id' not in message: continue
            if message['id'] != self.sequence: raise ValueError('Inspector answered another command identity')
            if 'error' in message: raise ValueError(f'Inspector refused {method}: {message["error"]}')
            if method in ('Profiler.enable', 'Profiler.setSamplingInterval', 'Profiler.start') and message.get('result') is None:
                return {}  # workerd omits the empty result object for these commands.
            if not isinstance(message.get('result'), dict): raise ValueError('Inspector command has no result')
            return message['result']

    def start(self):
        try:
            self.connect(); self.command('Profiler.enable')
            self.command('Profiler.setSamplingInterval', {'interval': 100})
            self.command('Profiler.start'); self.started = time.time()
        except BaseException:
            self.close(); raise

    def stop(self):
        try:
            raw = self.command('Profiler.stop').get('profile')
            self.ended = time.time()
            self.raw_profile = raw
            try:
                self.profile = validate_profile(raw)
            except ValueError:
                self.output.parent.mkdir(parents=True, exist_ok=True)
                with self.output.with_suffix('.invalid.json').open('x') as handle: json.dump(raw, handle)
                raise
            self.output.parent.mkdir(parents=True, exist_ok=True)
            with self.output.open('x') as handle: json.dump(raw, handle)
        finally:
            self.close()

    def close(self):
        if self.reader is not None: self.reader.close(); self.reader = None
        if self.socket is not None: self.socket.close(); self.socket = None


def payloads():
    return {'ascii-small': 'x'*100, 'ascii-large': 'x'*65536,
            'unicode-large': ('é🙂"\\\n\t'*10923)[:65536], 'late-escape': 'x'*65535+'\n'}


def wasm_sections(data):
    """Exact encoded noncustom sections, preserving indices/code, not just size."""
    if data[:8] != b'\0asm\x01\0\0\0': raise ValueError('Invalid Wasm header')
    result = []; offset = 8
    while offset < len(data):
        begin = offset; kind = data[offset]; offset += 1; size = shift = 0
        while True:
            if offset >= len(data) or shift > 28: raise ValueError('Truncated Wasm section size')
            byte = data[offset]; offset += 1; size |= (byte & 127) << shift
            if not byte & 128: break
            shift += 7
        end = offset+size
        if end > len(data): raise ValueError('Truncated Wasm section')
        if kind: result.append(data[begin:end])
        offset = end
    return result



class Window:
    """Observe the owned PostgreSQL process's parameter-free duration records."""
    def __init__(self, inspector, logfile, durations=True):
        self.inspector, self.logfile, self.durations = inspector, logfile, durations
        self.observation = None
    def start(self):
        if self.inspector: self.inspector.start()
        self.offset = self.logfile.stat().st_size
    def stop(self):
        if self.inspector: self.inspector.stop()
        raw = self.logfile.read_bytes()[self.offset:].decode(errors='replace')
        durations = [float(v) for v in re.findall(r'duration: ([0-9.]+) ms', raw)]
        if self.durations and not durations:
            raise ValueError('Disposable PostgreSQL emitted no server duration observations')
        if not self.durations and durations:
            raise ValueError('PostgreSQL emitted duration records while logging was disabled')
        self.observation = {'enabled': self.durations, 'server_phase_duration_ms': durations, 'server_duration_records': len(durations),
            'duration_record_prefixes': [line.split(' ms', 1)[0]+' ms' for line in raw.splitlines() if re.search(r'duration: [0-9.]+ ms', line)],
            'scope': ('PostgreSQL log_duration protocol-phase records, including separate Parse/Bind/Execute where applicable, for the load window. Enabled for this owned disposable process lifetime; SQL text and parameters excluded. Logging adds diagnostic overhead; overlapping records cannot be added/subtracted into request tails.' if self.durations else 'PostgreSQL duration logging disabled and checked before load; no server duration observations collected.')}
    def close(self):
        if self.inspector: self.inspector.close()


def check_duration_setting(database, enabled):
    actual = database.query('postgres', "SELECT current_setting('log_duration')")
    if actual != ('on' if enabled else 'off'):
        raise ValueError('PostgreSQL duration setting differs from the requested mode')


def configure(directory):
    """Existing harness globals, used sequentially; never import todo eagerly."""
    golden.WORK = directory/'golden'; golden.STAMP = golden.WORK/'build.json'
    spike.WORK = golden.WORK; spike.EXTRA_ENV = {}; spike.EXTRA_BINDINGS = []


def build(label):
    import ai_accounting as guard
    from connected_deliveries import CARGO_WRAPPER
    from version_writes import FLAGS, selected_flags
    before = guard.source(); directory = WORK/label
    directory.mkdir(parents=True, exist_ok=False)
    stamp = {'source': before, 'start': context(), 'artifacts': {}, 'complete': False,
             'inherited_flags': selected_flags(os.environ),
             'compiler_selection': {'requested': {key: os.environ.get(key) for key in ('RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN')},
                 'rustup_which': {name: subprocess.check_output(['rustup', 'which', name], text=True).strip()
                                  for name in ('rustc', 'rustdoc', 'cargo')},
                 'scope': 'Outer requested selectors and Rustup resolution; not an independent trace of child executable selection.'}}
    (directory/'build.json').write_text(json.dumps(stamp, indent=2)+'\n')
    wrapper = directory/'capture-cargo'; wrapper.write_text(CARGO_WRAPPER); wrapper.chmod(0o755)
    provenance = directory/'cargo-provenance.jsonl'
    real = subprocess.check_output(['rustup', 'which', 'cargo'], text=True).strip()
    changes = {**FLAGS, 'CARGO': str(wrapper), 'JADPO_BENCH_CARGO_REAL': real,
               'JADPO_BENCH_CARGO_LOG': str(provenance), 'npm_config_offline': 'true'}
    previous = {k: os.environ.get(k) for k in changes}
    try:
        os.environ.update(changes)
        subprocess.run(['npm', 'ci', '--offline', '--ignore-scripts', '--no-audit', '--no-fund'],
                       cwd=ROOT/'tools/workers', check=True)
        configure(directory); golden.WORK.mkdir()
        golden_stamp = golden.build()
        import todo
        todo.WORK = directory/'focused'; spike.WORK = todo.WORK
        todo.build()
        focused_stamp = json.loads((todo.WORK/'build.json').read_text())
        subprocess.run(['cargo', 'build', '--locked', '-p', 'jadpo-codegen', '--example', 'jadpo-profile-worker'],
                       cwd=ROOT, check=True)
        binder = ROOT/'target/debug/examples/jadpo-profile-worker'
        stamp['diagnostic'] = {}
        paths = [wrapper, provenance, binder, *map(Path, golden_stamp['artifacts']),
                 *map(Path, focused_stamp['artifacts'])]
        for name, work in (('golden', golden.WORK), ('focused', todo.WORK)):
            for target in ('native', 'workers'):
                project = work/target
                paths.extend(p for p in (project/'build/rust').rglob('*') if p.is_file())
                paths.extend(p for p in project.rglob('*') if p.is_file()
                             and not {'build', '.jadpo'} & set(p.relative_to(project).parts))
            original = work/'workers/build/target/kiosk'
            compiled = work/'workers/build/rust/kiosk/jadpo_worker_kiosk.wasm'
            named = directory/('named-'+name)
            subprocess.run([str(binder), str(compiled), str(named)], check=True)
            if (named/'kiosk.js').read_bytes() != (original/'kiosk.js').read_bytes():
                raise ValueError('Diagnostic JavaScript differs from production')
            if wasm_sections((named/'kiosk.wasm').read_bytes()) != wasm_sections((original/'kiosk.wasm').read_bytes()):
                raise ValueError('Diagnostic executable Wasm sections differ from production')
            for path in original.iterdir():
                if path.is_file() and path.name not in ('kiosk.js', 'kiosk.wasm'):
                    shutil.copy2(path, named/path.name)
            paths += [compiled, *[p for p in named.iterdir() if p.is_file()]]
            stamp['diagnostic'][name] = {'compiled_input': str(compiled), 'production': str(original),
                'named': str(named), 'javascript_identical': True, 'noncustom_sections_identical': True,
                'production_bytes': (original/'kiosk.wasm').stat().st_size,
                'diagnostic_bytes': (named/'kiosk.wasm').stat().st_size}
        stamp['runtimes'] = {}
        for name in ('workerd', 'bun', 'node', 'psql', 'rustc', 'cargo', 'python'):
            location = (subprocess.check_output(['rustup', 'which', name], text=True).strip()
                        if name in ('rustc', 'cargo') else (sys.executable if name == 'python'
                        else (spike.workerd() if name == 'workerd' else spike.tool(name))))
            path = Path(location).resolve(); paths.append(path)
            stamp['runtimes'][name] = {'path': str(path), 'sha256': golden.sha(path),
                'version': subprocess.check_output([str(path), '--version'], text=True).strip()}
        for key in ('RUSTC', 'RUSTDOC'):
            if os.environ.get(key): paths.append(Path(os.environ[key]))
        paths.extend(Path(path) for path in stamp['compiler_selection']['rustup_which'].values())
        snapshots = directory/'artifacts'; snapshots.mkdir()
        stamp['saved'] = {}
        for index, path in enumerate(dict.fromkeys(paths)):
            saved = snapshots/f'{index:04d}-{path.name}'; shutil.copy2(path, saved)
            digest = golden.sha(path)
            stamp['artifacts'].update({str(path): digest, str(saved): golden.sha(saved)})
            stamp['saved'][str(path)] = str(saved)
        stamp['inner_cargo'] = [json.loads(line) for line in provenance.read_text().splitlines()]
        if not stamp['inner_cargo']: raise ValueError('Missing effective inner Cargo flags')
        stamp.update(golden=golden_stamp, focused=focused_stamp, end=context(),
                     flags=selected_flags(os.environ), complete=True)
        if guard.source() != before: raise ValueError('Source changed during preparation')
        (directory/'build.json').write_text(json.dumps(stamp, indent=2)+'\n')
        guard.validate(stamp, before)
    except BaseException as error:
        stamp.update(complete=False, failure=str(error), end=context())
        (directory/'build.json').write_text(json.dumps(stamp, indent=2)+'\n')
        raise
    finally:
        for key, value in previous.items():
            if value is None: os.environ.pop(key, None)
            else: os.environ[key] = value
    print(f'passed: request profiles · 176 contract cases · {directory}')


def context():
    return {'utc': time.time(), 'load': os.getloadavg(), 'host_cpu_ticks': golden.host_cpu_ticks(),
            'processes': subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pcpu=,comm='], text=True)}


def fresh(directory):
    import ai_accounting as guard
    stamp = json.loads((directory/'build.json').read_text())
    if stamp.get('complete') is not True: raise ValueError('Incomplete build')
    guard.validate(stamp, guard.source())
    golden.validate_conformance(stamp['golden']['conformance'], golden.contract_ids())
    return stamp


def summary(profile):
    from collections import Counter
    validate_profile(profile)
    nodes = {n['id']: n for n in profile['nodes']}
    parent = {child: n['id'] for n in profile['nodes'] for child in n.get('children', [])}
    self_time, inclusive, kinds = Counter(), Counter(), Counter()
    samples = Counter(profile['samples']); inclusive_samples = Counter()
    for leaf, delta in zip(profile['samples'], profile['timeDeltas']):
        self_time[leaf] += delta
        node = nodes[leaf]; frame = node['callFrame']; name, url = frame['functionName'], frame['url']
        kind = name if name in ('(idle)', '(garbage collector)', '(program)') else (
            'WebAssembly' if url.startswith('wasm://') else 'JavaScript/native')
        kinds[kind] += delta
        current = leaf
        while current is not None:
            inclusive[current] += delta; inclusive_samples[current] += 1
            current = parent.get(current)
    total = sum(profile['timeDeltas'])
    def rows(counter, counts):
        return [{'node_id': n, 'parent_id': parent.get(n),
                 'function': nodes[n]['callFrame']['functionName'], 'url': nodes[n]['callFrame']['url'],
                 'script_id': nodes[n]['callFrame']['scriptId'],
                 'line_number': nodes[n]['callFrame'].get('lineNumber'),
                 'column_number': nodes[n]['callFrame'].get('columnNumber'),
                 'sampled_us': us, 'percent': us*100/total, 'samples': counts[n]}
                for n, us in counter.most_common(50)]
    return {'samples': len(profile['samples']), 'sampled_us': total,
            'profile_window_us': profile['endTime']-profile['startTime'],
            'categories': {k: {'sampled_us': v, 'percent': v*100/total} for k,v in kinds.items()},
            'self': rows(self_time, samples), 'inclusive': rows(inclusive, inclusive_samples)}


def wire_probe(server, path, expected, token=None):
    connection = http.client.HTTPConnection('127.0.0.1', server.port, timeout=15)
    headers = {server.header.split(':', 1)[0]: '10.253.0.1'}
    if token: headers['authorization'] = 'Bearer '+token
    try:
        connection.request('GET', path, headers=headers)
        answer = connection.getresponse(); raw = answer.read()
        if answer.status != 200 or json.loads(raw) != expected:
            raise ValueError('Whole HTTP response differs from fixture')
        return {'response_body_bytes': len(raw), 'response_body_sha256': hashlib.sha256(raw).hexdigest()}
    finally:
        connection.close()



def validate_host(record):
    for path, digest in record['files'].items():
        if golden.sha(path) != digest: raise ValueError('A consumed host file or its snapshot changed')


def capture_host(folder, intended, destination, stamp):
    """Guard the actual served bytes, then keep an immutable per-cell copy."""
    for name in ('kiosk.js', 'kiosk.wasm'):
        digest = stamp['artifacts'].get(str(intended/name))
        if not digest or golden.sha(folder/name) != digest:
            raise ValueError('The actual served module differs from its intended build')
    destination.mkdir(exist_ok=False)
    record = {'intended': str(intended), 'files': {}, 'saved': {}}
    for path in folder.iterdir():
        if path.is_file():
            saved = destination/path.name; shutil.copy2(path, saved); saved.chmod(0o600)
            record['files'].update({str(path): golden.sha(path), str(saved): golden.sha(saved)})
            record['saved'][str(path)] = str(saved)
    validate_host(record)
    return record


def run(args):
    directory = WORK/args.label; stamp = fresh(directory)
    output = directory/args.output; output.mkdir(exist_ok=False)
    result = {'source': stamp['source'], 'build': str(directory/'build.json'),
              'build_sha256': golden.sha(directory/'build.json'), 'options': vars(args),
              'start': context(), 'rows': [], 'complete': False,
              'interpretation': 'Whole production HTTP request. CPU samples include idle and diagnostic inspector overhead; they are not component wall times.'}
    try:
        for round_ in range(args.rounds):
            for workload in args.workloads.split(','):
                for target in args.targets.split(','):
                    configure(directory)
                    focused = workload != 'golden-read'
                    if focused:
                        import todo
                        todo.WORK = directory/'focused'; spike.WORK = todo.WORK
                        spike.EXTRA_ENV = {'SIGNING_KEY': todo.KEY}
                        spike.EXTRA_BINDINGS = ['(name = "SIGNING_KEY", text = '+json.dumps(todo.KEY)+')']
                        harness = todo
                    else:
                        harness = golden
                    work = harness.WORK
                    logs = work/'logs'; logs.mkdir(exist_ok=True)
                    durations = args.postgres_durations == 'on'
                    database = harness.Database(diagnostics=durations)
                    server = observer = inspector = None
                    row = {'round': round_+1, 'workload': workload, 'target': target,
                           'instrumentation': args.profile if target == 'workers' else 'none', 'verified': False}
                    result['rows'].append(row)
                    try:
                        check_duration_setting(database, durations)
                        database.prepare(target)
                        inspector_port = spike.free_port() if target == 'workers' and args.profile != 'none' else None
                        # Use an isolated named copy in the generated workerd config;
                        # never replace the production output or its build stamp.
                        original_config = spike.workerd_config
                        def configuration(*a, **kw):
                            path = original_config(*a, **kw)
                            if args.profile == 'named':
                                for name in ('kiosk.js', 'kiosk.wasm'):
                                    shutil.copy2(directory/('named-focused' if focused else 'named-golden')/name, path/name)
                            intended = directory/('named-focused' if focused else 'named-golden') if args.profile == 'named' else work/'workers/build/target/kiosk'
                            row['consumed_host'] = capture_host(path, intended, output/f'{len(result["rows"]):03d}-host', stamp)
                            return path
                        spike.workerd_config = configuration
                        try:
                            if focused:
                                server = harness.Server(target, database, logs, inspector_port)
                                route = 'read' if workload == 'ascii-small' else 'large'
                                payload = payloads()[workload]
                            else:
                                environment = golden.configuration(database.url(target), secrets.token_urlsafe(32))
                                owners, signed_at = golden.credentials(target, environment, args.principals)
                                golden.seed(database, target, owners)
                                server = golden.Server(target, database, environment, inspector_port)
                                cases = golden.cases_for(server, owners, 'read')
                                if time.time()*1000-signed_at+(args.warmup+args.duration+45)*1000 >= 230000:
                                    raise ValueError('Golden credentials cannot cover this request window')
                        finally:
                            spike.workerd_config = original_config
                        if target == 'workers': validate_host(row['consumed_host'])
                        row.update(cold_ms=server.cold_ms, first_ms=server.first_ms, idle_rss_mb=server.idle_rss_mb,
                                   pid=server.process.pid, command=server.process.args,
                                   helper_pids=[p.pid for p in server.helpers])
                        if inspector_port:
                            inspector = Inspector(inspector_port, output/f'{len(result["rows"]):03d}-{workload}.cpuprofile')
                        observer = Window(inspector, spike.WORK/'pg.log', durations=durations)
                        if focused:
                            harness.measure(server, database, route, args.connections, args.warmup, args.duration,
                                            args.principals, 16384, row, observer=observer, payload=payload)
                            identity, title = (harness.SMALL, 'small') if route == 'read' else (harness.LARGE, 'large')
                            row.update(payload_scalars=len(payload), payload_utf8_bytes=len(payload.encode()),
                                       payload_sha256=hashlib.sha256(payload.encode()).hexdigest())
                            row.update(wire_probe(server, '/api/samples/'+identity,
                                                  {'id': identity, 'title': title, 'payload': payload}))
                        else:
                            golden.measure(server, database, owners, cases, 'read', args.connections,
                                           args.warmup, args.duration, row, observer=observer)
                            row.update(wire_probe(server, cases[0]['path'], cases[0]['expected_json'], owners[0]['token']))
                        row['database_observation'] = observer.observation
                        if inspector:
                            row['profile'] = {'path': str(inspector.output), 'sha256': golden.sha(inspector.output),
                                'identity': inspector.identity, 'started': inspector.started, 'ended': inspector.ended,
                                'summary': summary(inspector.profile),
                                'window_scope': 'Profiler begins before load spawn; includes warmup, measured admission, drain and stop-command overhead. CPU counters exclude warmup and are captured before profile transfer/validation.'}
                        if target == 'workers': validate_host(row['consumed_host'])
                    except BaseException as error:
                        row.update(verified=False, failure=str(error)); raise
                    finally:
                        if observer: observer.close()
                        if server: server.stop()
                        database.stop()
                    fresh(directory)
                    print(f'  verified: {workload} {target} round {round_+1}', file=sys.stderr, flush=True)
        fresh(directory); result['complete'] = True
    except BaseException as error:
        result['failure'] = str(error); raise
    finally:
        result['end'] = context()
        (output/'results.json').write_text(json.dumps(result, indent=2)+'\n')
    print(f'passed: whole-request profiles · {len(result["rows"])} cells · {output}')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    prepare = commands.add_parser('build'); prepare.add_argument('--label', required=True)
    measure = commands.add_parser('run'); measure.add_argument('--label', required=True)
    measure.add_argument('--output', required=True)
    measure.add_argument('--profile', choices=('production', 'named', 'none'), default='production')
    measure.add_argument('--postgres-durations', choices=('on', 'off'), default='on',
                         help='owned PostgreSQL duration logging; off for capacity comparisons')
    measure.add_argument('--targets', default='workers')
    measure.add_argument('--workloads', default='golden-read,'+','.join(payloads()))
    measure.add_argument('--connections', type=int, default=16)
    measure.add_argument('--rounds', type=int, default=3)
    measure.add_argument('--duration', type=float, default=8)
    measure.add_argument('--warmup', type=float, default=2)
    measure.add_argument('--principals', type=int, default=2048)
    args = parser.parse_args(argv)
    if not args.label or not all(c.isalnum() or c in '-_' for c in args.label): parser.error('Use a simple immutable build label')
    if args.command == 'build': build(args.label); return
    targets, workloads = args.targets.split(','), args.workloads.split(',')
    if (not set(targets) <= {'workers', 'bun', 'native'} or len(targets) != len(set(targets))
            or not set(workloads) <= {'golden-read', *payloads()} or len(workloads) != len(set(workloads))
            or args.rounds < 1 or args.connections < 1 or not 1 <= args.principals <= 4096
            or not math.isfinite(args.duration) or not math.isfinite(args.warmup)
            or args.duration <= 0 or args.warmup < 0 or args.warmup+args.duration > 120
            or Path(args.output).name != args.output or args.output in ('', '.', '..')):
        parser.error('Use unique supported cells, a new output name and a finite window up to 120 seconds')
    run(args)


if __name__ == '__main__':
    try: main()
    except (ValueError, OSError, EOFError, subprocess.SubprocessError) as error:
        print('error: Whole-request profiling failed', file=sys.stderr)
        print(f'  reason: {error}', file=sys.stderr)
        print('  next: Inspect the retained run and choose a fresh output name.', file=sys.stderr)
        sys.exit(1)
