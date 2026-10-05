"""Protocol counterexamples; synthetic drivers do not execute golden behaviour."""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('golden_cases', REPOSITORY / 'tools/golden_cases.py')
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class GoldenProtocolTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # Deliberately tiny, synthetic contract. These passes cannot be golden
        # application results, and nothing registers these drivers in production.
        self.case = {'id': 'SYNTHETIC-001', 'kind': 'http',
                     'fixtureVariants': ['zero', 'many'],
                     'expect': {'status': 200, 'databaseQueriesAtMost': 2, 'body': {'safe': True}},
                     'followUps': [{'expect': {'status': 403, 'writes': 0}}]}
        self.contract = {'contractVersion': 'synthetic-test', 'cases': [self.case]}
        self.write(harness.ACCEPTANCE, json.dumps(self.contract))
        self.write(harness.OBLIGATIONS, json.dumps({'acceptance_sha256': harness.digest(self.root/harness.ACCEPTANCE),
                                                   'cases': {'SYNTHETIC-001': {'status': 'not_executed'}}}))
        self.write('tools/golden_cases.py', 'synthetic-harness')
        self.write('examples/golden-todo-migration/app.jadpo', '// synthetic source')
        self.write('jadpo/crates/core/src/lib.rs', '// synthetic compiler')
        self.write('jadpo/crates/core/src/runtime/synthetic.ts', '// embedded runtime')
        self.write('jadpo/data/zones.txt', 'synthetic embedded data')
        self.write('jadpo/target/debug/jadpo', 'synthetic compiler binary')

    def write(self, path, content):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)

    def observations(self):
        return [{'variant_index': i, 'steps': [{'status': 200, 'databaseQueries': 2, 'body': {'safe': True}},
                                              {'status': 403, 'writes': 0}]}
                for i in range(2)]

    def run_synthetic(self, driver=None):
        driver = driver or (lambda case, backend: self.observations())
        return harness.run_cases(self.root, {(backend, 'http'): driver for backend in harness.BACKENDS})

    def test_no_adapters_is_not_a_pass_and_covers_both_backends(self):
        report = harness.run_cases(self.root)
        self.assertEqual(report['status'], 'not_run')
        self.assertEqual([r['backend'] for r in report['results']], ['sqlite', 'postgres'])
        self.assertTrue(all(r['reason'] and r['status'] == 'not_run' for r in report['results']))
        harness.validate_report(report, self.root)

    def test_complete_observations_are_supporting_results_never_release_claims(self):
        report = self.run_synthetic()
        self.assertEqual(report['status'], 'observations_passed')
        self.assertFalse(report['release_equivalent'])
        harness.validate_report(report, self.root)

    def test_single_bad_case_fails_even_if_other_backend_passes(self):
        def driver(case, backend):
            observed = self.observations()
            if backend == 'postgres':
                observed[0]['steps'][0]['status'] = 500
            return observed
        report = self.run_synthetic(driver)
        self.assertEqual([r['status'] for r in report['results']], ['passed', 'failed'])
        self.assertEqual(report['status'], 'failed')
        harness.validate_report(report, self.root)

    def test_query_budget_and_missing_boolean_are_not_inferred(self):
        self.assertTrue(harness.compare({'databaseQueriesAtMost': 1}, {'databaseQueries': 2}))
        self.assertTrue(harness.compare({'databaseQueriesAtMost': 1}, {'databaseQueries': True}))
        self.assertTrue(harness.compare({'databaseQueriesAtMost': 1}, {'databaseQueries': -1}))
        self.assertTrue(harness.compare({'safe': False}, {}))
        self.assertTrue(harness.compare({'body': {'safe': True}}, {'body': {'safe': 1}}))

    def test_missing_variant_follow_up_and_reordered_variant_fail(self):
        for mutation in ('variant', 'followup', 'duplicate'):
            with self.subTest(mutation=mutation):
                observations = self.observations()
                if mutation == 'variant':
                    observations.pop()
                elif mutation == 'followup':
                    observations[0]['steps'].pop()
                else:
                    observations[1]['variant_index'] = 0
                self.assertTrue(harness.assess(self.case, observations))

    def test_duplicate_missing_foreign_and_fabricated_pass_results_rejected(self):
        original = harness.run_cases(self.root)
        for mutation in ('duplicate', 'missing', 'foreign', 'pass', 'release', 'aggregate', 'schema', 'version'):
            with self.subTest(mutation=mutation):
                report = deepcopy(original)
                if mutation == 'duplicate':
                    report['results'].append(deepcopy(report['results'][0]))
                elif mutation == 'missing':
                    report['results'].pop()
                elif mutation == 'foreign':
                    report['results'][0]['id'] = 'OTHER-001'
                elif mutation == 'pass':
                    report['results'][0]['status'] = 'passed'
                elif mutation == 'release':
                    report['release_equivalent'] = True
                elif mutation == 'aggregate':
                    report['status'] = 'observations_passed'
                elif mutation == 'schema':
                    report['schema_version'] = 2
                else:
                    report['contract_version'] = 'foreign'
                with self.assertRaises(ValueError):
                    harness.validate_report(report, self.root)

    def test_source_compiler_contract_or_harness_change_invalidates_results(self):
        paths = ['examples/golden-todo-migration/app.jadpo', 'jadpo/crates/core/src/lib.rs',
                 'jadpo/target/debug/jadpo', 'jadpo/crates/core/src/runtime/synthetic.ts',
                 'jadpo/data/zones.txt', 'tools/golden_cases.py', harness.ACCEPTANCE]
        for path in paths:
            with self.subTest(path=path):
                report = self.run_synthetic()
                old = (self.root/path).read_text()
                self.write(path, old + '\n')
                with self.assertRaises(ValueError):
                    harness.validate_report(report, self.root)
                self.write(path, old)

    def test_new_and_removed_source_files_invalidate_results(self):
        report = self.run_synthetic()
        added = self.root/'examples/golden-todo-migration/new.jadpo'
        added.write_text('// new')
        with self.assertRaises(ValueError):
            harness.validate_report(report, self.root)
        added.unlink()
        (self.root/'examples/golden-todo-migration/app.jadpo').unlink()
        with self.assertRaises(ValueError):
            harness.validate_report(report, self.root)

    def test_missing_built_compiler_does_not_call_driver(self):
        (self.root/'jadpo/target/debug/jadpo').unlink()
        def unexpected_driver(case, backend):
            self.fail('Must not run an unbound adapter')
        report = self.run_synthetic(unexpected_driver)
        self.assertEqual(report['status'], 'not_run')
        harness.validate_report(report, self.root)

    def test_driver_exception_keeps_case_without_exposing_exception_secret(self):
        def broken(case, backend):
            raise RuntimeError('secret-service-key')
        report = self.run_synthetic(broken)
        self.assertEqual(len(report['results']), 2)
        self.assertEqual(report['status'], 'failed')
        self.assertNotIn('secret-service-key', json.dumps(report))
        harness.validate_report(report, self.root)

    def test_driver_cannot_mutate_the_frozen_case_or_hide_source_changes(self):
        def mutate_case(case, backend):
            case['expect']['status'] = 500
            return self.observations()
        self.assertEqual(self.run_synthetic(mutate_case)['status'], 'observations_passed')
        def mutate_source(case, backend):
            self.write('examples/golden-todo-migration/app.jadpo', '// changed during run')
            return self.observations()
        with self.assertRaisesRegex(ValueError, 'changed during'):
            self.run_synthetic(mutate_source)

    def test_tampered_observations_or_expectations_cannot_keep_pass_label(self):
        report = self.run_synthetic()
        report['results'][0]['observations'][0]['steps'][0]['status'] = 500
        with self.assertRaises(ValueError):
            harness.validate_report(report, self.root)
        report = self.run_synthetic()
        report['results'][0]['required_scenarios'][0]['expectations'][0]['status'] = 500
        with self.assertRaises(ValueError):
            harness.validate_report(report, self.root)

    def test_duplicate_acceptance_ids_rejected(self):
        self.contract['cases'].append(deepcopy(self.case))
        self.write(harness.ACCEPTANCE, json.dumps(self.contract))
        with self.assertRaisesRegex(ValueError, 'unique'):
            harness.read_contract(self.root)


class ActualGoldenInventoryTests(unittest.TestCase):
    def test_all_frozen_ids_have_two_pending_backend_entries(self):
        contract = harness.read_contract(REPOSITORY)
        report = harness.run_cases(REPOSITORY)
        self.assertEqual(len(contract['cases']), 44)
        self.assertEqual(len(report['results']), 88)
        self.assertEqual(report['status'], 'not_run')
        self.assertTrue(all(r['status'] == 'not_run' for r in report['results']))
        harness.validate_report(report, REPOSITORY)
        user = next(r for r in report['results'] if r['id'] == 'USER-001')
        self.assertEqual(len(user['required_scenarios'][0]['expectations']), 4)
        relationship = next(r for r in report['results'] if r['id'] == 'REL-001')
        self.assertEqual(len(relationship['required_scenarios']), 3)

    def test_command_is_nonzero_for_pending_adapters_and_never_reuses_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)/'cases.json'
            command = [sys.executable, str(REPOSITORY/'tools/golden_cases.py'), '--output', str(output)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            original = output.read_bytes()
            self.assertEqual(json.loads(original)['status'], 'not_run')
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
