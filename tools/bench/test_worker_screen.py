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
