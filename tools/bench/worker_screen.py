#!/usr/bin/env python3
"""Short, explicitly unqualified production-Worker profile experiments.

Reuse the qualified harness's request/state/security controls without claiming
complete contract qualification. No native build, npm install or named diagnostic binding.
"""
from __future__ import annotations

import argparse
from datetime import datetime
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import time
import zlib

import ai_accounting as guard
import golden
import request_profile as qualified
import run as spike
from contracts import validate_measurement
from connected_deliveries import CARGO_WRAPPER
from version_writes import FLAGS, selected_flags

ROOT = spike.ROOT
WORK = ROOT/'tools/bench/work/worker-screen'
LEVELS = ('s', '2', '3')
WORKLOADS = ('golden-read', 'ascii-large', 'unicode-large')
QUALIFICATION = 'exploratory; no full contract qualification'
# Reused artifacts must run the same application/schema/security/load contract.
REFERENCE_PREFIXES = ('Cargo.toml', 'Cargo.lock', 'tools/bench/request_profile.py',
    'tools/bench/golden.py', 'tools/bench/golden_host.mjs', 'tools/bench/todo.py',
    'tools/bench/run.py', 'tools/bench/contracts.py', 'tools/bench/golden_contracts.py',
    'tools/bench/ai_accounting.py', 'tools/bench/version_writes.py',
    'tools/bench/connected_deliveries.py', 'tools/bench/fixtures/',
    'tools/bench/load/', 'tools/bench/typescript/', 'tools/golden-todo/',
    'examples/golden-todo/', 'tools/workers/', 'crates/jadpo-check/',
    'crates/jadpo-plan/', 'crates/jadpo-sql/', 'crates/jadpo-codegen/', 'crates/jadpo-cli/')


def selected_levels(levels):
    if not levels or len(set(levels)) != len(levels) or set(levels)-set(LEVELS):
        raise ValueError('Unknown or duplicate optimization levels')
    return tuple(levels)


def reference_inputs(reference, current):
    if reference.get('complete') is not True:
        raise ValueError('Historical reference is incomplete')
    golden.validate_conformance(reference.get('golden', {}).get('conformance', []), golden.contract_ids())
    producer = reference.get('source', {})
    if not producer.get('head') or not producer.get('tree'):
        raise ValueError('Historical producer identity missing')
    for prefix in REFERENCE_PREFIXES:
        old = {k:v for k,v in producer.get('files', {}).items() if k.startswith(prefix)}
        now = {k:v for k,v in current.get('files', {}).items() if k.startswith(prefix)}
        if not old or old != now:
            raise ValueError(f'Reused reference input differs: {prefix}')


def reference_files(reference, manifest, served=None):
    files = {}
    for app in ('golden', 'focused'):
        for name in ('kiosk.wasm', 'kiosk.js', 'worker.mjs', 'wrangler.json'):
            original = str(Path(manifest).parent/app/'workers/build/target/kiosk'/name)
            saved = reference.get('saved', {}).get(original)
            digest = reference.get('artifacts', {}).get(original)
            if name == 'wrangler.json' and not saved and served is not None:
                saved, digest, original = served_config(reference, manifest, served, app)
                files[(app, name)] = {'original': original, 'saved': saved,
                                     'sha256': digest, 'origin': 'immutable served snapshot'}
                continue
            if (not saved or not digest or reference['artifacts'].get(saved) != digest
                    or golden.sha(saved) != digest):
                raise ValueError('Historical immutable saved artifact is missing or changed')
            files[(app, name)] = {'original': original, 'saved': saved, 'sha256': digest}
    return files


def served_config(reference, manifest, served, app):
    if (served.get('complete') is not True or served.get('source') != reference['source']
            or served.get('build_sha256') != golden.sha(manifest)
            or Path(served.get('build', '')).resolve() != Path(manifest).resolve()):
        raise ValueError('Historical served results do not link to the qualified build')
    intended = str(Path(manifest).parent/app/'workers/build/target/kiosk')
    for row in served.get('rows', []):
        host = row.get('consumed_host', {})
        if row.get('target') != 'workers' or host.get('intended') != intended:
            continue
        validate_row(row)
        if row.get('instrumentation') != 'none':
            raise ValueError('Historical served config must come from the capacity protocol')
        names = {}
        for original, saved in host.get('saved', {}).items():
            name = Path(original).name
            if name not in ('kiosk.wasm', 'kiosk.js', 'worker.mjs', 'wrangler.json'):
                continue
            digest = host.get('files', {}).get(original)
            if not digest or host['files'].get(saved) != digest or golden.sha(saved) != digest:
                raise ValueError('Historical served snapshot changed')
            if name != 'wrangler.json' and reference['artifacts'].get(str(Path(intended)/name)) != digest:
                raise ValueError('Historical served module differs from its producer')
            names[name] = (saved, digest, original)
        if set(names) != {'kiosk.wasm', 'kiosk.js', 'worker.mjs', 'wrangler.json'}:
            raise ValueError('Historical served snapshot is incomplete')
        return names['wrangler.json']
    raise ValueError('No historical served configuration for this application')


def runner_config(path):
    config = json.loads(Path(path).read_text())
    return {key: config[key] for key in ('main', 'compatibility_date', 'rules', 'ratelimits')}


def validate_reference(record, current):
    path = Path(record['manifest'])
    if golden.sha(path) != record['manifest_sha256']:
        raise ValueError('Historical manifest changed')
    reference = json.loads(path.read_text())
    reference_inputs(reference, current)
    served = None
    if record.get('served_results'):
        metadata = record['served_results']
        if golden.sha(metadata['path']) != metadata['sha256']:
            raise ValueError('Historical served result manifest changed')
        served = json.loads(Path(metadata['path']).read_text())
    files = reference_files(reference, path, served)
    if record['producer'] != reference['source'] or record['historical_conformance'] != reference['golden']['conformance']:
        raise ValueError('Historical qualification or producer changed')
    for (app, name), identity in files.items():
        copied = record['apps'][app][name]
        if copied['origin'] != identity or golden.sha(copied['path']) != identity['sha256']:
            raise ValueError('Imported reference copy changed')



def sleep_events(raw, start, end):
    events = []
    for line in raw.splitlines():
        if not re.match(r'^\d{4}-\d\d-\d\d .*\b(?:Sleep|DarkWake)\s', line):
            continue
        when = datetime.strptime(line[:25], '%Y-%m-%d %H:%M:%S %z').timestamp()
        if start <= when <= end: events.append(line)
    return events


def check_sleep(start, end):
    if sys.platform != 'darwin':
        return {'available': False, 'scope': 'No macOS pmset sleep-event check on this host'}
    raw = subprocess.check_output(['pmset', '-g', 'log'], text=True)
    events = sleep_events(raw, start, end)
    if events: raise ValueError(f'Sleep/DarkWake during screen: {events}')
    return {'available': True, 'command': ['pmset', '-g', 'log'],
            'start_utc': start, 'end_utc': end, 'events': events}


def profile_flags(level):
    if level not in LEVELS:
        raise ValueError('Unknown optimization level')
    return {'CARGO_PROFILE_RELEASE_OPT_LEVEL': level,
            'CARGO_PROFILE_RELEASE_LTO': 'fat',
            'CARGO_PROFILE_RELEASE_CODEGEN_UNITS': '1'}


def schedule(workloads=WORKLOADS, levels=LEVELS, reference=False, rounds=2):
    if not workloads or len(set(workloads)) != len(workloads) or set(workloads)-set(WORKLOADS):
        raise ValueError('Unknown or duplicate screen workload')
    levels = selected_levels(levels)
    if rounds not in (1, 2, 3):
        raise ValueError('Screen rounds must be one, two or three')
    variants = (*(('reference',) if reference else ()), levels[0], 'bun', *levels[1:])
    first = [{'workload': workload, 'variant': variant}
             for workload in workloads for variant in variants]
    return [dict(cell, round=round_+1)
            for round_ in range(rounds)
            for cell in (list(reversed(first)) if round_ % 2 else first)]


def dependencies(folder):
    return {str(p.relative_to(folder)): {
        'sha256': golden.sha(p), 'symlink': os.readlink(p) if p.is_symlink() else None}
        for p in sorted(folder.rglob('*')) if p.is_file()}


def validate_manifest(stamp, current):
    levels = selected_levels(stamp.get('profile_levels', LEVELS))
    if (stamp.get('schemaVersion') != 1 or stamp.get('kind') != 'worker-screen'
            or stamp.get('qualification') != QUALIFICATION
            or stamp.get('complete') is not True or 'completeContract' in stamp
            or set(stamp.get('profiles', {})) != set(levels)):
        raise ValueError('Incomplete or incorrectly qualified experimental manifest')
    guard.validate(stamp, current)
    if stamp.get('reference'):
        validate_reference(stamp['reference'], current)
    for root, expected in stamp.get('dependencies', {}).items():
        if dependencies(Path(root)) != expected:
            raise ValueError('Cached TypeScript dependencies changed')
    generated = {}
    for level, profile in stamp['profiles'].items():
        cargo = [entry for entry in profile.get('cargo', [])
                 if 'build' in entry.get('argv', []) and '--release' in entry['argv']
                 and 'wasm32-unknown-unknown' in entry['argv']]
        if not cargo or any(any(entry['effective_flags'].get(k) != v
                                for k, v in profile_flags(level).items()) for entry in cargo):
            raise ValueError('Effective Worker Cargo profile differs from the experiment')
        if set(profile.get('apps', {})) != {'golden', 'focused'}:
            raise ValueError('Worker application missing')
        for app, record in profile['apps'].items():
            if not any(entry.get('cwd') == record.get('cargo_cwd') for entry in cargo):
                raise ValueError('Worker application has no matching effective Cargo invocation')
            inputs = record.get('generated_sources')
            if not inputs or app in generated and generated[app] != inputs:
                raise ValueError('Generated source differs between optimization levels')
            generated[app] = inputs
            if stamp.get('reference'):
                historical_config = stamp['reference']['apps'][app]['wrangler.json']['path']
                if runner_config(Path(record['production'])/'wrangler.json') != runner_config(historical_config):
                    raise ValueError('Historical and candidate runner configuration differs')
            folder = Path(record['production'])
            for name in ('kiosk.wasm', 'kiosk.js'):
                path = folder/name
                if str(path) not in stamp['artifacts']:
                    raise ValueError('Production artifact is not in the immutable manifest')
            wasm = (folder/'kiosk.wasm').read_bytes()
            qualified.wasm_sections(wasm)
            if (record.get('wasm_sha256') != hashlib.sha256(wasm).hexdigest()
                    or record.get('wasm_bytes') != len(wasm)
                    or record.get('gzip_default6_bytes') != len(gzip.compress(wasm, compresslevel=6, mtime=0))
                    or record.get('gzip_level9_bytes') != len(gzip.compress(wasm, compresslevel=9, mtime=0))):
                raise ValueError('Production Wasm identity or size differs from its profile record')


def validate_row(row):
    validate_measurement(row)
    database = row.get('database_observation', {})
    if (row.get('verified') is not True or row.get('persisted_delta') != 0
            or database.get('enabled') is not False
            or database.get('server_duration_records') != 0):
        raise ValueError('Screen state or PostgreSQL duration guard failed')
    delta = row['load_completed_unix']-row['load_started_unix']-row['load_wall_s']
    if abs(delta) >= 1:
        raise ValueError('UTC and load-generator clocks differ by one second or more')
    if (row.get('response_body_bytes', 0) <= 0
            or not re.fullmatch('[0-9a-f]{64}', row.get('response_body_sha256', ''))):
        raise ValueError('Whole-response byte identity missing')


def post_guard(stamp, current, rows):
    # Earlier served originals are regenerated per cell; their immutable saved
    # copies are the historical evidence. Any final drift faults the whole run.
    validate_manifest(stamp, current)
    for row in rows:
        host = row.get('consumed_host')
        if host:
            for saved in host['saved'].values():
                if golden.sha(saved) != host['files'][saved]:
                    raise ValueError('A saved served Worker snapshot changed')


def snapshot(path, directory, stamp):
    path = Path(path)
    saved = directory/'artifacts'/f'{len(stamp["saved"]):05d}-{path.name}'
    saved.parent.mkdir(exist_ok=True)
    shutil.copy2(path, saved)
    stamp['artifacts'].update({str(path): golden.sha(path), str(saved): golden.sha(saved)})
    stamp['saved'][str(path)] = str(saved)


def focused_application():
    import todo
    application = (todo.FIXTURE/'app.jadpo').read_text()
    application = application.replace('connections: {',
        'connections: { limits: { driver: Driver.cloudflare_rate_limits namespace: 1001 }', 1)
    application = re.sub(r'client_address:\s*\{[^}]+\}',
                        'client_address: { source: ClientAddressSource.cloudflare }', application)
    return application.replace('application {', '''application {
    rate_limits: { store: limits }
    deployables: { kiosk: { serves: [main] target: Target.wasm } }''', 1)


def copy_project(app, destination, workers):
    import todo
    source = golden.PROJECT if app == 'golden' else todo.FIXTURE
    shutil.copytree(source, destination, dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns('typescript', 'build', '.jadpo'))
    if app == 'golden':
        application = golden.application_for('workers' if workers else 'native')
    else:
        application = focused_application() if workers else (source/'app.jadpo').read_text()
    (destination/'app.jadpo').write_text(application)


def build(label, tools_from, levels=LEVELS, reference_build=None, reference_results=None):
    levels = selected_levels(levels)
    directory = WORK/label
    directory.mkdir(parents=True, exist_ok=False)
    before = guard.source()
    reference_path = Path(tools_from) if Path(tools_from).is_absolute() else qualified.WORK/tools_from/'build.json'
    reference = json.loads(reference_path.read_text())
    if reference.get('complete') is not True:
        raise ValueError('Cached tool reference is incomplete')
    for prefix in ('tools/bench/load/', 'tools/bench/typescript/',
                   'examples/golden-todo/typescript/', 'tools/workers/'):
        old = {k:v for k,v in reference['source']['files'].items() if k.startswith(prefix)}
        now = {k:v for k,v in before['files'].items() if k.startswith(prefix)}
        if old != now:
            raise ValueError('A reused tool or TypeScript source changed')
    if any(k.startswith('CARGO_PROFILE_RELEASE_') for k in os.environ):
        raise ValueError('Unset inherited release overrides; the screen defines all profiles')
    stamp = {'schemaVersion': 1, 'kind': 'worker-screen', 'qualification': QUALIFICATION,
             'source': before, 'complete': False, 'profiles': {}, 'profile_levels': levels, 'artifacts': {}, 'saved': {},
             'tools_reference': str(reference_path), 'tools_reference_sha256': golden.sha(reference_path),
             'compression': {'default_level': 6, 'optional_level': 9,
                             'zlib_runtime': zlib.ZLIB_RUNTIME_VERSION},
             'inherited_flags': selected_flags(os.environ),
             'build_environment': {'fixed': {**FLAGS, 'SOURCE_DATE_EPOCH': '0'},
                                   'XDG_CACHE_HOME': os.environ.get('XDG_CACHE_HOME')},
             'start': qualified.context()}
    manifest = directory/'build.json'
    manifest.write_text(json.dumps(stamp, indent=2)+'\n')
    try:
        environment = {**os.environ, **FLAGS, 'SOURCE_DATE_EPOCH': '0'}
        subprocess.run(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '--bin', 'jadpo'],
                       cwd=ROOT, env=environment, check=True)
        cached_workerd = next((entry/'bin/workerd' for entry in sorted(spike.WORKERD.glob('workerd-*')) if (entry/'bin/workerd').is_file()), Path('/missing-workerd'))
        if not cached_workerd.is_file():
            raise ValueError('Cached pinned workerd is missing; no installation is attempted')
        signing_helper = ROOT/'target/jadpo-compiled/golden-todo/native/debug/golden-native-contract'
        binaries = [spike.LOAD, spike.POOL, cached_workerd, signing_helper,
                    *[Path(spike.tool(name)).resolve() for name in ('bun', 'node', 'psql')]]
        reference_root = reference_path.parents[5]
        stamp['reused_tools'] = {}
        for binary in binaries:
            original = reference_root/binary.relative_to(ROOT) if binary.is_relative_to(ROOT) else binary
            if reference['artifacts'].get(str(original)) != golden.sha(binary):
                raise ValueError('A reused executable differs from the qualified tool reference')
            stamp['reused_tools'][str(binary)] = {'original': str(original), 'sha256': golden.sha(binary)}
        selected = {name: subprocess.check_output(['rustup', 'which', name], text=True).strip()
                    for name in ('cargo', 'rustc', 'rustdoc')}
        stamp['compiler_selection'] = {
            'requested': {k: environment.get(k) for k in ('RUSTUP_TOOLCHAIN', 'RUSTC', 'RUSTDOC')},
            'rustup_which': selected,
            'scope': 'Requested selectors and Rustup resolution, not a child executable trace.'}
        stamp['dependencies'] = {}
        for root in (ROOT/'tools/bench/typescript/node_modules',
                     golden.TS/'node_modules'):
            if not root.is_dir():
                raise ValueError('Cached TypeScript dependencies are missing')
            stamp['dependencies'][str(root)] = dependencies(root)
        # Bun consumes emitted JavaScript, which is distinct from tracked TS.
        for root in (ROOT/'tools/bench/typescript/build', golden.TS/'build'):
            scripts = sorted(root.rglob('*.js'))
            if not scripts: raise ValueError('Cached TypeScript output is missing')
            for script in scripts:
                original = str(reference_root/script.relative_to(ROOT))
                saved = reference.get('saved', {}).get(original)
                digest = reference['artifacts'].get(original)
                if (not saved or not digest or reference['artifacts'].get(saved) != digest
                        or golden.sha(saved) != digest or golden.sha(script) != digest):
                    raise ValueError('Reused TypeScript output differs from its immutable reference')
                snapshot(script, directory, stamp)
        stamp['database_prepare_compiler'] = {'path': str(spike.JADPO), 'sha256': golden.sha(spike.JADPO), 'source': before, 'scope': 'Current compiler applies unchanged schema; reference Worker remains historical'}
        stamp['runtimes'] = {name: dict(metadata) for name, metadata in reference['runtimes'].items()}
        stamp['runtimes']['workerd']['path'] = str(cached_workerd)
        for binary in dict.fromkeys([*binaries, spike.JADPO, *map(Path, selected.values()),
                *[Path(environment[k]) for k in ('RUSTC', 'RUSTDOC') if environment.get(k)]]):
            snapshot(binary, directory, stamp)
        for root in stamp['dependencies']:
            for relative in stamp['dependencies'][root]:
                snapshot(Path(root)/relative, directory, stamp)
        for app in ('golden', 'focused'):
            copy_project(app, directory/'projects'/app, True)
        if reference_build:
            origin = Path(reference_build)
            historical = json.loads(origin.read_text())
            reference_inputs(historical, before)
            record = {'manifest': str(origin), 'manifest_sha256': golden.sha(origin),
                      'producer': historical['source'],
                      'historical_conformance': historical['golden']['conformance'],
                      'qualification_scope': 'Historical producer only; this screen remains exploratory',
                      'apps': {'golden': {}, 'focused': {}}}
            served = None
            if reference_results:
                served = json.loads(Path(reference_results).read_text())
                record['served_results'] = {'path': str(reference_results),
                                            'sha256': golden.sha(reference_results)}
            for (app, name), identity in reference_files(historical, origin, served).items():
                copied = directory/'reference'/app/'workers/build/target/kiosk'/name
                copied.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(identity['saved'], copied)
                snapshot(copied, directory, stamp)
                record['apps'][app][name] = {'origin': identity, 'path': str(copied)}
            for app in ('golden', 'focused'):
                copy_project(app, directory/'reference'/app/'native', False)
                for path in (directory/'reference'/app/'native').rglob('*'):
                    if path.is_file(): snapshot(path, directory, stamp)
            stamp['reference'] = record
        for level in levels:
            available = shutil.disk_usage(directory).free
            if available < 4*1024**3:
                raise ValueError('Less than 4 GiB available before the next Worker profile build')
            variant = directory/level
            wrapper = variant/'capture-cargo'
            wrapper.parent.mkdir()
            wrapper.write_text(CARGO_WRAPPER); wrapper.chmod(0o755)
            provenance = variant/'cargo-provenance.jsonl'
            child = {**environment, **profile_flags(level), 'CARGO': str(wrapper),
                     'JADPO_BENCH_CARGO_REAL': selected['cargo'],
                     'JADPO_BENCH_CARGO_LOG': str(provenance)}
            profile = {'apps': {}, 'requested_profile': profile_flags(level),
                       'available_before_bytes': available}
            stamp['profiles'][level] = profile
            for app in ('golden', 'focused'):
                project = directory/'projects'/app
                subprocess.run([str(spike.JADPO), 'check'], cwd=project,
                               env=child, check=True)
                subprocess.run([str(spike.JADPO), 'build'], cwd=project,
                               env=child, check=True)
                production = variant/app/'workers/build/target/kiosk'
                shutil.copytree(project/'build/target/kiosk', production)
                copy_project(app, variant/app/'native', False)
                generated = {}
                for path in sorted((project/'build/rust').rglob('*')):
                    if not path.is_file():
                        continue
                    relative = path.relative_to(project/'build/rust')
                    saved = variant/app/'generated'/relative
                    saved.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, saved)
                    # The unbound executable is an artifact, not generated source.
                    if path.suffix != '.wasm':
                        generated[str(relative)] = golden.sha(saved)
                    snapshot(saved, directory, stamp)
                for path in production.iterdir():
                    if path.is_file():
                        snapshot(path, directory, stamp)
                for path in (variant/app/'native').rglob('*'):
                    if path.is_file():
                        snapshot(path, directory, stamp)
                wasm = (production/'kiosk.wasm').read_bytes()
                profile['apps'][app] = {
                    'production': str(production), 'generated_sources': generated,
                    'cargo_cwd': str(project/'build/rust/kiosk'),
                    'wasm_bytes': len(wasm),
                    'gzip_default6_bytes': len(gzip.compress(wasm, compresslevel=6, mtime=0)),
                    'gzip_level9_bytes': len(gzip.compress(wasm, compresslevel=9, mtime=0)),
                    'wasm_sha256': hashlib.sha256(wasm).hexdigest(),
                    'executable_sections_sha256': hashlib.sha256(
                        b''.join(qualified.wasm_sections(wasm))).hexdigest(),
                    'production_module_bytes': sum(p.stat().st_size for p in production.iterdir()
                        if p.suffix in ('.wasm', '.js', '.mjs')),
                    'module_gzip6_proxy_bytes': sum(len(gzip.compress(p.read_bytes(), compresslevel=6, mtime=0))
                        for p in production.iterdir() if p.suffix in ('.wasm', '.js', '.mjs')),
                    'module_gzip9_proxy_bytes': sum(len(gzip.compress(p.read_bytes(), compresslevel=9, mtime=0))
                        for p in production.iterdir() if p.suffix in ('.wasm', '.js', '.mjs')),
                    'compressed_size_scope': 'Module gzip sums (default level 6 and level 9); not a provider upload measurement.'}
            profile['cargo'] = [json.loads(line) for line in provenance.read_text().splitlines()]
            snapshot(wrapper, directory, stamp)
            snapshot(provenance, directory, stamp)
            profile['available_after_bytes'] = shutil.disk_usage(directory).free
            if guard.source() != before:
                raise ValueError('Source changed during experimental build')
        stamp.update(complete=True, end=qualified.context())
        validate_manifest(stamp, before)
    except BaseException as error:
        stamp.update(complete=False, failure=str(error), end=qualified.context())
        raise
    finally:
        manifest.write_text(json.dumps(stamp, indent=2)+'\n')
    print(f'passed: Worker screen build · experimental · {directory}')


def configure(variant, focused):
    qualified.configure(variant)
    if focused:
        import todo
        todo.WORK = variant/'focused'; spike.WORK = todo.WORK
        spike.EXTRA_ENV = {'SIGNING_KEY': todo.KEY}
        spike.EXTRA_BINDINGS = ['(name = "SIGNING_KEY", text = '+json.dumps(todo.KEY)+')']
        return todo
    return golden


def smoke(label, output):
    """No-load setup check of both imported production hosts; never timing."""
    directory = WORK/label
    manifest = directory/'build.json'
    stamp = json.loads(manifest.read_text())
    validate_manifest(stamp, guard.source())
    if not stamp.get('reference'): raise ValueError('Setup smoke requires a reference')
    destination = directory/output
    destination.mkdir(exist_ok=False)
    result = {'kind': 'worker-screen-setup', 'qualification': 'setup only; no performance measurement',
              'source': stamp['source'], 'build': str(manifest),
              'build_sha256': golden.sha(manifest), 'complete': False, 'rows': []}
    try:
        for app in ('golden', 'focused'):
            harness = configure(directory/'reference', app == 'focused')
            database = harness.Database(diagnostics=False)
            server = None
            original = spike.workerd_config
            row = {'application': app, 'verified': False}
            result['rows'].append(row)
            try:
                qualified.check_duration_setting(database, False)
                database.prepare('workers')
                def configuration(*args, **kwargs):
                    path = original(*args, **kwargs)
                    row['consumed_host'] = qualified.capture_host(path,
                        directory/'reference'/app/'workers/build/target/kiosk',
                        destination/f'{app}-host', stamp)
                    return path
                spike.workerd_config = configuration
                if app == 'golden':
                    environment = golden.configuration(database.url('workers'), secrets.token_urlsafe(32))
                    owners, _ = golden.credentials('workers', environment, 1)
                    golden.seed(database, 'workers', owners)
                    server = golden.Server('workers', database, environment)
                    case = golden.cases_for(server, owners, 'read')[0]
                    row.update(qualified.wire_probe(server, case['path'], case['expected_json'], owners[0]['token']))
                else:
                    logs = harness.WORK/'logs'; logs.mkdir(exist_ok=True)
                    server = harness.Server('workers', database, logs)
                    payload = qualified.payloads()['ascii-large']
                    harness.prepare(server, database, 1, {harness.SMALL: 'x'*100, harness.LARGE: payload})
                    row.update(qualified.wire_probe(server, '/api/samples/'+harness.LARGE,
                        {'id': harness.LARGE, 'title': 'large', 'payload': payload}))
                qualified.validate_host(row['consumed_host'])
                qualified.check_duration_setting(database, False)
                row['verified'] = True
            finally:
                spike.workerd_config = original
                if server: server.stop()
                database.stop()
        post_guard(stamp, guard.source(), result['rows'])
        result['complete'] = True
    except BaseException as error:
        result['failure'] = str(error)
        raise
    finally:
        (destination/'results.json').write_text(json.dumps(result, indent=2)+'\n')
    print(f'passed: no-load reference setup smoke · {destination}')



def qualification_source(producer, current):
    """Only the qualifier and its tests may differ from the timed producer."""
    allowed = {'tools/bench/worker_screen.py', 'tools/bench/test_worker_screen.py'}
    if not all(source.get('head') and source.get('tree') and source.get('files')
               for source in (producer, current)):
        raise ValueError('Qualification source identity missing')
    changed = {name for name in producer['files'].keys() | current['files'].keys()
               if producer['files'].get(name) != current['files'].get(name)}
    if changed-allowed:
        raise ValueError('Timed production source differs from qualification source')
    return sorted(changed)


def qualify(label, output):
    """Current-source 176 fallback cases plus exact timed production acceptance."""
    directory = WORK/label
    manifest = directory/'build.json'
    stamp = json.loads(manifest.read_text())
    current = guard.source()
    changed = qualification_source(stamp['source'], current)
    validate_manifest(stamp, stamp['source'])
    if stamp.get('profile_levels') != ['s']:
        raise ValueError('Selected qualification requires the shipping s profile')
    destination = directory/output
    destination.mkdir(exist_ok=False)
    result = {'kind': 'worker-selected-qualification', 'complete': False,
              'source': current, 'timed_producer': stamp['source'],
              'compatible_harness_changes': changed, 'build': str(manifest),
              'build_sha256': golden.sha(manifest), 'start': qualified.context(), 'rows': [],
              'scope': '176 current-source contract cases use test controls and ordinary recorded fallback; separate production acceptance consumes exact timed Worker modules, without test hooks or operation counts.'}
    try:
        for app in ('golden', 'focused'):
            harness = configure(directory/'s', app == 'focused')
            database = harness.Database(diagnostics=False)
            server = None
            original = spike.workerd_config
            row = {'application': app, 'verified': False}
            result['rows'].append(row)
            try:
                qualified.check_duration_setting(database, False)
                database.prepare('workers')
                def configuration(*args, **kwargs):
                    path = original(*args, **kwargs)
                    row['consumed_host'] = qualified.capture_host(path,
                        directory/'s'/app/'workers/build/target/kiosk', destination/f'{app}-host', stamp)
                    return path
                spike.workerd_config = configuration
                if app == 'golden':
                    environment = golden.configuration(database.url('workers'), secrets.token_urlsafe(32))
                    owners, _ = golden.credentials('workers', environment, 2)
                    golden.seed(database, 'workers', owners)
                    before = database.state('workers')
                    server = golden.Server('workers', database, environment)
                    cases = golden.cases_for(server, owners, 'read')
                    listed = golden.cases_for(server, owners, 'signed-list')
                    row['owner_reads'] = [qualified.wire_probe(server, case['path'], case['expected_json'], owner['token'])
                                          for case, owner in zip(cases, owners)]
                    row['list'] = qualified.wire_probe(server, listed[0]['path'], listed[0]['expected_json'], owners[0]['token'])
                    expected = {'id': owners[0]['id'], 'todos': sorted(listed[0]['expected_json']['items'], key=lambda item: item['id'])}
                    row['include'] = qualified.wire_probe(server, '/users/'+owners[0]['id']+'/todos', expected, owners[0]['token'])
                    for path in (cases[1]['path'], '/todos/ffffffff-ffff-4fff-8fff-ffffffffffff'):
                        status, body = golden.request(server, 'GET', path, owners[0]['token'])
                        golden.validate_failure(status, body, 404, 'todo_not_found')
                    for token, code in ((None, 'authentication_required'), ('jadpo_token_'+'0'*64+'.invalid', 'invalid_credentials')):
                        status, body = golden.request(server, 'GET', cases[0]['path'], token)
                        golden.validate_failure(status, body, 401, code)
                    if before != database.state('workers'):
                        raise ValueError('Production read acceptance changed golden rows')
                    row['controls'] = ['owner bodies', 'foreign and missing concealed', 'absent and forged credentials', 'ordinary list and includes', 'unchanged rows']
                else:
                    logs = harness.WORK/'logs'; logs.mkdir(exist_ok=True)
                    server = harness.Server('workers', database, logs)
                    payload = qualified.payloads()['unicode-large']
                    harness.prepare(server, database, 1, {harness.SMALL: 'x'*100, harness.LARGE: payload})
                    before = database.query('workers', 'SELECT json_agg(s ORDER BY id) FROM sample s')
                    row.update(qualified.wire_probe(server, '/api/samples/'+harness.LARGE,
                               {'id': harness.LARGE, 'title': 'large', 'payload': payload}))
                    if before != database.query('workers', 'SELECT json_agg(s ORDER BY id) FROM sample s'):
                        raise ValueError('Production read acceptance changed sample rows')
                    row['controls'] = ['large canonical body', 'existing absent/forged/expired opaque credentials', 'signed owner isolation', 'persisted write and unchanged samples']
                qualified.validate_host(row['consumed_host'])
                qualified.check_duration_setting(database, False)
                row['verified'] = True
            finally:
                spike.workerd_config = original
                if server: server.stop()
                database.stop()
        # This checks the timed signing helper BEFORE conformance rebuilds it.
        qualification_source(stamp['source'], guard.source())
        post_guard(stamp, stamp['source'], result['rows'])
        result['timed_build_postguard_before_conformance'] = True
        result['conformance'] = golden.conformance()
        result['conformance_artifacts'] = {'artifacts': {}, 'saved': {}}
        artifacts = result['conformance_artifacts']
        for path in [ROOT/'target/jadpo-compiled/golden-todo/native/debug/golden-native-contract',
                     spike.JADPO, *sorted((ROOT/'tools/golden-todo/work/workers/bound').glob('*'))]:
            if path.is_file(): snapshot(path, destination, artifacts)
        # The conformance helper has its own identity; saved timed inputs and
        # actual served files remain guarded after its live path is rebuilt.
        for saved in stamp['saved'].values():
            if golden.sha(saved) != stamp['artifacts'][saved]:
                raise ValueError('Immutable timed input changed during conformance')
        for row in result['rows']: qualified.validate_host(row['consumed_host'])
        for profile in stamp['profiles'].values():
            for app in profile['apps'].values():
                for path in Path(app['production']).iterdir():
                    if path.is_file() and golden.sha(path) != stamp['artifacts'].get(str(path)):
                        raise ValueError('Timed production module changed during conformance')
        if guard.source() != current or golden.sha(manifest) != result['build_sha256']:
            raise ValueError('Qualification source or timed manifest changed')
        result['complete'] = True
    except BaseException as error:
        result['failure'] = str(error)
        raise
    finally:
        result['end'] = qualified.context()
        (destination/'results.json').write_text(json.dumps(result, indent=2)+'\n')
    print(f'passed: selected qualification · 176 fallback cases + exact timed production acceptance · {destination}')


def run(label, output, duration, warmup, principals, workloads=WORKLOADS, rounds=2):
    directory = WORK/label
    manifest = directory/'build.json'
    stamp = json.loads(manifest.read_text())
    validate_manifest(stamp, guard.source())
    destination = directory/output
    destination.mkdir(exist_ok=False)
    result = {'schemaVersion': 1, 'kind': 'worker-screen', 'qualification': QUALIFICATION,
              'source': stamp['source'], 'build': str(manifest),
              'build_sha256': golden.sha(manifest), 'complete': False,
              'protocol': {'connections': 16, 'duration_s': duration, 'warmup_s': warmup,
                           'inspector': 'none', 'postgres_durations': 'off',
                           'schedule': schedule(workloads, stamp.get('profile_levels', LEVELS), bool(stamp.get('reference')), rounds), 'rounds': rounds},
              'start': qualified.context(), 'rows': []}
    try:
        for cell in result['protocol']['schedule']:
            level = stamp.get('profile_levels', LEVELS)[0] if cell['variant'] == 'bun' else cell['variant']
            variant = directory/level
            focused = cell['workload'] != 'golden-read'
            harness = configure(variant, focused)
            target = 'bun' if cell['variant'] == 'bun' else 'workers'
            row = dict(cell, target=target, verified=False)
            result['rows'].append(row)
            database = harness.Database(diagnostics=False)
            server = observer = None
            original = spike.workerd_config
            try:
                qualified.check_duration_setting(database, False)
                database.prepare(target)
                def configuration(*args, **kwargs):
                    path = original(*args, **kwargs)
                    row['consumed_host'] = qualified.capture_host(
                        path, variant/('focused' if focused else 'golden')/'workers/build/target/kiosk',
                        destination/f'{len(result["rows"]):03d}-host', stamp)
                    return path
                spike.workerd_config = configuration
                if focused:
                    logs = harness.WORK/'logs'; logs.mkdir(exist_ok=True)
                    server = harness.Server(target, database, logs)
                else:
                    environment = golden.configuration(database.url(target), secrets.token_urlsafe(32))
                    owners, signed_at = golden.credentials(target, environment, principals)
                    golden.seed(database, target, owners)
                    server = golden.Server(target, database, environment)
                    cases = golden.cases_for(server, owners, 'read')
                    if time.time()*1000-signed_at+(warmup+duration+45)*1000 >= 230000:
                        raise ValueError('Golden credentials cannot cover this load window')
                spike.workerd_config = original
                if target == 'workers':
                    qualified.validate_host(row['consumed_host'])
                row.update(cold_ms=server.cold_ms, first_ms=server.first_ms,
                           idle_rss_mb=server.idle_rss_mb, pid=server.process.pid,
                           helper_pids=[p.pid for p in server.helpers], command=server.process.args)
                observer = qualified.Window(None, spike.WORK/'pg.log', durations=False)
                if focused:
                    payload = qualified.payloads()[cell['workload']]
                    harness.measure(server, database, 'large', 16, warmup, duration, principals,
                                    16384, row, observer=observer, payload=payload)
                    row.update(payload_scalars=len(payload), payload_utf8_bytes=len(payload.encode()),
                               payload_sha256=hashlib.sha256(payload.encode()).hexdigest())
                    row.update(qualified.wire_probe(server, '/api/samples/'+harness.LARGE,
                        {'id': harness.LARGE, 'title': 'large', 'payload': payload}))
                else:
                    golden.measure(server, database, owners, cases, 'read', 16, warmup, duration,
                                   row, observer=observer)
                    row.update(qualified.wire_probe(server, cases[0]['path'],
                                                   cases[0]['expected_json'], owners[0]['token']))
                row['database_observation'] = observer.observation
                validate_row(row)
                if target == 'workers':
                    qualified.validate_host(row['consumed_host'])
            except BaseException as error:
                row.update(verified=False, failure=str(error))
                raise
            finally:
                spike.workerd_config = original
                if observer:
                    observer.close()
                if server:
                    server.stop()
                database.stop()
                (destination/'results.json').write_text(json.dumps(result, indent=2)+'\n')
            if guard.source() != stamp['source']:
                raise ValueError('Source changed during the experimental campaign')
            print(f'  verified: {cell["workload"]} · {cell["variant"]} · round {cell["round"]}',
                  file=sys.stderr, flush=True)
        if golden.sha(manifest) != result['build_sha256']:
            raise ValueError('Experimental build manifest changed during the campaign')
        post_guard(stamp, guard.source(), result['rows'])
        result['sleep_observation'] = check_sleep(result['start']['utc'], time.time())
        result.update(complete=True)
    except BaseException as error:
        result['failure'] = str(error)
        raise
    finally:
        result['end'] = qualified.context()
        (destination/'results.json').write_text(json.dumps(result, indent=2)+'\n')
    print(f'passed: Worker screen · {len(result["rows"])} cells · experimental · {destination}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    build_parser = commands.add_parser('build')
    build_parser.add_argument('--label', required=True)
    build_parser.add_argument('--tools-from', required=True)
    build_parser.add_argument('--profiles', nargs='+', choices=LEVELS, default=LEVELS)
    build_parser.add_argument('--reference-build', type=Path)
    build_parser.add_argument('--reference-results', type=Path)
    smoke_parser = commands.add_parser('smoke')
    smoke_parser.add_argument('--label', required=True)
    smoke_parser.add_argument('--output', required=True)
    qualify_parser = commands.add_parser('qualify')
    qualify_parser.add_argument('--label', required=True)
    qualify_parser.add_argument('--output', required=True)
    run_parser = commands.add_parser('run')
    run_parser.add_argument('--label', required=True)
    run_parser.add_argument('--output', required=True)
    run_parser.add_argument('--duration', type=int, default=2)
    run_parser.add_argument('--warmup', type=int, default=1)
    run_parser.add_argument('--principals', type=int, default=2048)
    run_parser.add_argument('--workloads', nargs='+', choices=WORKLOADS, default=WORKLOADS)
    run_parser.add_argument('--rounds', type=int, choices=(1, 2, 3), default=2)
    args = parser.parse_args()
    if (not re.fullmatch('[a-z0-9][a-z0-9-]*', args.label)
            or args.command == 'build' and not Path(args.tools_from).is_absolute() and not re.fullmatch('[a-z0-9][a-z0-9-]*', args.tools_from)):
        parser.error('label must contain lower-case letters, digits and hyphens')
    if args.command == 'run' and (args.duration <= 0 or args.warmup < 0
                                  or args.principals < 2048
                                  or not re.fullmatch('[a-z0-9][a-z0-9-]*', args.output)):
        parser.error('invalid load window, credential pool or output name')
    if args.command in ('smoke', 'qualify') and not re.fullmatch('[a-z0-9][a-z0-9-]*', args.output):
        parser.error('invalid setup output name')
    if args.command == 'build' and args.reference_results and not args.reference_build:
        parser.error('reference results require a reference build')
    print(f'Worker screen · {args.command} · experimental', file=sys.stderr, flush=True)
    if args.command == 'build':
        build(args.label, args.tools_from, args.profiles, args.reference_build, args.reference_results)
    elif args.command == 'smoke':
        smoke(args.label, args.output)
    elif args.command == 'qualify':
        qualify(args.label, args.output)
    else:
        run(args.label, args.output, args.duration, args.warmup, args.principals, args.workloads, args.rounds)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'failed: Worker screen\n  reason: {error}\n'
              '  next: (agent) inspect the retained experimental manifest or results', file=sys.stderr)
        sys.exit(1)
