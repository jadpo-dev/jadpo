"""Synthetic counterexamples for checkpoint integrity, not application passes."""
from copy import deepcopy
import hashlib
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
                     'tests/assurance/service-reference-mail-v0.1.json',
                     'tools/golden_checkpoint.py', 'tools/golden_http_checkpoint.ts',
                     'tools/golden_sql_observer.ts', 'tools/golden_startup_observer.ts'):
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
        self.tls_text = 'synthetic local TLS fixture; never used'
        self.tls_digest = hashlib.sha256(self.tls_text.encode()).hexdigest()

    def raw(self, case_id):
        case = self.cases[case_id]
        if case_id in ('AUTH-009', 'AUTH-011'): return self.service_raw(case_id)
        if case_id in ('AUTH-003', 'AUTH-004', 'AUTH-007'): return self.jwt_raw(case_id)
        if case_id == 'AUTH-008': return self.cache_raw()
        if case_id == 'USER-001':
            return self.user_timeline_raw()
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
        if case_id in ('PATCH-002', 'PATCH-005'):
            fixture = next(item['fixture'] for item in replies if 'fixture' in item)
            persisted = next(item for item in replies if 'persistence' in item)
            fixture.update(status='open', created_at='2026-01-15T11:00:00.000Z', updated_at='2026-01-15T11:00:00.000Z')
            if case_id == 'PATCH-005': fixture['reminder_sent_at'] = '2026-01-15T11:59:00.000Z'
            before = deepcopy(fixture); before['reminder_sent_at'] = None
            body = {field: before[field] for field in ('id', 'title', 'status', 'due_at', 'created_at', 'updated_at')}
            creation = {'method': 'POST', 'path': '/todos', 'auth': 'alice-session', 'status': 201, 'at': '2026-01-15T11:00:00Z', 'requestJson': {'title': 'original fixture title', 'due_at': '2026-02-01T00:00:00Z'}, 'body': body, 'authenticationAttempts': 1, 'principals': [self.principal()], 'credentialsIssued': 0, 'statements': [{'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'INSERT', 'sql': 'INSERT INTO "todo"'}, {'operation': 'INSERT', 'sql': 'INSERT INTO "__jadpo_delivery_schedules_v1"'}, {'operation': 'COMMIT', 'sql': 'COMMIT'}]}
            enrollment = {'source_entity': 'Todo', 'source_entity_id': fixture['id'], 'schedule_revision': '1'}
            capture = {'logicalAlias': 't1', 'before': before, 'after': deepcopy(fixture), 'enrollment': enrollment, 'enrollmentAfterSetup': deepcopy(enrollment), 'setupStatements': [{'operation': 'UPDATE', 'sql': 'UPDATE todo SET reminder_sent_at = $1 WHERE id = $2'}] if case_id == 'PATCH-005' else [], 'validator': 'validate_TodoView', 'validatedBody': deepcopy(body), 'requestCredentialIssuance': {'at': self.clock, 'expires': '2026-01-15T12:05:00Z', 'strategy': 'browser_session', 'credentialsIssued': 1, 'statements': [{'operation': 'INSERT', 'sql': 'INSERT INTO "__jadpo_auth_sessions"'}]}}
            next(item for item in replies if 'method' in item)['statements'].append({'operation': 'UPDATE', 'sql': 'UPDATE "__jadpo_delivery_schedules_v1"'})
            persisted['enrollmentAfterPrimary'] = {**enrollment, 'schedule_revision': '2'}
            replies[:0] = [{'fixtureCreation': creation}, {'fixtureEnrollment': capture}]
        if case_id in ('AUTH-002', 'AUTH-006', 'AUTH-010'):
            replies.insert(0, {'authenticationFixture': self.authentication_fixture(case_id)})
            reply = replies[1]
            reply.update(principals=[self.principal()], credentialsIssued=0, authorityQueries={'principal': [], 'credential': []})
            if case_id == 'AUTH-002':
                reply.update(status=200, body={'items': [], 'next': None})
                replies.append({'outputValidation': {'route': 'GET /todos', 'schema': 'TodoPage', 'validator': 'validate_TodoPage', 'accepted': True, 'validatedBody': deepcopy(reply['body'])}})
            if case_id == 'AUTH-006':
                reply.update(status=403, body={'error': {'code': 'user_disabled'}}, principals=[], statements=self.authority_statements(), authorityQueries={'principal': [1], 'credential': [0]})
                disabled = deepcopy(replies[0]['authenticationFixture']['user']); disabled.update(status='disabled', disabled_at=case['timeline']['disabledAt'])
                replies.insert(1, {'disabledUser': disabled, 'at': case['timeline']['disabledAt'], 'sessionRecord': deepcopy(replies[0]['authenticationFixture']['sessionRecord'])})
            if case_id == 'AUTH-010':
                reply.update(status=204, body=None, statements=[{'operation': 'SELECT', 'sql': 'SELECT data FROM "__jadpo_auth_sessions" WHERE id = $1'}, {'operation': 'SELECT', 'sql': 'SELECT id FROM "user" WHERE "authentication_subject" = $1'}], authorityQueries={'principal': [1], 'credential': [0]})
        return replies

    def principal(self):
        return {'kind': 'user', 'subject': 'alice', 'authenticationStrength': 'primary', 'values': {'user_id': checkpoint.ALIASES['alice']}}

    def authority_statements(self):
        return [{'operation': 'SELECT', 'sql': 'SELECT data FROM "__jadpo_auth_sessions" WHERE id = $1'}, {'operation': 'SELECT', 'sql': 'SELECT id FROM "user" WHERE "authentication_subject" = $1'}]

    def authentication_fixture(self, case_id):
        case = self.cases[case_id]
        expires = case.get('timeline', {}).get('credentialExpiresAt', '2026-01-15T12:05:00Z')
        return {'user': {'id': checkpoint.ALIASES['alice'], 'authentication_subject': 'alice', 'status': 'active', 'disabled_at': None},
                'sessionRecord': {'strategy': 'api_bearer' if case_id in ('AUTH-010', 'AUTH-008') else 'browser_session', 'subject': 'alice', 'userId': checkpoint.ALIASES['alice'], 'expires': expires, 'revoked': False},
                'issuedAt': case.get('timeline', {}).get('issuedAt', self.clock), 'credentialExpiresAt': expires,
                'issuanceObservation': {'operation': 'compiler-owned authentication issuance', 'strategy': 'api_bearer' if case_id in ('AUTH-010', 'AUTH-008') else 'browser_session', 'credentialsIssued': 1, 'statements': [{'operation': 'INSERT', 'sql': 'INSERT INTO "__jadpo_auth_sessions" VALUES (?)'}]},
                'specification': {'fixture': case['fixture'], 'timeline': case.get('timeline')}}

    def jwt_raw(self, case_id):
        case = self.cases[case_id]; subject = 'bob' if case_id == 'AUTH-004' else 'alice'; issuer = 'https://127.0.0.1:44321'
        key = {'kty': 'RSA', 'kid': 'synthetic-rsa', 'alg': 'RS256', 'use': 'sig', 'n': 'synthetic', 'e': 'AQAB'}
        claims = {'iss': issuer, 'aud': 'synthetic-todo', 'sub': subject, 'iat': 1768478400, 'exp': 1768478520}
        if case_id == 'AUTH-007': claims['email'] = 'attacker@example.test'
        credential = {'name': subject+'-jwt', 'header': {'alg': 'RS256', 'kid': key['kid'], 'typ': 'JWT'}, 'claims': claims, 'publicJwk': key}
        users = [{'id': checkpoint.ALIASES[name], 'authentication_subject': name, 'email': name+'@example.test', 'status': 'active'} for name in ('alice', 'bob')]
        fixture = {'credentials': [credential], 'users': users, 'issuer': issuer, 'audience': claims['aud'], 'certificateSha256': self.tls_digest,
                   'specification': {'fixture': case['fixture'], 'request': case['request'], 'clock': self.clock}}
        principal = self.principal(); principal.update(subject=subject, values={'user_id': checkpoint.ALIASES[subject]})
        request = {'method': 'GET', 'path': '/todos', 'auth': 'bob-jwt'} if case_id == 'AUTH-004' else case['request']
        reply = {'method': request['method'], 'path': checkpoint.request_path(request['path']), 'auth': request['auth'], 'requestJson': None, 'at': self.clock,
                 'status': 200, 'body': {'user_id': checkpoint.ALIASES['alice'], 'todos': []} if case_id == 'AUTH-007' else {'items': [], 'next': None},
                 'statements': [self.authority_statements()[1]], 'authenticationAttempts': 1, 'credentialsIssued': 0, 'principals': [principal], 'authorityQueries': {'principal': [0], 'credential': []},
                 'storageReads': [], 'jwtVerifications': [{'strategy': 'api_bearer', 'subject': subject, 'issuer': issuer, 'audience': claims['aud'], 'issuedAt': claims['iat'], 'expires': claims['exp']}],
                 'providerRequests': [{'method': 'GET', 'path': '/.well-known/openid-configuration', 'authorizationPresent': False, 'status': 200, 'body': {'issuer': issuer, 'jwks_uri': issuer+'/jwks'}}, {'method': 'GET', 'path': '/jwks', 'authorizationPresent': False, 'status': 200, 'body': {'keys': [key]}}]}
        raw = [{'jwtFixture': fixture}]
        if case_id == 'AUTH-004':
            raw.append({'jwtValidityControl': reply})
            conflict = deepcopy(reply); conflict.update(auth=case['request']['auth'], status=401, body={'error': {'code': 'ambiguous_credentials'}}, statements=[], principals=[], jwtVerifications=[], providerRequests=[])
            raw.append(conflict)
        else:
            raw.append(reply)
            schema = 'UserWithTodos' if case_id == 'AUTH-007' else 'TodoPage'
            raw.append({'outputValidation': {'route': 'GET /users/{user_id}/todos' if case_id == 'AUTH-007' else 'GET /todos', 'schema': schema, 'validator': 'validate_'+schema, 'accepted': True, 'validatedBody': deepcopy(reply['body'])}})
            if case_id == 'AUTH-007': raw.append({'provisioningPersistence': deepcopy(users)})
        return raw

    def cache_raw(self):
        case = self.cases['AUTH-008']
        control = {'method': 'DELETE', 'path': checkpoint.request_path('/users/alice'), 'auth': 'alice-bounded-bearer', 'requestJson': None, 'at': self.clock, 'status': 503, 'body': {'error': {'code': 'authentication_unavailable'}},
                   'statements': [], 'authenticationAttempts': 1, 'principals': [], 'credentialsIssued': 0, 'storageReads': [{'operation': 'session.get', 'outcome': 'faulted'}], 'faultConfiguration': {'sessionStorageUnavailable': True}}
        reply = deepcopy(control); reply.update(method='GET', path=case['request']['path'], status=200, body={'items': [], 'next': None}, principals=[self.principal()], storageReads=[], authorityQueries={'principal': [], 'credential': []})
        return [{'authenticationFixture': self.authentication_fixture('AUTH-008')}, {'cacheFaultControl': control}, reply]

    def service_raw(self, case_id):
        case = self.cases[case_id]
        principal = {'kind': 'service', 'subject': 'reporting', 'authenticationStrength': 'primary', 'values': {'service_id': checkpoint.ALIASES['reporting']}}
        service = {'id': checkpoint.ALIASES['reporting'], 'owner_id': checkpoint.ALIASES['alice'], 'name': 'reporting', 'status': 'active', 'disabled_at': None}
        current = deepcopy(service)
        if case_id == 'AUTH-011': current.update(status='disabled', disabled_at=self.clock)
        credential = {'id': '00000000-0000-4000-8000-000000000042', 'service_id': service['id'], 'status': 'active', 'expires_at': '2026-01-15T12:05:00Z', 'revoked_at': None, 'privateMetadataRevoked': False}
        statements = [{'operation': 'SELECT', 'sql': 'SELECT data FROM "__jadpo_auth_service_credentials" WHERE id = $1'}, {'operation': 'SELECT', 'sql': 'SELECT * FROM "service_credential" WHERE id = $1'}, {'operation': 'SELECT', 'sql': 'SELECT id FROM "service" WHERE "name" = $1'}]
        fixture = {'issuedService': service, 'service': current, 'credential': credential, 'issuance': {'credentialsIssued': 1, 'authorityPrincipals': [principal], 'statements': [{'operation': 'INSERT', 'sql': 'INSERT INTO "service_credential" VALUES (?)'}]},
                   'validityControl': {'completed': True, 'authorityPrincipals': [principal], 'statements': deepcopy(statements), 'at': self.clock}, 'at': self.clock,
                   'specification': {'fixture': case['fixture'], 'request': case['request'], 'clock': self.clock}}
        reply = {'method': 'POST', 'path': '/auth/exchange', 'auth': 'reporting-service-key', 'requestJson': None, 'at': self.clock, 'status': 200 if case_id == 'AUTH-009' else 403,
                 'statements': statements, 'authenticationAttempts': 0, 'authorityPrincipals': [principal] if case_id == 'AUTH-009' else [], 'credentialsIssued': 1 if case_id == 'AUTH-009' else 0,
                 'body': {'error': {'code': 'service_disabled'}}}
        raw = [{'serviceFixture': fixture}, reply]
        if case_id == 'AUTH-009':
            token = {'sha256': 'a'*64, 'byteLength': 512, 'representation': 'jdx1'}
            reply.update(body={'access_token': token, 'token_type': 'Bearer', 'expires_at': '2026-01-15T12:05:00Z'}, exchangeDisclosure={'incomingSecretInResponse': False, 'responseFields': ['access_token', 'token_type', 'expires_at'], 'returnedCredential': deepcopy(token)})
            after = deepcopy(credential); after['revoked_at'] = self.clock
            raw.append({'serviceRevocation': {'before': deepcopy(credential), 'after': after, 'statements': [{'operation': 'UPDATE', 'sql': 'UPDATE "service_credential" SET revoked_at = $1'}]}})
            raw.append({'serviceRevokedControl': {'method': 'POST', 'path': '/auth/exchange', 'auth': 'reporting-service-key', 'requestJson': None, 'at': self.clock, 'status': 401, 'body': {'error': {'code': 'invalid_credentials'}}, 'authorityPrincipals': [], 'credentialsIssued': 0, 'statements': deepcopy(statements[:2])}})
        return raw

    def user_timeline_raw(self):
        case = self.cases['USER-001']; fixture = self.authentication_fixture('USER-001')
        todo = {'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice'], 'deleted_at': None}
        disabled = deepcopy(fixture['user']); disabled.update(status='disabled', disabled_at=case['timeline']['disabledAt'])
        def reply(request, at, status, body, principals):
            return {'method': request['method'], 'path': checkpoint.request_path(request['path']), 'auth': request['auth'], 'requestJson': None,
                    'at': at, 'status': status, 'body': body, 'statements': [] if status == 401 else self.authority_statements(), 'authenticationAttempts': 1, 'principals': principals, 'credentialsIssued': 0,
                    'authorityQueries': {'principal': [], 'credential': []} if status == 401 else {'principal': [1], 'credential': [0]}}
        follow = case['followUps']
        return [{'authenticationFixture': fixture}, {'disableFixture': {'user': deepcopy(fixture['user']), 'todo': deepcopy(todo)}},
                reply(case['request'], case['timeline']['disabledAt'], 204, None, [self.principal()]),
                {'disabledPersistence': {'user': disabled, 'todo': deepcopy(todo), 'sessionRecord': deepcopy(fixture['sessionRecord'])}},
                reply(follow[0]['request'], follow[0]['at'], 403, {'error': {'code': 'user_disabled'}}, []),
                {'operation': follow[1]['operation'], 'auth': follow[1]['auth'], 'at': follow[1]['at'], 'completed': False,
                 'fault': {'failure': 'UserDisabled', 'nativeCode': 'principal_inactive', 'status': 401},
                 'failureMetadata': {'failure': 'UserDisabled', 'code': 'user_disabled'},
                 'statements': self.authority_statements(), 'authenticationAttempts': 0, 'principals': [], 'credentialsIssued': 0, 'authorityQueries': {'principal': [1], 'credential': [0]}},
                reply(follow[2]['request'], follow[2]['at'], 401, {'error': {'code': 'invalid_credentials'}}, [])]

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
            for generated, retained in (('persistence.ts', 'original-persistence.ts'), ('first-party-authentication.ts', 'original-authentication.ts'), ('jwt-authentication.ts', 'original-jwt-authentication.ts'), ('app.ts', 'original-app.ts')):
                (target/generated).write_text('synthetic instrumented')
                (evidence/retained).write_text('synthetic original')
            plan_path = evidence/'application/build/persistence/entities.json'; plan_path.parent.mkdir()
            plan_path.write_text(json.dumps({'query_plans': self.query_plans}))
            failure_path = evidence/'application/build/audit/failures.json'; failure_path.parent.mkdir()
            failure_path.write_text(json.dumps({'failures': [{'failure': 'UserDisabled', 'code': 'user_disabled'}]}))
            outputs = self.outputs()
            for output in outputs: (evidence/f'{output["backend"]}-raw.json').write_text(json.dumps(output))
            report = harness.run_cases(self.root); checkpoint.apply_checkpoint(report, outputs, self.root)
            original = checkpoint.hashes(evidence/'application/build')
            original['target/persistence.ts'] = harness.digest(evidence/'original-persistence.ts')
            original['target/first-party-authentication.ts'] = harness.digest(evidence/'original-authentication.ts')
            original['target/app.ts'] = harness.digest(evidence/'original-app.ts')
            original['target/jwt-authentication.ts'] = harness.digest(evidence/'original-jwt-authentication.ts')
            for name in ('jwks-fixture-cert.pem', 'jwks-fixture-key.pem'): (evidence/name).write_text(self.tls_text)
            for name in ('owner-mutation-source', 'owner-policy-projection'):
                (evidence/name).mkdir(); (evidence/name/'synthetic.jadpo').write_text('synthetic partial probe; never executed')
            (evidence/'owner-partial-probe.json').write_text('{"synthetic":true}')
            report['checkpoint'] = {'supported_ids': list(checkpoint.SUPPORTED), 'generated_original': original,
                                    'generated_instrumented': checkpoint.hashes(evidence/'application/build'),
                                    'raw_files': {f'{backend}-raw.json': harness.digest(evidence/f'{backend}-raw.json') for backend in harness.BACKENDS},
                                    'dependency_files': checkpoint.hashes(jose), 'tls_fixture': {name: harness.digest(evidence/name) for name in ('jwks-fixture-cert.pem', 'jwks-fixture-key.pem')},
                                    'owner_partial_sources': {name: checkpoint.hashes(evidence/name) for name in ('owner-mutation-source', 'owner-policy-projection')}, 'owner_partial_probe_sha256': harness.digest(evidence/'owner-partial-probe.json')}
            checkpoint.validate_checkpoint(report, evidence, dependencies, self.root)
            for path in (target/'persistence.ts', evidence/'original-authentication.ts', evidence/'original-jwt-authentication.ts', evidence/'jwks-fixture-cert.pem', evidence/'sqlite-raw.json', jose/'package.json'):
                previous = path.read_bytes(); path.write_bytes(previous+b'\n')
                with self.subTest(path=path.name), self.assertRaises(ValueError): checkpoint.validate_checkpoint(report, evidence, dependencies, self.root)
                path.write_bytes(previous)
            changed = deepcopy(report)
            entry = next(entry for entry in changed['results'] if entry['id'] == 'AUTH-001' and entry['backend'] == 'sqlite')
            entry['observations'][0]['steps'][0]['writes'] = 99
            with self.assertRaisesRegex(ValueError, 'differ from raw'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies, self.root)
            # Even coherently rehashed raw cannot replace the compiler's
            # declared native-fault to public-failure metadata.
            changed_outputs = deepcopy(outputs)
            timeline = next(item for item in changed_outputs[0]['results'] if item['id'] == 'USER-001')['raw']
            timeline[5]['failureMetadata']['code'] = 'foreign_public_code'
            changed = deepcopy(report)
            raw_path = evidence/'sqlite-raw.json'; original_raw = raw_path.read_bytes()
            raw_path.write_text(json.dumps(changed_outputs[0]))
            changed['checkpoint']['raw_files']['sqlite-raw.json'] = harness.digest(raw_path)
            checkpoint.apply_checkpoint(changed, changed_outputs, self.root)
            with self.assertRaisesRegex(ValueError, 'refresh failure mapping'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies, self.root)
            raw_path.write_bytes(original_raw)
            changed_outputs = deepcopy(outputs)
            jwt = next(item for item in changed_outputs[0]['results'] if item['id'] == 'AUTH-003')['raw']
            jwt[0]['jwtFixture']['certificateSha256'] = 'unbound-local-trust'
            changed = deepcopy(report); raw_path.write_text(json.dumps(changed_outputs[0]))
            changed['checkpoint']['raw_files']['sqlite-raw.json'] = harness.digest(raw_path)
            checkpoint.apply_checkpoint(changed, changed_outputs, self.root)
            with self.assertRaisesRegex(ValueError, 'JWT issuer trust'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies, self.root)
            raw_path.write_bytes(original_raw)
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
        next(item['fixture'] for item in raw if 'fixture' in item)['reminder_sent_at'] = None
        with self.assertRaises(ValueError):
            checkpoint.observations(self.cases['PATCH-005'], raw, self.clock)

    def test_native_patch_fixture_requires_real_creation_identity_enrollment_and_setup(self):
        for case_id in ('PATCH-002', 'PATCH-005'):
            for mutation in ('creation_missing', 'creation_order', 'status', 'identity', 'clock', 'validator', 'insert', 'schedule_insert', 'commit', 'source_identity', 'before_revision', 'setup_revision', 'after_revision', 'advance', 'credentials', 'setup_state', 'principal', 'transaction_order'):
                raw = self.raw(case_id)
                creation = next(item for item in raw if 'fixtureCreation' in item)
                capture = next(item['fixtureEnrollment'] for item in raw if 'fixtureEnrollment' in item)
                reply = next(item for item in raw if 'method' in item)
                retained = next(item for item in raw if 'persistence' in item)
                if mutation == 'creation_missing': raw.remove(creation)
                elif mutation == 'creation_order': raw.append(raw.pop(0))
                elif mutation == 'status': creation['fixtureCreation']['status'] = 200
                elif mutation == 'identity': creation['fixtureCreation']['body']['id'] = 'foreign'
                elif mutation == 'clock': creation['fixtureCreation']['at'] = self.clock
                elif mutation == 'validator': capture['validator'] = 'validate_Foreign'
                elif mutation in ('insert', 'schedule_insert', 'commit'):
                    remove = {'insert': 'INSERT INTO "todo"', 'schedule_insert': 'INSERT INTO "__jadpo_delivery_schedules_v1"', 'commit': 'COMMIT'}[mutation]
                    creation['fixtureCreation']['statements'] = [row for row in creation['fixtureCreation']['statements'] if row['sql'] != remove]
                elif mutation == 'source_identity': capture['enrollment']['source_entity_id'] = checkpoint.ALIASES['bob']
                elif mutation == 'before_revision': capture['enrollment']['schedule_revision'] = '9'
                elif mutation == 'setup_revision': capture['enrollmentAfterSetup']['schedule_revision'] = '2'
                elif mutation == 'after_revision': retained['enrollmentAfterPrimary']['schedule_revision'] = '1'
                elif mutation == 'advance': reply['statements'] = []
                elif mutation == 'credentials': capture['requestCredentialIssuance']['credentialsIssued'] = 0
                elif mutation == 'setup_state': capture['after']['owner_id'] = checkpoint.ALIASES['bob']
                elif mutation == 'principal': creation['fixtureCreation']['principals'] = []
                elif mutation == 'transaction_order': creation['fixtureCreation']['statements'].reverse()
                with self.subTest(case_id=case_id, mutation=mutation), self.assertRaises(ValueError):
                    checkpoint.observations(self.cases[case_id], raw, self.clock)

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

    def test_native_principal_identity_and_actual_validator_determine_auth_output(self):
        raw = self.raw('AUTH-002')
        step = checkpoint.observations(self.cases['AUTH-002'], raw, self.clock)[0]['steps'][0]
        self.assertEqual(step['principalUser'], 'alice')
        raw[1]['principals'][0]['values']['user_id'] = checkpoint.ALIASES['bob']
        self.assertEqual(checkpoint.observations(self.cases['AUTH-002'], raw, self.clock)[0]['steps'][0]['principalUser'], 'bob')
        raw[-1]['outputValidation']['accepted'] = False
        self.assertTrue(checkpoint.observations(self.cases['AUTH-002'], raw, self.clock)[0]['steps'][0]['providerFieldsInOutput'])
        for mutation in ('missing', 'multiple', 'private'):
            raw = self.raw('AUTH-002')
            if mutation == 'missing': del raw[1]['principals']
            elif mutation == 'multiple': raw[1]['principals'].append(self.principal())
            else: raw[1]['principals'][0]['values']['provider_claim'] = 'foreign'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-002'], raw, self.clock)

    def test_disabled_authority_fixture_clock_session_and_chronology_are_bound(self):
        for mutation in ('active', 'revoked', 'issued', 'expiry', 'disabled', 'order', 'specification', 'issuance', 'storage'):
            raw = self.raw('AUTH-006')
            if mutation == 'active': raw[0]['authenticationFixture']['user']['status'] = 'disabled'
            elif mutation == 'revoked': raw[1]['sessionRecord']['revoked'] = True
            elif mutation == 'issued': raw[0]['authenticationFixture']['issuedAt'] = self.clock
            elif mutation == 'expiry': raw[0]['authenticationFixture']['credentialExpiresAt'] = self.clock
            elif mutation == 'disabled': raw[1]['disabledUser']['disabled_at'] = self.clock
            elif mutation == 'order': raw.reverse()
            elif mutation == 'specification': raw[0]['authenticationFixture']['specification']['fixture'] = []
            elif mutation == 'issuance': raw[0]['authenticationFixture']['issuanceObservation']['credentialsIssued'] = 0
            else: raw[0]['authenticationFixture']['issuanceObservation']['statements'] = []
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-006'], raw, self.clock)

    def test_fresh_authority_counts_are_reconstructed_from_physical_sql(self):
        raw = self.raw('AUTH-010')
        step = checkpoint.observations(self.cases['AUTH-010'], raw, self.clock)[0]['steps'][0]
        self.assertEqual((step['principalAuthorityQueries'], step['credentialAuthorityQueries']), (1, 1))
        raw[1]['statements'].append(deepcopy(raw[1]['statements'][1])); raw[1]['authorityQueries']['principal'].append(2)
        self.assertEqual(checkpoint.observations(self.cases['AUTH-010'], raw, self.clock)[0]['steps'][0]['principalAuthorityQueries'], 2)
        raw[1]['authorityQueries']['credential'] = []
        with self.assertRaisesRegex(ValueError, 'query accounting'): checkpoint.observations(self.cases['AUTH-010'], raw, self.clock)
        raw = self.raw('AUTH-010'); del raw[1]['authorityQueries']
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-010'], raw, self.clock)

    def test_full_user_timeline_retains_all_steps_clocks_and_refresh_issuance(self):
        raw = self.raw('USER-001')
        observations = checkpoint.observations(self.cases['USER-001'], raw, self.clock)
        self.assertEqual(len(observations[0]['steps']), 4)
        self.assertFalse(harness.assess(self.cases['USER-001'], observations))
        raw[5]['credentialsIssued'] = 1
        self.assertTrue(harness.assess(self.cases['USER-001'], checkpoint.observations(self.cases['USER-001'], raw, self.clock)))
        for mutation in ('missing', 'clock', 'request', 'operation', 'fault', 'mapping', 'session', 'order', 'authority', 'principal'):
            raw = self.raw('USER-001')
            if mutation == 'missing': raw.pop(5)
            elif mutation == 'clock': raw[5]['at'] = self.clock
            elif mutation == 'request': raw[4]['method'] = 'GET'
            elif mutation == 'operation': raw[5]['operation'] = 'synthetic refresh'
            elif mutation == 'fault': raw[5]['fault']['nativeCode'] = 'invalid_credentials'
            elif mutation == 'mapping': raw[5]['failureMetadata']['failure'] = 'ServiceDisabled'
            elif mutation == 'session': raw[3]['disabledPersistence']['sessionRecord']['revoked'] = True
            elif mutation == 'order': raw[2], raw[3] = raw[3], raw[2]
            elif mutation == 'authority': raw[5].update(statements=[], authorityQueries={'principal': [], 'credential': []})
            else: raw[2]['principals'] = []
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['USER-001'], raw, self.clock)

    def test_jwt_principal_requires_actual_native_verification_and_real_provider_journal(self):
        self.assertFalse(harness.assess(self.cases['AUTH-003'], checkpoint.observations(self.cases['AUTH-003'], self.raw('AUTH-003'), self.clock)))
        for mutation in ('verified', 'provider', 'key', 'subject', 'issuer', 'specification', 'order'):
            raw = self.raw('AUTH-003')
            if mutation == 'verified': raw[1]['jwtVerifications'] = []
            elif mutation == 'provider': raw[1]['providerRequests'].reverse()
            elif mutation == 'key': raw[0]['jwtFixture']['credentials'][0]['publicJwk']['d'] = 'private-key'
            elif mutation == 'subject': raw[1]['principals'][0]['values']['user_id'] = checkpoint.ALIASES['bob']
            elif mutation == 'issuer': raw[0]['jwtFixture']['issuer'] = 'http://127.0.0.1:44321'
            elif mutation == 'specification': raw[0]['jwtFixture']['specification']['clock'] = '2030-01-01T00:00:00Z'
            else: raw[0], raw[1] = raw[1], raw[0]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-003'], raw, self.clock)

    def test_conflicting_credentials_require_real_valid_bob_control_and_no_primary_validation(self):
        self.assertFalse(harness.assess(self.cases['AUTH-004'], checkpoint.observations(self.cases['AUTH-004'], self.raw('AUTH-004'), self.clock)))
        for mutation in ('missing', 'status', 'subject', 'primary', 'order'):
            raw = self.raw('AUTH-004')
            if mutation == 'missing': raw.pop(1)
            elif mutation == 'status': raw[1]['jwtValidityControl']['status'] = 401
            elif mutation == 'subject': raw[1]['jwtValidityControl']['jwtVerifications'][0]['subject'] = 'alice'
            elif mutation == 'primary': raw[2]['jwtVerifications'] = deepcopy(raw[1]['jwtValidityControl']['jwtVerifications'])
            else: raw[1], raw[2] = raw[2], raw[1]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-004'], raw, self.clock)

    def test_provisioning_email_separation_uses_database_post_state_and_actual_output(self):
        raw = self.raw('AUTH-007')
        self.assertFalse(harness.assess(self.cases['AUTH-007'], checkpoint.observations(self.cases['AUTH-007'], raw, self.clock)))
        raw[-1]['provisioningPersistence'][0]['email'] = 'attacker@example.test'
        self.assertTrue(checkpoint.observations(self.cases['AUTH-007'], raw, self.clock)[0]['steps'][0]['providerEmailUsed'])
        raw = self.raw('AUTH-007'); raw[1]['body']['email'] = 'attacker@example.test'
        step = checkpoint.observations(self.cases['AUTH-007'], raw, self.clock)[0]['steps'][0]
        self.assertEqual(step['privateFieldsAbsent'], [])
        self.assertTrue(step['providerEmailUsed'])
        raw = self.raw('AUTH-007'); raw[0]['jwtFixture']['credentials'][0]['claims'].pop('email')
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-007'], raw, self.clock)

    def test_bounded_bearer_requires_active_real_cache_fault_and_uncalled_primary_boundary(self):
        self.assertFalse(harness.assess(self.cases['AUTH-008'], checkpoint.observations(self.cases['AUTH-008'], self.raw('AUTH-008'), self.clock)))
        for mutation in ('control', 'fault', 'primary', 'authority', 'status', 'order'):
            raw = self.raw('AUTH-008')
            if mutation == 'control': raw.pop(1)
            elif mutation == 'fault': raw[1]['cacheFaultControl']['faultConfiguration']['sessionStorageUnavailable'] = False
            elif mutation == 'primary': raw[2]['storageReads'] = [{'operation': 'session.get', 'outcome': 'delegated'}]
            elif mutation == 'authority': raw[1]['cacheFaultControl']['statements'] = self.authority_statements()
            elif mutation == 'status': raw[1]['cacheFaultControl']['status'] = 200
            else: raw[1], raw[2] = raw[2], raw[1]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-008'], raw, self.clock)

    def test_service_exchange_identity_disclosure_and_native_issuance_are_observed(self):
        raw = self.raw('AUTH-009')
        self.assertFalse(harness.assess(self.cases['AUTH-009'], checkpoint.observations(self.cases['AUTH-009'], raw, self.clock)))
        raw[1]['exchangeDisclosure']['incomingSecretInResponse'] = True
        self.assertTrue(harness.assess(self.cases['AUTH-009'], checkpoint.observations(self.cases['AUTH-009'], raw, self.clock)))
        for mutation in ('principal', 'issued', 'disclosure', 'key', 'validity', 'private', 'order'):
            raw = self.raw('AUTH-009')
            if mutation == 'principal': raw[1]['authorityPrincipals'][0]['values']['service_id'] = checkpoint.ALIASES['alice']
            elif mutation == 'issued': raw[1]['credentialsIssued'] = 0
            elif mutation == 'disclosure': raw[1]['exchangeDisclosure']['returnedCredential']['sha256'] = 'unbound'
            elif mutation == 'key': raw[0]['serviceFixture']['credential']['revoked_at'] = self.clock
            elif mutation == 'validity': raw[0]['serviceFixture']['validityControl']['completed'] = False
            elif mutation == 'private': raw[0]['serviceFixture']['credential']['privateMetadataRevoked'] = True
            else: raw[0], raw[1] = raw[1], raw[0]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-009'], raw, self.clock)

    def test_service_revocation_control_uses_declared_post_state_and_refuses_stale_cache(self):
        for mutation in ('missing', 'unchanged', 'principal', 'issuance', 'query', 'clock'):
            raw = self.raw('AUTH-009')
            if mutation == 'missing': raw.pop(2)
            elif mutation == 'unchanged': raw[2]['serviceRevocation']['after']['revoked_at'] = None
            elif mutation == 'principal': raw[3]['serviceRevokedControl']['authorityPrincipals'] = deepcopy(raw[1]['authorityPrincipals'])
            elif mutation == 'issuance': raw[3]['serviceRevokedControl']['credentialsIssued'] = 1
            elif mutation == 'query': raw[3]['serviceRevokedControl']['statements'] = []
            else: raw[3]['serviceRevokedControl']['at'] = '2030-01-01T00:00:00Z'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-009'], raw, self.clock)

    def test_disabled_service_is_validly_issued_then_rejected_by_real_authority(self):
        self.assertFalse(harness.assess(self.cases['AUTH-011'], checkpoint.observations(self.cases['AUTH-011'], self.raw('AUTH-011'), self.clock)))
        for mutation in ('disabled', 'authority', 'issued', 'service', 'source'):
            raw = self.raw('AUTH-011')
            if mutation == 'disabled': raw[0]['serviceFixture']['service']['status'] = 'active'
            elif mutation == 'authority': raw[1]['statements'] = []
            elif mutation == 'issued': raw[1]['credentialsIssued'] = 1
            elif mutation == 'service': raw[0]['serviceFixture']['credential']['service_id'] = checkpoint.ALIASES['bob']
            else: raw[0]['serviceFixture']['specification']['fixture'] = []
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['AUTH-011'], raw, self.clock)

    def test_relationship_partial_observations_never_promote_original_case_entries(self):
        outputs = self.outputs()
        for output in outputs:
            output['partialEvidence'] = [{'id': case_id, 'raw': [{'accepted': False, 'fault': {'kind': 'constraint'}}]} for case_id in ('REL-002', 'REL-003', 'REL-004')]
        report = harness.run_cases(self.root); checkpoint.apply_checkpoint(report, outputs, self.root)
        for entry in report['results']:
            if entry['id'] in ('REL-002', 'REL-003', 'REL-004'):
                self.assertEqual(entry['status'], 'not_run'); self.assertNotIn('observations', entry)

    def partial_outputs_fixture(self):
        output = {'partialEvidence': [{'id': case_id, 'adapterFailure': True, 'raw': []} for case_id in ('REL-002', 'REL-003', 'REL-004')]}
        for case_id in ('CONFIG-001', 'CONFIG-002'):
            environment = self.cases[case_id]['environment']
            probes = [{'label': label, 'originalEnvironment': overrides, 'exitCode': 1, 'jsonEvents': [{'kind': 'checkpoint_startup_observation', 'exitCode': 1, 'listenerStarts': 0}], 'outputSha256': 'a'*64, 'outputBytes': 120, 'credentialTextDisclosed': False} for label, overrides in (('healthy_control', {}), ('original_environment', environment))]
            output['partialEvidence'].append({'id': case_id, 'raw': [{'nativeStartup': {'specification': environment, 'probes': probes}}]})
        case = self.cases['BOUNDARY-001']
        variants = [{'variant_index': 0, 'fixture_variant': case['fixtureVariants'][0], 'healthyControl': {'status': 200}, 'outage': {'response': {'status': 500}}}, {'variant_index': 1, 'fixture_variant': case['fixtureVariants'][1], 'providerCalls': [{'responseStatus': 401, 'authorizationPresent': True}], 'profiles': {'activation': {'executionMs': 60000, 'leaseMs': 40000}, 'delivery': {'executionMs': 60000, 'leaseMs': 40000}}, 'nativeFixtureCreation': {'status': 201}, 'nativeHostOutcome': {'selected': 1, 'attempted': 1}}]
        output['partialEvidence'].append({'id': 'BOUNDARY-001', 'raw': [{'boundaryVariants': variants, 'specification': {'fixtureVariants': case['fixtureVariants'], 'requestOrRun': case['requestOrRun']}}]})
        return output

    def test_partial_startup_and_boundary_inventory_and_causal_controls_are_bound(self):
        checkpoint.validate_partial_outputs(self.partial_outputs_fixture(), self.cases)
        for mutation in ('missing_id', 'duplicate_id', 'environment', 'control_order', 'exit', 'observer', 'output_digest', 'missing_variant', 'variant_order', 'healthy', 'outage', 'provider_uncalled', 'provider_status', 'profile', 'native_creation', 'native_selection'):
            output = self.partial_outputs_fixture()
            config = output['partialEvidence'][3]['raw'][0]['nativeStartup']
            boundary = output['partialEvidence'][-1]['raw'][0]['boundaryVariants']
            if mutation == 'missing_id': output['partialEvidence'].pop()
            elif mutation == 'duplicate_id': output['partialEvidence'][-1]['id'] = 'REL-002'
            elif mutation == 'environment': config['specification'] = {}
            elif mutation == 'control_order': config['probes'].reverse()
            elif mutation == 'exit': config['probes'][1]['exitCode'] = '1'
            elif mutation == 'observer': config['probes'][1]['jsonEvents'] = []
            elif mutation == 'output_digest': config['probes'][1]['outputSha256'] = 'foreign'
            elif mutation == 'missing_variant': boundary.pop()
            elif mutation == 'variant_order': boundary.reverse()
            elif mutation == 'healthy': boundary[0]['healthyControl']['status'] = 500
            elif mutation == 'outage': boundary[0]['outage']['response']['status'] = 200
            elif mutation == 'provider_uncalled': boundary[1]['providerCalls'] = []
            elif mutation == 'provider_status': boundary[1]['providerCalls'][0]['responseStatus'] = 202
            elif mutation == 'profile': boundary[1]['profiles']['activation']['executionMs'] = 1
            elif mutation == 'native_creation': boundary[1]['nativeFixtureCreation']['status'] = 400
            elif mutation == 'native_selection': boundary[1]['nativeHostOutcome']['selected'] = 0
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.validate_partial_outputs(output, self.cases)

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
