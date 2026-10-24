#!/usr/bin/env python3
"""Private gate orchestration regressions; no live provider transport."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import run as harness
import verify as gate

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent
ROOT = PROJECT.parents[2]


class ContractReporting(unittest.TestCase):
    def test_missing_own_contract_lists_every_unavailable_obligation(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = harness.verify_contract(Path(temporary), ['node', 'bun'])
        self.assertFalse(result['completeContract'])
        self.assertEqual(result['counts'], {'ran': 88, 'passed': 0, 'failed': 88})
        self.assertEqual({item['id'] for item in result['cases']}, set(harness.CASE_IDS))

    def test_preview_filtered_or_missing_runtime_cannot_claim_complete(self):
        reports = [{'runtime': runtime, 'id': identifier, 'status': 'passed'}
                   for runtime in ['node', 'bun'] for identifier in harness.CASE_IDS]
        self.assertTrue(harness.complete_contract(ROOT, ['node', 'bun'], list(harness.CASE_IDS), reports))
        self.assertFalse(harness.complete_contract(ROOT.parent, ['node', 'bun'], list(harness.CASE_IDS), reports))
        self.assertFalse(harness.complete_contract(ROOT, ['node'], list(harness.CASE_IDS), reports))
        self.assertFalse(harness.complete_contract(ROOT, ['node', 'bun'], ['AUTH-001'], reports))
        self.assertFalse(harness.complete_contract(ROOT, ['node', 'bun'], list(harness.CASE_IDS), reports[:-1]))

    def test_runtime_selection_rejects_empty_duplicate_and_unknown(self):
        for value in ['', 'node,node', 'node,', 'deno']:
            with self.assertRaises(ValueError):
                harness.read_runtimes(value)


class Dependencies(unittest.TestCase):
    def test_install_is_locked_and_scripts_disabled(self):
        calls = []
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            with patch.object(gate, 'run', side_effect=lambda args, **kwargs: calls.append((args, kwargs))):
                gate.install_dependencies(project)
        self.assertEqual(calls[0][0], ['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'])
        self.assertEqual(calls[0][1]['cwd'], project)

    def test_concurrent_install_or_verification_waits_for_lock_and_failure_releases(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            code = '''import sys
from pathlib import Path
from verify import dependency_lock
print('waiting', flush=True)
with dependency_lock(Path(sys.argv[1])):
    print('entered', flush=True)
    raise SystemExit(7)
'''
            child = None
            try:
                with gate.dependency_lock(project):
                    child = subprocess.Popen([sys.executable, '-c', code, str(project)], cwd=HERE,
                                             stdout=subprocess.PIPE, text=True)
                    self.assertEqual(child.stdout.readline().strip(), 'waiting')
                    self.assertFalse(select.select([child.stdout], [], [], 0.1)[0], 'another installer entered the active verification')
                self.assertEqual(child.stdout.readline().strip(), 'entered')
                self.assertEqual(child.wait(timeout=5), 7)
                with gate.dependency_lock(project):
                    pass  # The failed child no longer holds the OS lock.
            finally:
                if child is not None:
                    if child.poll() is None:
                        child.kill()
                        child.wait()
                    child.stdout.close()


class TestDiscovery(unittest.TestCase):
    def test_production_build_artifacts_do_not_duplicate_authored_tests(self):
        build = PROJECT / 'build'
        build.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='discovery-', dir=build) as temporary:
            emitted = Path(temporary) / 'emitted.test.js'
            emitted.write_text('import { it } from "vitest"; it("emitted build sentinel", () => {});\n')
            # Listing collects identities without executing hooks or tests. The
            # deliberately unreachable URL ensures no live database is needed.
            answer = subprocess.run([str(PROJECT / 'node_modules/.bin/vitest'), 'list',
                                     '--json', '--maxWorkers=2'], cwd=PROJECT, check=True,
                                    capture_output=True, text=True,
                                    env={**os.environ, 'TEST_DATABASE_URL':
                                         'postgresql://127.0.0.1:1/discovery_only'})
            tests = json.loads(answer.stdout)
        expected_files = {str(path) for path in (PROJECT / 'test').glob('*.test.ts')}
        self.assertEqual({test['file'] for test in tests}, expected_files)
        self.assertEqual(len(tests), 42)
        self.assertEqual(len({(test['file'], test['name']) for test in tests}), 42)


class SharedPostgres(unittest.TestCase):
    def test_keyword_socket_runs_real_node_bun_requests_and_preserves_parameters(self):
        # Keep the socket path short on both macOS and Linux, including quoting.
        with tempfile.TemporaryDirectory(prefix="d3 sock ' ", dir='/tmp') as socket_path, \
                tempfile.TemporaryDirectory(prefix='socket-control-', dir=PROJECT / 'build') as temporary:
            server = harness.Postgres(Path(temporary), socket_directory=Path(socket_path))
            shared = None
            try:
                escaped = socket_path.replace('\\', '\\\\').replace("'", "\\'")
                dsn = f"host='{escaped}' port={server.port} user=golden dbname=postgres application_name='gate socket + marker' connect_timeout=2"
                shared = harness.SharedPostgres(dsn)
                database = 'golden_socket_' + os.urandom(5).hex()
                shared.prepare(database)
                code = """import { Store } from './src/db/store.ts';
const store = new Store(process.env.DATABASE_URL);
try {
  const [row] = await store.client.unsafe("SELECT current_setting('application_name') AS name");
  process.stdout.write(JSON.stringify(row));
} finally { await store.close(); }
"""
                result = subprocess.check_output([harness.tool('node'), '--input-type=module', '-e', code], cwd=PROJECT,
                                                 env={**os.environ, 'DATABASE_URL': shared.url(database)}, text=True)
                self.assertEqual(json.loads(result)['name'], 'gate socket + marker')
                before = subprocess.check_output([harness.tool('psql'), '-At', '-d', dsn,
                                                  '-c', 'SELECT datname FROM pg_database ORDER BY datname'], text=True)
                with patch.dict(os.environ, {'JADPO_TEST_DATABASE_URL': dsn}):
                    # Call the runner's production execution function; the outer
                    # wrapper already owns the dependency lock in gate runs.
                    result = harness.verify_contract(ROOT, ['node', 'bun'], ['CREATE-001'])
                self.assertEqual(result['counts'], {'ran': 2, 'passed': 2, 'failed': 0})
                self.assertFalse(result['completeContract'])
                after = subprocess.check_output([harness.tool('psql'), '-At', '-d', dsn,
                                                 '-c', 'SELECT datname FROM pg_database ORDER BY datname'], text=True)
                self.assertEqual(before, after)
            finally:
                if shared is not None:
                    shared.stop()
                server.stop()

    def test_failure_drops_only_owned_databases_and_keeps_server_alive(self):
        with tempfile.TemporaryDirectory(prefix='gate-test-', dir=PROJECT / 'build') as temporary:
            owner = None
            shared = None
            sentinel = 'golden_sentinel_' + os.urandom(5).hex()
            try:
                base = os.environ.get('JADPO_TEST_DATABASE_URL', '').strip()
                if not base:
                    owner = harness.Postgres(Path(temporary))
                    base = owner.url('postgres')
                shared = harness.SharedPostgres(base)
                shared.sql('postgres', f'CREATE DATABASE {sentinel}')
                database = 'golden_cleanup_' + os.urandom(5).hex()
                bad = Path(temporary) / 'application'
                (bad / 'drizzle').mkdir(parents=True)
                (bad / 'drizzle' / '0000_bad.sql').write_text('THIS IS INVALID SQL;')
                with patch.object(harness, 'PROJECT', bad), self.assertRaises(subprocess.CalledProcessError):
                    shared.prepare(database)
                shared.stop()
                rows = subprocess.check_output([harness.tool('psql'), '-At', '-d', base,
                                                '-c', f"SELECT datname FROM pg_database WHERE datname IN ('{database}','{sentinel}')"], text=True)
                self.assertEqual(rows.strip(), sentinel)
                shared.sql('postgres', f'DROP DATABASE {sentinel}')
            finally:
                if shared is not None:
                    shared.stop()
                if owner is not None:
                    owner.stop()


class Registration(unittest.TestCase):
    def test_tests_group_owns_one_full_ts_pipeline_and_quick_does_not(self):
        specification = importlib.util.spec_from_file_location('jadpo_gate_for_test', ROOT / 'tools/verify.py')
        module = importlib.util.module_from_spec(specification)
        sys.modules[specification.name] = module
        specification.loader.exec_module(module)
        full = [step for step in module.steps('full') if 'TypeScript golden' in step.name]
        self.assertEqual(len(full), 1)
        self.assertEqual(full[0].group, 'tests')
        self.assertTrue(full[0].postgres)
        self.assertNotIn('--contract-source', full[0].command)
        self.assertFalse([step for step in module.steps('quick') if 'TypeScript golden' in step.name])

    def test_ci_declares_same_exact_runtime_versions(self):
        text = (ROOT / '.github/workflows/validation.yml').read_text()
        tests = text.split('\n  tests:\n', 1)[1].split('\n  compiled:', 1)[0]
        self.assertIn('node-version-file: examples/golden-todo/typescript/.node-version', tests)
        self.assertIn('bun-version-file: examples/golden-todo/typescript/.bun-version', tests)
        self.assertEqual((PROJECT / '.node-version').read_text().strip(), '24.18.1')
        self.assertEqual((PROJECT / '.bun-version').read_text().strip(), '1.2.20')


if __name__ == '__main__':
    (PROJECT / 'build').mkdir(exist_ok=True)
    unittest.main()
