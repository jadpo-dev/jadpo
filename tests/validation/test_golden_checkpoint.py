"""Synthetic counterexamples for checkpoint integrity, not application passes."""
from copy import deepcopy
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools'))
import golden_cases as harness
import golden_checkpoint as checkpoint


class CheckpointIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.cases = {case['id']: case for case in harness.read_contract(ROOT)['cases']}

    def raw(self, case_id):
        case = self.cases[case_id]
        replies = []
        for request in case.get('requests', [case.get('request')]):
            replies.append({'method': request['method'], 'path': request['path'].replace('/todos/t2', '/todos/00000000-0000-4000-8000-000000000022'),
                            'auth': request.get('auth'), 'requestJson': request.get('json'),
                            'status': 404 if case_id == 'READ-002' else 401,
                            'body': {'error': {'code': 'todo_not_found' if case_id == 'READ-002' else 'authentication_required', 'request_id': f'req_{len(replies)}'}},
                            'statements': [], 'authenticationAttempts': 1})
        if case_id == 'CREATE-004':
            replies[0]['body'] = {'id': 'synthetic-id', 'due_at': None}
            replies.append({'persistence': {'id': 'synthetic-id', 'storedDueAt': None}})
        return replies

    def outputs(self):
        return [{'backend': backend, 'results': [{'id': case_id, 'raw': self.raw(case_id)} for case_id in checkpoint.SUPPORTED]} for backend in harness.BACKENDS]

    def test_all_inventory_entries_retained_and_selected_observations_do_not_infer_pass(self):
        report = harness.run_cases(ROOT)
        checkpoint.apply_checkpoint(report, self.outputs(), ROOT)
        self.assertEqual(len(report['results']), 88)
        self.assertEqual(sum(entry['status'] == 'not_run' for entry in report['results']), 72)
        self.assertEqual(report['status'], 'failed')
        self.assertFalse(report['release_equivalent'])

    def test_missing_duplicate_foreign_result_and_backend_are_rejected(self):
        for mutation in ('missing', 'duplicate', 'foreign', 'backend'):
            outputs = self.outputs()
            if mutation == 'missing': outputs[0]['results'].pop()
            elif mutation == 'duplicate': outputs[0]['results'].append(deepcopy(outputs[0]['results'][0]))
            elif mutation == 'foreign': outputs[0]['results'][0]['id'] = 'AUTH-002'
            else: outputs[1]['backend'] = 'sqlite'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                checkpoint.apply_checkpoint(harness.run_cases(ROOT), outputs, ROOT)

    def test_statement_and_auth_counts_are_derived_from_raw_instrumentation(self):
        raw = self.raw('AUTH-001')
        raw[0]['statements'] = [{'sql': 'SELECT 1', 'operation': 'SELECT'}, {'sql': 'INSERT INTO todo DEFAULT VALUES', 'operation': 'INSERT'}]
        raw[0]['authenticationAttempts'] = 7
        observed = checkpoint.observations(self.cases['AUTH-001'], raw)[0]['steps'][0]
        self.assertEqual(observed['databaseQueries'], 2)
        self.assertEqual(observed['writes'], 1)
        self.assertEqual(observed['authenticationAttempts'], 7)
        self.assertTrue(harness.assess(self.cases['AUTH-001'], [{'variant_index': 0, 'steps': [observed]}]))

    def test_changed_request_auth_body_and_missing_instrumentation_fail(self):
        for mutation in ('path', 'auth', 'requestJson', 'statements', 'authenticationAttempts'):
            raw = self.raw('AUTH-005')
            raw[0][mutation] = {'foreign': True} if mutation == 'requestJson' else None
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, AttributeError)):
                checkpoint.observations(self.cases['AUTH-005'], raw)

    def test_concealment_pair_demands_both_requests_and_compares_complete_public_body(self):
        raw = self.raw('READ-002')
        self.assertTrue(checkpoint.observations(self.cases['READ-002'], raw)[0]['steps'][0]['samePublicShape'])
        raw[1]['body']['private_owner'] = 'bob'
        self.assertFalse(checkpoint.observations(self.cases['READ-002'], raw)[0]['steps'][0]['samePublicShape'])
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['READ-002'], raw[:1])

    def test_stored_and_output_values_are_separate_and_require_matching_identity(self):
        raw = self.raw('CREATE-004')
        raw[1]['persistence']['storedDueAt'] = 'unexpected stored date'
        step = checkpoint.observations(self.cases['CREATE-004'], raw)[0]['steps'][0]
        self.assertEqual(step['storedDueAt'], 'unexpected stored date')
        self.assertIsNone(step['outputDueAt'])
        raw[1]['persistence']['id'] = 'foreign'
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['CREATE-004'], raw)

    def test_scenario_expansion_is_not_silently_partial(self):
        for field, value in (('fixtureVariants', ['first', 'second']), ('followUps', [{'expect': {'status': 200}}])):
            case = deepcopy(self.cases['AUTH-001']); case[field] = value
            with self.assertRaises(ValueError): checkpoint.observations(case, self.raw('AUTH-001'))
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['USER-001'], [])

    def test_actual_adapter_and_instrumentation_are_bound_source_inputs(self):
        paths = harness.provenance(ROOT)['files']
        for name in ('golden_checkpoint.py', 'golden_http_checkpoint.ts', 'golden_sql_observer.ts'):
            self.assertIn('tools/'+name, paths)

    def test_adapter_failure_keeps_case_failed_and_later_cases_present(self):
        outputs = self.outputs()
        outputs[0]['results'][0] = {'id': 'AUTH-001', 'adapterFailure': True, 'raw': []}
        report = harness.run_cases(ROOT)
        checkpoint.apply_checkpoint(report, outputs, ROOT)
        entry = next(entry for entry in report['results'] if entry['id'] == 'AUTH-001' and entry['backend'] == 'sqlite')
        self.assertEqual(entry['status'], 'failed')
        self.assertNotIn('observations', entry)
        self.assertEqual(len(report['results']), 88)

    def test_artifact_raw_dependency_and_rederived_observation_integrity(self):
        with tempfile.TemporaryDirectory() as temporary:
            evidence = Path(temporary)
            target = evidence/'application/build/target'; target.mkdir(parents=True)
            dependencies = evidence/'dependencies'; jose = dependencies/'node_modules/jose'; jose.mkdir(parents=True)
            (jose/'package.json').write_text('synthetic dependency')
            for generated, retained in (('persistence.ts', 'original-persistence.ts'), ('first-party-authentication.ts', 'original-authentication.ts')):
                (target/generated).write_text('synthetic instrumented')
                (evidence/retained).write_text('synthetic original')
            outputs = self.outputs()
            for output in outputs: (evidence/f'{output["backend"]}-raw.json').write_text(json.dumps(output))
            report = harness.run_cases(ROOT); checkpoint.apply_checkpoint(report, outputs, ROOT)
            original = checkpoint.hashes(evidence/'application/build')
            original['target/persistence.ts'] = harness.digest(evidence/'original-persistence.ts')
            original['target/first-party-authentication.ts'] = harness.digest(evidence/'original-authentication.ts')
            report['checkpoint'] = {'supported_ids': list(checkpoint.SUPPORTED), 'generated_original': original,
                                    'generated_instrumented': checkpoint.hashes(evidence/'application/build'),
                                    'raw_files': {f'{backend}-raw.json': harness.digest(evidence/f'{backend}-raw.json') for backend in harness.BACKENDS},
                                    'dependency_files': checkpoint.hashes(jose)}
            checkpoint.validate_checkpoint(report, evidence, dependencies)
            for path in (target/'persistence.ts', evidence/'original-authentication.ts', evidence/'sqlite-raw.json', jose/'package.json'):
                previous = path.read_bytes(); path.write_bytes(previous+b'\n')
                with self.subTest(path=path.name), self.assertRaises(ValueError): checkpoint.validate_checkpoint(report, evidence, dependencies)
                path.write_bytes(previous)
            changed = deepcopy(report)
            entry = next(entry for entry in changed['results'] if entry['id'] == 'AUTH-001' and entry['backend'] == 'sqlite')
            entry['observations'][0]['steps'][0]['writes'] = 99
            with self.assertRaisesRegex(ValueError, 'differ from raw'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies)


if __name__ == '__main__': unittest.main()
