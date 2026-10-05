"""Checks that the validation gate cannot silently lose evidence."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

REPOSITORY = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('jadpo_verify', REPOSITORY / 'tools/verify.py')
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


class EvidenceInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)
        self.addCleanup(patch.stopall)
        patch.object(verify, 'ROOT', self.root).start()
        self.write('tests/runtime/example.test.ts', '// test source')
        self.write('examples/sample/app.jadpo', 'type Name = Text {}')
        self.write('examples/golden-todo/app.jadpo', '// design contract')
        acceptance = json.dumps({'cases': [{'id': 'AUTH-001'}]})
        self.write('examples/golden-todo/acceptance.json', acceptance)
        self.obligations = {'acceptance_sha256': hashlib.sha256(acceptance.encode()).hexdigest(),
                            'cases': {'AUTH-001': {'status': 'not_executed', 'gap': 'No integrated runtime evidence'}}}
        self.write_obligations()
        self.manifest = {'runtime_suites': ['tests/runtime/example.test.ts'], 'postgres_suites': {},
                         'examples': {'examples/sample': {'status': 'runnable', 'reason': 'Example'},
                                      'examples/golden-todo': {'status': 'design_contract', 'reason': 'Not executable'}}}

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)

    def write_obligations(self):
        self.write('tests/validation/golden-obligations.json', json.dumps(self.obligations))

    def test_registered_evidence_is_accepted_without_claiming_golden_execution(self):
        result = verify.inventory(self.manifest)
        self.assertEqual(result['cases']['AUTH-001']['status'], 'not_executed')

    def test_new_nested_runtime_suite_cannot_be_silently_omitted(self):
        self.write('tests/runtime/auth/revocation.test.ts', '// newly authored suite')
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            verify.inventory(self.manifest)

    def test_duplicate_suite_does_not_count_as_extra_evidence(self):
        self.manifest['runtime_suites'] *= 2
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            verify.inventory(self.manifest)

    def test_deleted_suite_is_not_a_pass(self):
        (self.root/'tests/runtime/example.test.ts').unlink()
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            verify.inventory(self.manifest)

    def test_example_with_only_entity_dossiers_still_requires_classification(self):
        self.write('examples/new-app/entities/customer.jadpo', 'entity Customer {}')
        with self.assertRaisesRegex(ValueError, 'explicitly classified'):
            verify.inventory(self.manifest)

    def test_removing_a_golden_obligation_is_rejected(self):
        self.obligations['cases'] = {}
        self.write_obligations()
        with self.assertRaisesRegex(ValueError, 'Every golden acceptance'):
            verify.inventory(self.manifest)

    def test_changing_acceptance_requires_revisiting_the_evidence_map(self):
        self.write('examples/golden-todo/acceptance.json', json.dumps({'cases': [{'id': 'AUTH-001', 'expect': {'status': 200}}]}))
        with self.assertRaisesRegex(ValueError, 'Golden acceptance changed'):
            verify.inventory(self.manifest)

    def test_a_claimed_pass_without_an_executable_case_is_rejected(self):
        self.obligations['cases']['AUTH-001']['status'] = 'passed'
        self.write_obligations()
        with self.assertRaisesRegex(ValueError, 'unexecuted obligation'):
            verify.inventory(self.manifest)

    def test_quarantine_requires_an_explanation(self):
        self.manifest['examples']['examples/sample'] = {'status': 'legacy', 'reason': ''}
        with self.assertRaisesRegex(ValueError, 'Invalid example classification'):
            verify.inventory(self.manifest)


class VerificationEnvironmentTests(unittest.TestCase):
    def test_caller_database_and_component_output_canaries_never_reach_subprocesses(self):
        # Exercise the real function used before every subprocess, without any
        # connection, component output write or broad verifier invocation.
        unsafe = ['DATABASE_URL', 'SQLITE_PATH', 'JADPO_DELIVERY_HOOK_DATABASE_URL',
                  'JADPO_DELIVERY_HOOK_COMPONENT_OUTPUT', 'JADPO_DELIVERY_HOOK_COMPONENT_VARIANT', 'JADPO_BIN']
        canaries = {name: 'canary-no-connection-or-write' for name in unsafe}
        canaries['RETAINED_VERIFICATION_CONTROL'] = 'retained'
        with patch.dict(verify.os.environ, canaries, clear=True):
            environment = verify.verification_environment()
            for name in unsafe[:-1]:
                self.assertNotIn(name, environment)
            self.assertEqual(environment['JADPO_BIN'], str(verify.COMPILER))
            self.assertEqual(environment['RETAINED_VERIFICATION_CONTROL'], 'retained')
            self.assertEqual(verify.os.environ['JADPO_DELIVERY_HOOK_DATABASE_URL'], canaries['JADPO_DELIVERY_HOOK_DATABASE_URL'])


class GoldenCaseIntegrationTests(unittest.TestCase):
    def test_pending_real_protocol_is_retained_separately_from_supported_success(self):
        with tempfile.TemporaryDirectory() as directory:
            result = verify.collect_golden_case_report(Path(directory), verify.verification_environment())
            self.assertEqual(result['status'], 'not_run')
            self.assertEqual(result['case_backend_entries'], 88)
            self.assertEqual(result['counts'], {'passed': 0, 'failed': 0, 'not_run': 88})
            self.assertFalse(json.loads((Path(directory)/result['report']).read_text())['release_equivalent'])

    def test_crashed_or_contradictory_command_is_not_pending_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with patch.object(verify, 'execute', return_value=1):
                with self.assertRaises(FileNotFoundError):
                    verify.collect_golden_case_report(output, {})
            def contradictory(*args):
                (output/'golden-cases.json').write_text(json.dumps({'status': 'not_run', 'release_equivalent': False}))
                return 0
            with patch.object(verify, 'execute', side_effect=contradictory):
                with self.assertRaisesRegex(ValueError, 'contradicted'):
                    verify.collect_golden_case_report(output, {})

    def test_existing_report_cannot_turn_cli_crash_into_pending_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            path = output/'golden-cases.json'
            original = json.dumps({'status': 'not_run', 'release_equivalent': False, 'results': []})
            path.write_text(original)
            with self.assertRaisesRegex(ValueError, 'already exists'):
                verify.collect_golden_case_report(output, verify.verification_environment())
            self.assertEqual(path.read_text(), original)


if __name__ == '__main__':
    unittest.main()
