"""Synthetic counterexamples for checkpoint integrity, not application passes."""
from copy import deepcopy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools'))
import golden_cases as harness
import golden_checkpoint as checkpoint


class CheckpointIntegrityTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='jadpo-synthetic-checkpoint-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # Protocol tests run before compiler-build in the supported verifier.
        # This fake binary is only a digest fixture; it is never executed and
        # cannot establish real application behaviour. Production validation
        # still rejects passing observations when a compiler binding is absent.
        for name in (harness.ACCEPTANCE, harness.OBLIGATIONS, 'tools/golden_cases.py',
                     'tools/golden_checkpoint.py', 'tools/golden_http_checkpoint.ts',
                     'tools/golden_sql_observer.ts'):
            destination = self.root/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT/name, destination)
        self.compiler = self.root/'jadpo/target/debug/jadpo'
        self.compiler.parent.mkdir(parents=True)
        self.compiler.write_text('synthetic compiler digest fixture; never executable')
        contract = harness.read_contract(self.root)
        self.clock = contract['clock']
        self.cases = {case['id']: case for case in contract['cases']}
        self.query_plans = [{'entity': 'Todo', 'strategy': 'indexed_keyset_page', 'fetch_limit': 'page_size_plus_one'}]

    def raw(self, case_id):
        case = self.cases[case_id]
        if case_id == 'REL-001':
            return [{'variant_index': index, 'fixture_variant': variant,
                     'raw': self.collection_raw(case_id, count)} for index, (variant, count) in enumerate(zip(case['fixtureVariants'], (0, 1, 100)))]
        if case_id in ('LIST-001', 'LIST-002', 'LIST-003'):
            return self.collection_raw(case_id)
        replies = []
        requests = case.get('requests', [case.get('request')])
        if case_id == 'DELETE-001': requests = [case['request'], {**case['request'], 'method': 'GET'}]
        for request in requests:
            replies.append({'method': request['method'], 'path': checkpoint.request_path(request['path']),
                            'auth': request.get('auth'), 'requestJson': request.get('json'),
                            'requestedPath': request['path'],
                            'at': self.clock,
                            'status': 404 if case_id == 'READ-002' else 401,
                            'body': {'error': {'code': 'todo_not_found' if case_id == 'READ-002' else 'authentication_required', 'request_id': f'req_{len(replies)}'}},
                            'statements': [], 'authenticationAttempts': 1})
        if case_id == 'CREATE-004':
            replies[0]['body'] = {'id': 'synthetic-id', 'due_at': None}
            replies.append({'persistence': {'id': 'synthetic-id', 'storedDueAt': None}})
        if case_id in ('CREATE-001', 'READ-001'):
            replies[0]['body'] = {'id': checkpoint.ALIASES['t1'], 'title': 'synthetic view', 'due_at': None}
            replies.append({'outputValidation': {'route': 'POST /todos' if case_id == 'CREATE-001' else 'GET /todos/{todo_id}', 'schema': 'TodoView', 'validator': 'validate_TodoView', 'accepted': True, 'validatedBody': deepcopy(replies[0]['body'])}})
            if case_id == 'CREATE-001': replies.append({'persistence': {'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice']}})
            else: replies.insert(0, {'fixture': {'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice'], 'deleted_at': None}})
        if case_id.startswith('PATCH-') or case_id in ('DELETE-001', 'DELETE-002'):
            fixture = {'id': checkpoint.ALIASES['t2' if case_id == 'PATCH-004' else 't1'],
                       'owner_id': checkpoint.ALIASES['bob' if case_id == 'PATCH-004' else 'alice'],
                       'title': 'original fixture title', 'due_at': '2026-02-01T00:00:00.000Z' if case_id in ('PATCH-001', 'PATCH-002', 'PATCH-005') else None,
                       'reminder_sent_at': self.clock if case_id == 'PATCH-005' else None, 'deleted_at': self.clock if case_id == 'DELETE-002' else None}
            after = deepcopy(fixture)
            if case_id == 'PATCH-002': after['due_at'] = None
            if case_id == 'PATCH-005': after.update(due_at='2026-02-15T09:00:00Z', reminder_sent_at=None)
            if case_id == 'DELETE-001': after['deleted_at'] = self.clock
            replies.insert(0, {'fixture': fixture})
            if case_id == 'DELETE-001': replies.insert(2, {'persistence': after})
            else: replies.append({'persistence': after})
        return replies

    def collection_raw(self, case_id, count=5):
        case = self.cases[case_id]
        rows = [{'id': f'00000000-0000-4000-8000-{100 + index:012}', 'owner_id': checkpoint.ALIASES['alice'],
                 'title': 'synthetic fixture', 'status': 'open', 'due_at': '2026-01-30T00:00:00Z',
                 'created_at': '2026-01-15T11:00:00Z', 'deleted_at': None} for index in range(count)]
        visible = list(rows)
        if case_id != 'LIST-002':
            rows += [{'id': checkpoint.ALIASES['t2'], 'owner_id': checkpoint.ALIASES['bob'], 'created_at': self.clock, 'deleted_at': None},
                     {'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice'], 'created_at': self.clock, 'deleted_at': self.clock}]
        raw = [{'fixtureRows': rows, 'queryMetadata': deepcopy(self.query_plans)}]
        if case_id == 'REL-001':
            body = {'user_id': checkpoint.ALIASES['alice'], 'todos': [{'id': row['id']} for row in visible]}
            raw.append({'method': 'GET', 'path': checkpoint.request_path(case['request']['path']), 'requestedPath': case['request']['path'], 'auth': case['request']['auth'], 'requestJson': None, 'at': self.clock, 'status': 200, 'body': body,
                        'statements': [{'sql': 'SELECT id FROM "user" WHERE id = $1', 'operation': 'SELECT', 'numericParameters': {}},
                                       {'sql': 'SELECT id FROM "todo" ORDER BY "created_at" ASC, "id" ASC LIMIT $4', 'operation': 'SELECT', 'numericParameters': {'4': 101}}], 'authenticationAttempts': 1})
            raw.append({'outputValidation': {'route': 'GET /users/{user_id}/todos', 'schema': 'UserWithTodos', 'validator': 'validate_UserWithTodos', 'accepted': True, 'validatedBody': deepcopy(body)}})
            return raw
        descending = sorted(visible, key=lambda row: (row['created_at'], row['id']), reverse=True)
        requests = [*case['requests'], case['requests'][0]] if case_id == 'LIST-002' else [case['request']]
        for index, request in enumerate(requests):
            page = descending[(index if index < 3 else 0)*2:((index if index < 3 else 0)+1)*2] if case_id in ('LIST-001', 'LIST-002') else descending
            next_ = {'created_at': page[-1]['created_at'], 'id': page[-1]['id']} if page and index != 2 else None
            body = {'items': [{'id': row['id'], 'status': row['status'], 'due_at': row['due_at']} for row in page], 'next': next_}
            path = request['path']
            for token, previous in (('{first.next}', 1), ('{second.next}', 2)):
                if token in path: path = path.replace(token, quote(json.dumps(raw[previous]['body']['next'], separators=(',', ':')), safe="~()*!.'-"))
            raw.append({'method': request['method'], 'path': path, 'requestedPath': request['path'], 'auth': request.get('auth'), 'requestJson': request.get('json'), 'at': self.clock, 'status': 200, 'body': body,
                        'statements': [{'sql': 'SELECT id FROM "todo" ORDER BY "created_at" DESC, "id" DESC LIMIT $6', 'operation': 'SELECT', 'numericParameters': {'6': 3}}], 'authenticationAttempts': 1})
        return raw

    def outputs(self):
        return [{'backend': backend, 'results': [{'id': case_id, 'scenarios' if case_id == 'REL-001' else 'raw': self.raw(case_id)} for case_id in checkpoint.SUPPORTED]} for backend in harness.BACKENDS]

    def test_all_inventory_entries_retained_and_selected_observations_do_not_infer_pass(self):
        report = harness.run_cases(self.root)
        checkpoint.apply_checkpoint(report, self.outputs(), self.root)
        self.assertEqual(len(report['results']), 88)
        self.assertEqual(sum(entry['status'] == 'not_run' for entry in report['results']), 88 - len(checkpoint.SUPPORTED)*2)
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
                checkpoint.apply_checkpoint(harness.run_cases(self.root), outputs, self.root)

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
        report = harness.run_cases(self.root)
        checkpoint.apply_checkpoint(report, outputs, self.root)
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
            for generated, retained in (('persistence.ts', 'original-persistence.ts'), ('first-party-authentication.ts', 'original-authentication.ts'), ('app.ts', 'original-app.ts')):
                (target/generated).write_text('synthetic instrumented')
                (evidence/retained).write_text('synthetic original')
            plan_path = evidence/'application/build/persistence/entities.json'; plan_path.parent.mkdir()
            plan_path.write_text(json.dumps({'query_plans': self.query_plans}))
            outputs = self.outputs()
            for output in outputs: (evidence/f'{output["backend"]}-raw.json').write_text(json.dumps(output))
            report = harness.run_cases(self.root); checkpoint.apply_checkpoint(report, outputs, self.root)
            original = checkpoint.hashes(evidence/'application/build')
            original['target/persistence.ts'] = harness.digest(evidence/'original-persistence.ts')
            original['target/first-party-authentication.ts'] = harness.digest(evidence/'original-authentication.ts')
            original['target/app.ts'] = harness.digest(evidence/'original-app.ts')
            report['checkpoint'] = {'supported_ids': list(checkpoint.SUPPORTED), 'generated_original': original,
                                    'generated_instrumented': checkpoint.hashes(evidence/'application/build'),
                                    'raw_files': {f'{backend}-raw.json': harness.digest(evidence/f'{backend}-raw.json') for backend in harness.BACKENDS},
                                    'dependency_files': checkpoint.hashes(jose)}
            checkpoint.validate_checkpoint(report, evidence, dependencies, self.root)
            for path in (target/'persistence.ts', evidence/'original-authentication.ts', evidence/'sqlite-raw.json', jose/'package.json'):
                previous = path.read_bytes(); path.write_bytes(previous+b'\n')
                with self.subTest(path=path.name), self.assertRaises(ValueError): checkpoint.validate_checkpoint(report, evidence, dependencies, self.root)
                path.write_bytes(previous)
            changed = deepcopy(report)
            entry = next(entry for entry in changed['results'] if entry['id'] == 'AUTH-001' and entry['backend'] == 'sqlite')
            entry['observations'][0]['steps'][0]['writes'] = 99
            with self.assertRaisesRegex(ValueError, 'differ from raw'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies, self.root)
            changed = deepcopy(report)
            raw_path = evidence/'sqlite-raw.json'
            raw_output = json.loads(raw_path.read_text())
            next(item for item in raw_output['results'] if item['id'] == 'LIST-001')['raw'][0]['queryMetadata'][0]['foreign'] = True
            raw_path.write_text(json.dumps(raw_output))
            changed['checkpoint']['raw_files']['sqlite-raw.json'] = harness.digest(raw_path)
            with self.assertRaisesRegex(ValueError, 'query metadata differs'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies, self.root)

    def test_synthetic_fixture_still_rejects_passes_without_compiler_binding(self):
        self.compiler.unlink()
        report = harness.run_cases(self.root)
        self.assertIsNone(report['provenance']['compiler_sha256'])
        self.assertTrue(all(entry['status'] == 'not_run' for entry in report['results']))
        with self.assertRaisesRegex(ValueError, 'Passing label lacks complete matching observations'):
            checkpoint.apply_checkpoint(report, self.outputs(), self.root)

    def test_patch_due_and_title_derive_from_persisted_values(self):
        raw = self.raw('PATCH-001')
        raw[-1]['persistence']['title'] = 'observed actual title'
        step = checkpoint.observations(self.cases['PATCH-001'], raw)[0]['steps'][0]
        self.assertEqual(step['storedTitle'], 'observed actual title')
        self.assertEqual(step['storedDueAt'], '2026-02-01T00:00:00Z')
        raw[-1]['persistence']['due_at'] = '2030-01-01T00:00:00Z'
        self.assertEqual(checkpoint.observations(self.cases['PATCH-001'], raw)[0]['steps'][0]['storedDueAt'], '2030-01-01T00:00:00Z')
        cleared = checkpoint.observations(self.cases['PATCH-002'], self.raw('PATCH-002'))[0]['steps'][0]
        self.assertIsNone(cleared['storedDueAt'])

    def test_patch_fixtures_owner_identity_due_date_and_order_are_required(self):
        for mutation in ('owner', 'id', 'due', 'missing', 'order'):
            raw = self.raw('PATCH-001')
            if mutation == 'owner': raw[0]['fixture']['owner_id'] = checkpoint.ALIASES['bob']
            elif mutation == 'id': raw[-1]['persistence']['id'] = 'foreign'
            elif mutation == 'due': raw[0]['fixture']['due_at'] = None
            elif mutation == 'missing': raw.pop()
            else: raw[0], raw[1] = raw[1], raw[0]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                checkpoint.observations(self.cases['PATCH-001'], raw)

    def test_rejected_patch_cannot_claim_zero_writes_when_storage_changed(self):
        for case_id in ('PATCH-003', 'PATCH-004'):
            raw = self.raw(case_id)
            raw[-1]['persistence']['title'] = 'changed behind instrumentation'
            with self.subTest(case_id=case_id), self.assertRaisesRegex(ValueError, 'storage changed'):
                checkpoint.observations(self.cases[case_id], raw)

    def test_temporal_cases_require_original_clock_and_actual_instants(self):
        for case_id in ('CREATE-001', 'CREATE-003', 'PATCH-005', 'DELETE-001'):
            raw = self.raw(case_id)
            with self.subTest(case_id=case_id), self.assertRaisesRegex(ValueError, 'requires original controlled clock'):
                checkpoint.observations(self.cases[case_id], raw)
            next(item for item in raw if 'method' in item)['at'] = '2030-01-01T00:00:00Z'
            with self.subTest(case_id=case_id), self.assertRaisesRegex(ValueError, 'request clock'):
                checkpoint.observations(self.cases[case_id], raw, self.clock)

    def test_output_schema_requires_real_validator_result_and_exact_validated_body(self):
        for case_id in ('CREATE-001', 'READ-001'):
            for mutation in ('missing', 'rejected', 'body', 'validator', 'route'):
                raw = self.raw(case_id)
                item = next(item for item in raw if 'outputValidation' in item)
                if mutation == 'missing': raw.remove(item)
                elif mutation == 'rejected': item['outputValidation']['accepted'] = False
                elif mutation == 'body': item['outputValidation']['validatedBody']['title'] = 'foreign'
                else: item['outputValidation'][mutation] = 'foreign'
                with self.subTest(case_id=case_id, mutation=mutation):
                    if mutation in ('rejected', 'body'):
                        self.assertIsNone(checkpoint.observations(self.cases[case_id], raw, self.clock)[0]['steps'][0]['outputSchema'])
                    else:
                        with self.assertRaises(ValueError): checkpoint.observations(self.cases[case_id], raw, self.clock)

    def test_changed_due_and_reminder_clear_are_independent_stored_observations(self):
        raw = self.raw('PATCH-005')
        raw[-1]['persistence']['reminder_sent_at'] = '2026-01-15T11:59:00Z'
        step = checkpoint.observations(self.cases['PATCH-005'], raw, self.clock)[0]['steps'][0]
        self.assertEqual(step['storedDueAt'], '2026-02-15T09:00:00Z')
        self.assertEqual(step['storedReminderSentAt'], '2026-01-15T11:59:00Z')
        raw[0]['fixture']['reminder_sent_at'] = None
        with self.assertRaisesRegex(ValueError, 'previously reminded'):
            checkpoint.observations(self.cases['PATCH-005'], raw, self.clock)

    def test_soft_delete_requires_retained_row_and_actual_subsequent_get(self):
        raw = self.raw('DELETE-001')
        step = checkpoint.observations(self.cases['DELETE-001'], raw, self.clock)[0]['steps'][0]
        self.assertTrue(step['rowExists'])
        self.assertEqual(step['deletedAt'], self.clock)
        self.assertEqual(step['subsequentGetStatus'], 401)
        raw = [item for item in raw if not ('method' in item and item['method'] == 'GET')]
        with self.assertRaisesRegex(ValueError, 'Missing request'):
            checkpoint.observations(self.cases['DELETE-001'], raw, self.clock)

    def test_read_fixture_identity_and_delete_snapshot_order_are_bound(self):
        raw = self.raw('READ-001'); raw[0]['fixture']['owner_id'] = checkpoint.ALIASES['bob']
        with self.assertRaisesRegex(ValueError, 'owned read fixture'):
            checkpoint.observations(self.cases['READ-001'], raw, self.clock)
        raw = self.raw('DELETE-001'); raw[0], raw[1] = raw[1], raw[0]
        with self.assertRaisesRegex(ValueError, 'Reordered delete'):
            checkpoint.observations(self.cases['DELETE-001'], raw, self.clock)

    def test_relationship_variants_all_required_ordered_and_isolated(self):
        scenarios = self.raw('REL-001')
        observed = checkpoint.observations(self.cases['REL-001'], scenarios, self.clock)
        self.assertEqual([entry['variant_index'] for entry in observed], [0, 1, 2])
        self.assertEqual([entry['steps'][0]['childLimit'] for entry in observed], [100, 100, 100])
        for mutation in ('missing', 'order', 'name', 'fixture', 'failure'):
            raw = deepcopy(scenarios)
            if mutation == 'missing': raw.pop()
            elif mutation == 'order': raw[0], raw[1] = raw[1], raw[0]
            elif mutation == 'name': raw[0]['fixture_variant'] = 'foreign'
            elif mutation == 'fixture': raw[2]['raw'][0]['fixtureRows'].pop(0)
            else: raw[0]['adapterFailure'] = True
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                checkpoint.observations(self.cases['REL-001'], raw, self.clock)

    def test_child_limit_query_budget_order_and_cross_parent_output_are_observed(self):
        for mutation in ('limit', 'query', 'order', 'foreign', 'email'):
            raw = self.raw('REL-001')
            journal = raw[1]['raw']; reply = journal[1]
            if mutation == 'limit': reply['statements'][1]['numericParameters']['4'] = 11
            elif mutation == 'query': reply['statements'].append(deepcopy(reply['statements'][0]))
            elif mutation == 'order': reply['statements'][1]['sql'] = reply['statements'][1]['sql'].replace('ASC', 'DESC')
            elif mutation == 'foreign': reply['body']['todos'][0]['id'] = checkpoint.ALIASES['t2']
            else: reply['body']['email'] = 'private@example.test'
            # Keep the independent output-validator result consistent where the
            # test is challenging collection semantics rather than validation.
            journal[2]['outputValidation']['validatedBody'] = deepcopy(reply['body'])
            observations = checkpoint.observations(self.cases['REL-001'], raw, self.clock)
            with self.subTest(mutation=mutation):
                self.assertTrue(harness.assess(self.cases['REL-001'], observations) or any(step.get('checkpoint_failures') for scenario in observations for step in scenario['steps']))

    def test_cursor_request_templates_continuation_completeness_and_repeat_are_bound(self):
        for mutation in ('path', 'template', 'duplicate', 'gap', 'repeat', 'missing'):
            raw = self.raw('LIST-002')
            if mutation == 'path': raw[2]['path'] = raw[1]['path']
            elif mutation == 'template': raw[2]['requestedPath'] = 'foreign'
            elif mutation == 'duplicate': raw[2]['body']['items'][0] = deepcopy(raw[1]['body']['items'][0])
            elif mutation == 'gap': raw[3]['body']['items'] = []
            elif mutation == 'repeat': raw[4]['body']['items'] = []
            else: raw.pop()
            with self.subTest(mutation=mutation):
                if mutation in ('path', 'template', 'missing'):
                    with self.assertRaises(ValueError): checkpoint.observations(self.cases['LIST-002'], raw, self.clock)
                else:
                    observations = checkpoint.observations(self.cases['LIST-002'], raw, self.clock)
                    self.assertTrue(harness.assess(self.cases['LIST-002'], observations) or observations[0]['steps'][0].get('checkpoint_failures'))

    def test_list_fixture_owner_deleted_order_and_filters_are_not_inferred(self):
        for mutation in ('foreign', 'deleted', 'order', 'fixture'):
            raw = self.raw('LIST-001')
            if mutation == 'foreign': raw[1]['body']['items'][0]['id'] = checkpoint.ALIASES['t2']
            elif mutation == 'deleted': raw[1]['body']['items'][0]['id'] = checkpoint.ALIASES['t1']
            elif mutation == 'order': raw[1]['body']['items'].reverse()
            else: raw[0]['fixtureRows'] = [row for row in raw[0]['fixtureRows'] if row['owner_id'] != checkpoint.ALIASES['bob']]
            with self.subTest(mutation=mutation):
                if mutation == 'fixture':
                    with self.assertRaises(ValueError): checkpoint.observations(self.cases['LIST-001'], raw, self.clock)
                else:
                    observations = checkpoint.observations(self.cases['LIST-001'], raw, self.clock)
                    self.assertTrue(harness.assess(self.cases['LIST-001'], observations) or observations[0]['steps'][0].get('checkpoint_failures'))
        raw = self.raw('LIST-003'); raw[1]['body']['items'][0]['due_at'] = None
        self.assertTrue(harness.assess(self.cases['LIST-003'], checkpoint.observations(self.cases['LIST-003'], raw, self.clock)))

    def test_repeated_delete_requires_original_deleted_fixture_and_no_storage_change(self):
        raw = self.raw('DELETE-002'); raw[-1]['persistence']['title'] = 'changed'
        observations = checkpoint.observations(self.cases['DELETE-002'], raw, self.clock)
        self.assertTrue(observations[0]['steps'][0]['checkpoint_failures'])
        raw = self.raw('DELETE-002'); raw[0]['fixture']['deleted_at'] = None
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['DELETE-002'], raw, self.clock)

    def test_repeated_delete_rejects_reordered_fixture_request_and_persistence(self):
        original = self.raw('DELETE-002')
        for order in ((2, 1, 0), (1, 0, 2), (0, 2, 1)):
            with self.subTest(order=order):
                raw = [deepcopy(original[index]) for index in order]
                with self.assertRaisesRegex(ValueError, 'Reordered repeated-delete'):
                    checkpoint.observations(self.cases['DELETE-002'], raw, self.clock)

    def test_cursor_duplicate_fails_case_even_when_frozen_summary_fields_match(self):
        outputs = self.outputs()
        raw = next(item for item in outputs[0]['results'] if item['id'] == 'LIST-002')['raw']
        raw[3]['body']['items'].append(deepcopy(raw[1]['body']['items'][0]))
        observations = checkpoint.observations(self.cases['LIST-002'], raw, self.clock)
        self.assertFalse(harness.assess(self.cases['LIST-002'], observations))
        report = harness.run_cases(self.root); checkpoint.apply_checkpoint(report, outputs, self.root)
        entry = next(entry for entry in report['results'] if entry['id'] == 'LIST-002' and entry['backend'] == 'sqlite')
        self.assertEqual(entry['status'], 'failed')
        self.assertTrue(entry['failures'])


if __name__ == '__main__': unittest.main()
