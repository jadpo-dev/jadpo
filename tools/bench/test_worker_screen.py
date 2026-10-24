"""Fail-closed controls for the explicitly unqualified Worker screen."""
import copy
import hashlib
import gzip
import tempfile
import unittest
from pathlib import Path

from worker_screen import profile_flags, schedule, validate_manifest, validate_row, post_guard


class WorkerScreenControls(unittest.TestCase):
    def manifest(self, root):
        stamp = {'schemaVersion': 1, 'kind': 'worker-screen',
                 'qualification': 'exploratory; no full contract qualification',
                 'source': {'head': 'frozen'}, 'complete': True,
                 'artifacts': {}, 'profiles': {}}
        for level in ('s', '2', '3'):
            apps = {}
            for app in ('golden', 'focused'):
                folder = root / level / app
                folder.mkdir(parents=True)
                (folder/'kiosk.wasm').write_bytes(b'\0asm\1\0\0\0')
                (folder/'kiosk.js').write_text('export {};\n')
                for path in folder.iterdir():
                    stamp['artifacts'][str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
                apps[app] = {'production': str(folder),
                             'generated_sources': {'lib.rs': 'same-generated-source'},
                             'cargo_cwd': str(folder/'generated'),
                             'wasm_sha256': hashlib.sha256((folder/'kiosk.wasm').read_bytes()).hexdigest(),
                             'wasm_bytes': 8,
                             'gzip_default6_bytes': len(gzip.compress(
                                 (folder/'kiosk.wasm').read_bytes(), compresslevel=6, mtime=0)),
                             'gzip_level9_bytes': len(gzip.compress(
                                 (folder/'kiosk.wasm').read_bytes(), compresslevel=9, mtime=0))}
            stamp['profiles'][level] = {'apps': apps, 'cargo': [{
                'argv': ['build', '--release', '--target', 'wasm32-unknown-unknown'],
                'cwd': record['cargo_cwd'], 'effective_flags': profile_flags(level)}
                for record in apps.values()]}
        return stamp

    def test_selected_qualification_allows_only_explicit_harness_changes(self):
        from worker_screen import qualification_source
        source = {'head': 'timed', 'tree': 'tree', 'files': {
            'crates/jadpo-runtime/src/store.rs': {'mode': '100644', 'sha256': 'a'},
            'tools/bench/worker_screen.py': {'mode': '100644', 'sha256': 'b'}}}
        current = copy.deepcopy(source)
        current.update(head='qualifier', tree='newtree')
        current['files']['tools/bench/worker_screen.py']['sha256'] = 'qualifier'
        self.assertEqual(qualification_source(source, current), ['tools/bench/worker_screen.py'])
        for change in (lambda s: s['files']['crates/jadpo-runtime/src/store.rs'].update(sha256='drift'),
                       lambda s: s['files']['crates/jadpo-runtime/src/store.rs'].update(mode='100755'),
                       lambda s: s['files'].update({'new.rs': {'mode': '100644', 'sha256': 'new'}}),
                       lambda s: s['files'].pop('crates/jadpo-runtime/src/store.rs'),
                       lambda s: s.update(tree='')):
            bad = copy.deepcopy(current); change(bad)
            with self.assertRaises(ValueError): qualification_source(source, bad)

    def test_profile_matrix_and_source_artifacts_fail_closed(self):
        with tempfile.TemporaryDirectory() as raw:
            stamp = self.manifest(Path(raw))
            validate_manifest(stamp, stamp['source'])
            faults = [
                lambda s: s.update(complete=False),
                lambda s: s.update(completeContract=True),
                lambda s: s['profiles'].pop('2'),
                lambda s: s['profiles']['3']['apps']['focused']['generated_sources'].update(
                    {'lib.rs': 'different-generated-source'}),
                lambda s: s['profiles']['2']['cargo'][0]['effective_flags'].update(
                    CARGO_PROFILE_RELEASE_OPT_LEVEL='3'),
                lambda s: s['profiles']['s']['cargo'][0]['effective_flags'].update(
                    CARGO_PROFILE_RELEASE_LTO='false'),
                lambda s: s['profiles']['3'].update(cargo=[]),
                lambda s: s['profiles']['2']['cargo'].pop(),
                lambda s: s['profiles']['2']['apps']['focused'].update(
                    wasm_sha256='0'*64),
                lambda s: s['profiles']['2']['apps']['focused'].update(
                    wasm_bytes=999),
                lambda s: s['profiles']['s']['apps']['golden'].update(
                    gzip_default6_bytes=999),
            ]
            for fault in faults:
                bad = copy.deepcopy(stamp)
                fault(bad)
                with self.subTest(fault=fault), self.assertRaises(ValueError):
                    validate_manifest(bad, stamp['source'])
            with self.assertRaises(ValueError):
                validate_manifest(stamp, {'head': 'other'})
            wasm = Path(stamp['profiles']['2']['apps']['focused']['production'])/'kiosk.wasm'
            wasm.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                validate_manifest(stamp, stamp['source'])
            stamp['artifacts'][str(wasm)] = hashlib.sha256(wasm.read_bytes()).hexdigest()
            with self.assertRaises(ValueError):
                validate_manifest(stamp, stamp['source'])

    def test_final_guard_rejects_late_snapshot_or_dependency_drift(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            stamp = self.manifest(root)
            dependency = root/'node_modules'
            dependency.mkdir()
            (dependency/'module.js').write_text('export {};')
            from worker_screen import dependencies
            stamp['dependencies'] = {str(dependency): dependencies(dependency)}
            saved = root/'served.wasm'
            saved.write_bytes(b'validated served bytes')
            rows = [{'consumed_host': {'saved': {'original': str(saved)},
                     'files': {str(saved): hashlib.sha256(saved.read_bytes()).hexdigest()}}}]
            post_guard(stamp, stamp['source'], rows)
            saved.write_bytes(b'changed after its cell passed')
            with self.assertRaises(ValueError):
                post_guard(stamp, stamp['source'], rows)
            (dependency/'new.js').write_text('new unrecorded dependency')
            with self.assertRaises(ValueError):
                post_guard(stamp, stamp['source'], [])

    def test_schedule_has_all_variants_and_reverses_the_order(self):
        rows = schedule(('golden-read', 'ascii-large', 'unicode-large'))
        self.assertEqual(len(rows), 24)
        first = rows[:12]
        second = rows[12:]
        self.assertEqual([(r['workload'], r['variant']) for r in second],
                         list(reversed([(r['workload'], r['variant']) for r in first])))
        for round_ in (1, 2):
            for workload in ('golden-read', 'ascii-large', 'unicode-large'):
                self.assertEqual({r['variant'] for r in rows
                                  if r['round'] == round_ and r['workload'] == workload},
                                 {'s', '2', '3', 'bun'})
        for bad in ((), ('unknown',), ('golden-read', 'golden-read')):
            with self.assertRaises(ValueError):
                schedule(bad)
        with self.assertRaises(ValueError):
            profile_flags('fast')

    def test_single_shipping_profile_and_historical_matrix_are_explicit(self):
        from worker_screen import selected_levels
        rows = schedule(levels=('s',), reference=True)
        self.assertEqual(len(rows), 18)
        self.assertEqual({row['variant'] for row in rows}, {'reference', 's', 'bun'})
        self.assertEqual([(row['workload'], row['variant']) for row in rows[9:]],
                         list(reversed([(row['workload'], row['variant']) for row in rows[:9]])))
        for bad in ((), ('s', 's'), ('fast',)):
            with self.assertRaises(ValueError): selected_levels(bad)
        with self.assertRaises(ValueError): schedule(rounds=0)
        with tempfile.TemporaryDirectory() as raw:
            stamp = self.manifest(Path(raw))
            stamp['profile_levels'] = ['s']
            stamp['profiles'] = {'s': stamp['profiles']['s']}
            validate_manifest(stamp, stamp['source'])
            stamp['profile_levels'] = ['s', '2']
            with self.assertRaises(ValueError): validate_manifest(stamp, stamp['source'])

    def test_historical_reference_requires_full_qualification_and_immutable_saved_map(self):
        import json
        import golden
        from worker_screen import REFERENCE_PREFIXES, reference_inputs, reference_files, validate_reference
        files = {prefix + ('fixture' if prefix.endswith('/') else ''): {'mode': '100644', 'sha256': 'a'*64}
                 for prefix in REFERENCE_PREFIXES}
        source = {'head': 'historical', 'tree': 'tree', 'files': files}
        reference = {'source': source, 'complete': True,
                     'golden': {'conformance': [{'target': target, 'id': case, 'status': 'passed'}
                         for target in ('native', 'workers', 'bun', 'node') for case in golden.contract_ids()]},
                     'saved': {}, 'artifacts': {}}
        reference_inputs(reference, source)
        for fault in (lambda s: s.update(complete=False),
                      lambda s: s['golden'].update(conformance=[]),
                      lambda s: s['golden']['conformance'][0].update(status='failed'),
                      lambda s: s['source'].update(tree=''),
                      lambda s: s['source']['files'].pop(next(iter(files)))):
            bad = copy.deepcopy(reference); fault(bad)
            with self.assertRaises(ValueError): reference_inputs(bad, source)
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw); manifest = root/'build.json'
            for app in ('golden', 'focused'):
                for name in ('kiosk.wasm', 'kiosk.js', 'worker.mjs', 'wrangler.json'):
                    original = str(root/app/'workers/build/target/kiosk'/name)
                    saved = root/f'{app}-{name}'; saved.write_bytes(b'original immutable bytes')
                    reference['saved'][original] = str(saved)
                    digest = hashlib.sha256(saved.read_bytes()).hexdigest()
                    reference['artifacts'].update({original: digest, str(saved): digest})
            identities = reference_files(reference, manifest)
            # None of the original/live paths exist: saved copies alone are read.
            record = {'manifest': str(manifest), 'producer': source,
                      'historical_conformance': reference['golden']['conformance'],
                      'apps': {'golden': {}, 'focused': {}}}
            for (app, name), identity in identities.items():
                copied = root/f'imported-{app}-{name}'; copied.write_bytes(Path(identity['saved']).read_bytes())
                record['apps'][app][name] = {'origin': identity, 'path': str(copied)}
            manifest.write_text(json.dumps(reference))
            record['manifest_sha256'] = hashlib.sha256(manifest.read_bytes()).hexdigest()
            validate_reference(record, source)
            bad = copy.deepcopy(reference); bad['saved'].pop(next(iter(bad['saved'])))
            with self.assertRaises(ValueError): reference_files(bad, manifest)
            copied.write_bytes(b'drift')
            with self.assertRaises(ValueError): validate_reference(record, source)
            copied.write_bytes(b'original immutable bytes')
            saved.write_bytes(b'drift')
            with self.assertRaises(ValueError): validate_reference(record, source)
            manifest.write_text('{}')
            with self.assertRaises(ValueError): validate_reference(record, source)

    def test_served_config_requires_qualified_build_and_every_saved_module_link(self):
        import json
        from worker_screen import served_config, runner_config
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw); manifest = root/'build.json'
            reference = {'source': {'head': 'producer'}, 'artifacts': {}}
            intended = str(root/'focused/workers/build/target/kiosk')
            host = {'intended': intended, 'saved': {}, 'files': {}}
            for name in ('kiosk.wasm', 'kiosk.js', 'worker.mjs', 'wrangler.json'):
                original = str(root/'live'/name); saved = root/f'saved-{name}'
                saved.write_text(json.dumps({'name': 'historical', 'main': 'worker.mjs',
                    'compatibility_date': '2026-10-01', 'rules': [], 'ratelimits': []}) if name == 'wrangler.json' else 'module')
                digest = hashlib.sha256(saved.read_bytes()).hexdigest()
                host['saved'][original] = str(saved)
                host['files'].update({original: digest, str(saved): digest})
                reference['artifacts'][str(Path(intended)/name)] = digest
            manifest.write_text(json.dumps(reference))
            row = {'target': 'workers', 'instrumentation': 'none', 'consumed_host': host,
                   'requests': 100, 'rps': 50, 'p95_us': 10, 'verified': True,
                   'errors': 0, 'unexpected': 0, 'invalid_bodies': 0, 'all_unexpected': 0,
                   'warmup_errors': 0, 'warmup_invalid_bodies': 0, 'persisted_delta': 0,
                   'load_started_unix': 100, 'load_completed_unix': 103.01,
                   'load_wall_s': 3, 'response_body_bytes': 9, 'response_body_sha256': 'a'*64,
                   'database_observation': {'enabled': False, 'server_duration_records': 0}}
            served = {'complete': True, 'source': reference['source'], 'build': str(manifest),
                      'build_sha256': hashlib.sha256(manifest.read_bytes()).hexdigest(), 'rows': [row]}
            saved, digest, _ = served_config(reference, manifest, served, 'focused')
            self.assertEqual(digest, hashlib.sha256(Path(saved).read_bytes()).hexdigest())
            candidate = root/'candidate-wrangler.json'
            config = json.loads(Path(saved).read_text()); config['name'] = 'current-folder'
            candidate.write_text(json.dumps(config))
            self.assertEqual(runner_config(candidate), runner_config(saved))
            config['ratelimits'] = [{'name': 'LIMITS', 'simple': {'limit': 1, 'period': 60}}]
            candidate.write_text(json.dumps(config))
            self.assertNotEqual(runner_config(candidate), runner_config(saved))
            faults = [lambda s: s.update(complete=False), lambda s: s.update(source={}),
                      lambda s: s.update(build_sha256='0'*64),
                      lambda s: s['rows'][0].update(verified=False),
                      lambda s: s['rows'][0].update(instrumentation='inspector'),
                      lambda s: s['rows'][0]['consumed_host'].update(intended='different-app'),
                      lambda s: s['rows'][0]['consumed_host']['saved'].pop(str(root/'live/worker.mjs')),
                      lambda s: s['rows'][0]['consumed_host']['files'].update({str(root/'live/kiosk.wasm'): '0'*64})]
            for fault in faults:
                bad = copy.deepcopy(served); fault(bad)
                with self.assertRaises(ValueError): served_config(reference, manifest, bad, 'focused')
            Path(saved).write_text('drift')
            with self.assertRaises(ValueError): served_config(reference, manifest, served, 'focused')

    def test_sleep_events_reject_in_window_sleep_and_darkwake_only(self):
        from worker_screen import sleep_events
        raw = ('1970-01-01 01:01:41 +0100 Sleep                 Entering Sleep\n'
               '1970-01-01 00:01:42 +0000 DarkWake              from Normal Sleep\n'
               '1970-01-01 00:01:43 +0000 Wake                  normal\n'
               '1970-01-01 00:01:44 +0000 Sleep                 outside window\n')
        self.assertEqual(len(sleep_events(raw, 100, 103)), 2)
        self.assertEqual(sleep_events(raw, 200, 300), [])

    def test_row_rejects_errors_state_drift_logging_and_sleep(self):
        row = {'requests': 100, 'rps': 50, 'p95_us': 10, 'verified': True,
               'errors': 0, 'unexpected': 0, 'invalid_bodies': 0, 'all_unexpected': 0,
               'warmup_errors': 0, 'warmup_invalid_bodies': 0, 'persisted_delta': 0,
               'load_started_unix': 100, 'load_completed_unix': 103.01,
               'load_wall_s': 3, 'response_body_bytes': 9,
               'response_body_sha256': 'a'*64,
               'database_observation': {'enabled': False, 'server_duration_records': 0}}
        validate_row(row)
        faults = [
            lambda s: s.update(unexpected=1),
            lambda s: s.update(warmup_invalid_bodies=1),
            lambda s: s.update(persisted_delta=1),
            lambda s: s.update(verified=False),
            lambda s: s.update(load_completed_unix=160),
            lambda s: s.update(load_completed_unix=102),
            lambda s: s['database_observation'].update(enabled=True),
            lambda s: s['database_observation'].update(server_duration_records=1),
            lambda s: s.pop('response_body_sha256'),
        ]
        for fault in faults:
            bad = copy.deepcopy(row)
            fault(bad)
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                validate_row(bad)


if __name__ == '__main__':
    unittest.main()
