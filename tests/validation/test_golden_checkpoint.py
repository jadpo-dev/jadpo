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
                     'tests/assurance/service-reference-mail-v0.1.json', harness.JOB001_SUCCESSOR,
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
        if case_id in ('CONFIG-001', 'CONFIG-002'): return self.preflight_raw(case_id)
        if case_id == 'CONFIG-003': return self.readiness_raw()
        if case_id == 'BOUNDARY-001': return self.boundary_raw()
        if case_id in ('REL-002', 'REL-004'): return self.relationship_raw(case_id)
        if case_id in ('JOB-001', 'JOB-002', 'JOB-003'): return self.reminder_raw(case_id)
        if case_id == 'JOB-004': return self.maintenance_raw()
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

    def readiness_raw(self):
        phases = []
        for index, (label, status) in enumerate((('healthy_control', 200), ('database_unavailable', 503), ('recovered_control', 200))):
            ready = {'status': 'ready' if status == 200 else 'not_ready', 'checks': {'database': 'available' if status == 200 else 'unavailable'}, 'advisories': {'reminder_mail': 'unprobed'}}
            responses = []
            for plane, path in [('deployment', '/health/ready'), ('deployment', '/health'), ('application', '/health/ready'), ('application', '/health'), ('application', '/health/live')]:
                code, body = (status, deepcopy(ready)) if plane == 'deployment' else (200, {'status': 'healthy'}) if path == '/health/live' else (404, {'error': {'code': 'not_found'}})
                responses.append({'plane': plane, 'path': path, 'origin': 'http://127.0.0.1:'+str(30002 if plane == 'deployment' else 30001), 'method': 'GET', 'status': code, 'body': body})
            phases.append({'label': label, 'at': f'2026-01-15T12:00:{index:02}Z', 'applicationPort': 30001, 'deploymentPort': 30002, 'responses': responses, 'direct': {'callable': 'handleDeploymentRequest', 'path': '/health/ready', 'status': status, 'body': deepcopy(ready)}, 'processExitCalls': [], 'statements': [{'operation': 'PRAGMA', 'sql': 'PRAGMA schema_version'}]})
        query = 'SELECT "id" AS "id", "title" AS "title", "status" AS "status", "due_at" AS "due_at", "created_at" AS "created_at", "updated_at" AS "updated_at" FROM "todo" WHERE "owner_id" = ?1 AND "deleted_at" IS NULL AND (?2 IS NULL OR "status" = ?2) AND (?3 IS NULL OR "due_at" <= ?3) AND ?4 IS NULL AND ?5 IS NULL AND ("deleted_at" IS NULL) AND (("todo"."owner_id" = ?7)) ORDER BY "created_at" DESC, "id" DESC LIMIT ?6'
        request = {'method': 'GET', 'path': '/todos', 'requestedPath': '/todos', 'requestJson': None, 'auth': 'alice-session', 'status': 200, 'authenticationAttempts': 1, 'credentialsIssued': 0, 'principals': [{'kind': 'user', 'subject': 'alice', 'authenticationStrength': 'primary', 'values': {'user_id': checkpoint.ALIASES['alice']}}], 'authorityQueries': {'principal': [], 'credential': []}, 'faultConfiguration': {'sessionStorageUnavailable': False}, 'at': phases[0]['at'], 'statements': [{'operation': 'SELECT', 'sql': query, 'numericParameters': {'6': 26}, 'todoOwnerIdentities': {'1': checkpoint.ALIASES['alice'], '7': checkpoint.ALIASES['alice']}}]}
        phases[2]['statements'] = [{'operation': 'CREATE', 'sql': 'CREATE TABLE IF NOT EXISTS "todo" (id TEXT)'}, {'operation': 'SELECT', 'sql': 'SELECT sessions.id, sessions.data, sessions.revoked FROM "__jadpo_auth_sessions" AS sessions LIMIT 0'}]
        return [{'nativeReadiness': {'serviceMetadata': {'effects': [{'service': 'ReminderMail'}]}, 'specification': self.cases['CONFIG-003']['fixture'], 'selectedBackend': 'sqlite', 'deploymentCallable': 'createDeploymentServer', 'hostname': '127.0.0.1', 'applicationPort': 30001, 'deploymentPort': 30002, 'phases': phases, 'healthyControl': deepcopy(request), 'recovery': {**deepcopy(request), 'at': phases[2]['at']}, 'outage': {'startedAt': phases[0]['at'], 'restoredAt': phases[1]['at'], 'mode': 'actual separate-connection SQLite exclusive lock', 'lockAcquireStatements': [{'operation': 'BEGIN', 'sql': 'BEGIN EXCLUSIVE'}], 'lockReleaseStatements': [{'operation': 'ROLLBACK', 'sql': 'ROLLBACK'}], 'triggeringRequest': {**request, 'status': 500}}}}]

    def boundary_raw(self):
        case = self.cases['BOUNDARY-001']; readiness = self.readiness_raw()[0]['nativeReadiness']
        healthy = deepcopy(readiness['healthyControl']); healthy['operationalEvents'] = []
        request_id = '00000000-0000-4000-8000-000000000077'; intent_id = '00000000-0000-4000-8000-000000000078'
        def fault(http=False): return {'error': {'code': 'internal_fault', 'message': 'An internal error occurred.', 'request_id': ('req_' if http else '')+request_id}}
        def event(http=False): return {'schemaVersion': 1, 'kind': 'operational_log_event', 'eventName': 'operation.failed', 'classification': 'RUNTIME_UNHANDLED_FAULT' if http else 'RUNTIME_SERVICE_MISCONFIGURED', 'requestId': ('req_' if http else '')+request_id, 'traceId': None, 'semanticOperationId': 'route:GET:/todos' if http else 'job.overdue_reminders.delivery.reminder_v1', 'sourceRevision': 'synthetic_revision', 'attributes': {}}
        failed = deepcopy(readiness['outage']['triggeringRequest']); failed.update(body=fault(True), operationalEvents=[event(True)])
        database = {'variant_index': 0, 'fixture_variant': case['fixtureVariants'][0], 'healthyControl': healthy, 'outage': {'mode': 'actual separate-connection SQLite exclusive lock', 'response': failed, 'lockAcquireStatements': [{'sql': 'BEGIN EXCLUSIVE'}], 'lockReleaseStatements': [{'sql': 'ROLLBACK'}]}}
        payload = {'idempotency_key': intent_id, 'to': 'alice@example.test', 'from': 'todos@example.test', 'todo_title': 'synthetic boundary', 'due_at': '2026-01-14T12:00:00Z'}
        row = {'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice'], 'reminder_sent_at': None}
        state = {'intents': [{'intent_id': intent_id, 'source_entity_id': row['id'], 'source_revision': '1', 'state': 'outcome_unknown', 'payload': payload}], 'claims': [{'intent_id': intent_id, 'invocations': 1, 'possible_dispatch': 1}], 'receipts': [], 'todo': row}
        provider = {'variant_index': 1, 'fixture_variant': case['fixtureVariants'][1], 'providerCalls': [{'method': 'POST', 'path': '/v1/messages', 'body': payload, 'idempotencyKey': intent_id, 'authorizationPresent': True, 'responseStatus': 401, 'responseBody': {'code': 'authentication'}, 'responseHeaders': {'x-private-provider-canary': 'CHECKPOINT_PRIVATE_PROVIDER_AUTH_HEADER_STACK'}}], 'profiles': {'activation': {'executionMs': 60000, 'leaseMs': 40000}, 'delivery': {'executionMs': 60000, 'leaseMs': 40000}}, 'nativeFixtureCreation': {'status': 201, 'method': 'POST', 'requestedPath': '/todos', 'body': {'id': row['id']}, 'at': '2026-01-13T12:00:00Z'}, 'nativeHostOutcome': {'status': 'finished', 'selected': 1, 'attempted': 1, 'continued': False, 'failures': [fault()]}, 'nativeHostThrown': None, 'operationalEvents': [{'schemaVersion': 1, 'kind': 'operational_log_event', 'eventName': 'service.attempt', 'operationId': request_id, 'attempt': 1, 'elapsedMs': 1, 'outcome': 'misconfigured', 'retryScheduled': False, 'retryDelayMs': None}, event()], 'statements': [{'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'UPDATE', 'sql': checkpoint.native_boundary_delivery_sql('sqlite')[0], 'deliveryIntentIdentity': intent_id}, {'operation': 'COMMIT', 'sql': 'COMMIT'}, {'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'WITH', 'sql': checkpoint.native_boundary_delivery_sql('sqlite')[1], 'deliveryIntentIdentity': intent_id, 'deliveryOutcomeState': 'outcome_unknown'}, {'operation': 'COMMIT', 'sql': 'COMMIT'}], 'persisted': row, 'durableAfter': state, 'restart': {'at': '2026-01-15T12:15:00Z', 'outcome': {'status': 'finished', 'selected': 1, 'attempted': 1, 'continued': False}, 'statements': [{'operation': 'SELECT', 'sql': 'SELECT 1'}], 'durable': deepcopy(state)}}
        def activation(identity):
            return {'schemaVersion': 1, 'kind': 'operational_log_event', 'eventName': 'transaction.attempt', 'operationId': identity, 'operation': 'job.overdue_reminders.delivery.reminder_v1.activation_stage', 'attempt': 1, 'database': 'sqlite', 'phase': 'commit', 'outcome': 'committed', 'retryableCause': None, 'rollbackProven': False, 'commitAcknowledged': True, 'elapsedMs': 1, 'selectedDelayMs': None, 'retryScheduled': False, 'cancelled': False}
        provider['durableAfter']['schedules'] = [{'source_entity': 'Todo', 'source_entity_id': row['id'], 'schedule_revision': '1'}]
        provider['restart']['durable']['schedules'] = deepcopy(provider['durableAfter']['schedules'])
        provider['operationalEvents'].insert(0, activation(checkpoint.ALIASES['t1']))
        provider['restart']['operationalEvents'] = [activation(checkpoint.ALIASES['t2'])]
        return [{'boundaryVariants': [database, provider], 'selectedBackend': 'sqlite', 'sourceRevision': 'synthetic_revision', 'binding': 'job.overdue_reminders.delivery.reminder_v1', 'jobMetadata': self.reminder_raw('JOB-001')[0]['reminderFixture']['jobMetadata'], 'specification': {'fixtureVariants': case['fixtureVariants'], 'requestOrRun': case['requestOrRun']}}]

    def preflight_raw(self, case_id):
        case = self.cases[case_id]
        fields = [{'configuration': 'TodoConfiguration', 'name': name, 'binding': binding} for name, binding in [('mail_api_key', 'MAIL_API_KEY'), ('database_url', 'DATABASE_URL'), ('oidc_issuer', 'OIDC_ISSUER')]]
        diagnostics = [{'schemaVersion': 1, 'kind': 'configuration_diagnostic', 'code': 'config_required' if value == '<missing>' else 'config_invalid', 'declaration': 'TodoConfiguration.'+binding.lower()} for binding, value in reversed(list(case['environment'].items()))]
        observer = {'kind': 'checkpoint_startup_observation', 'exitCode': 1, 'listenerStarts': 0, 'listenerAttempts': 0, 'driverInitializations': 0}
        original = {'label': 'original_environment', 'originalEnvironment': case['environment'], 'exitCode': 1, 'jsonEvents': [observer, *diagnostics, {'schemaVersion': 1, 'kind': 'operational_log_event', 'eventName': 'operation.failed', 'classification': 'RUNTIME_STARTUP_FAILED', 'requestId': 'startup', 'traceId': None, 'semanticOperationId': 'runtime:start', 'sourceRevision': 'synthetic_revision', 'attributes': {}}], 'outputSha256': 'a'*64, 'outputBytes': 100, 'credentialTextDisclosed': False}
        healthy = {'label': 'healthy_control', 'originalEnvironment': {}, 'exitCode': 1, 'jsonEvents': [{**observer, 'driverInitializations': 1}], 'outputSha256': 'b'*64, 'outputBytes': 200, 'credentialTextDisclosed': False}
        return [{'nativeStartup': {'specification': case['environment'], 'selectedBackend': 'sqlite', 'sourceRevision': 'synthetic_revision', 'configurationMetadata': {'fields': fields}, 'probes': [healthy, original]}}]

    def relationship_raw(self, case_id):
        before = {'users': [{'id': checkpoint.ALIASES[name], 'status': 'disabled' if name == 'alice' and case_id == 'REL-004' else 'active'} for name in ('alice', 'bob')], 'todos': [{'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice']}] if case_id == 'REL-004' else []}
        return [{'persistenceOperation': self.cases[case_id]['operation'], 'accepted': False, 'publicBoundaryObserved': True, 'publicBoundary': 'generated persistence.normalizePersistenceFailure', 'publicFailure': {'status': 409, 'code': 'relationship_conflict', 'message': 'The operation conflicts with a relationship.'}, 'before': before, 'after': deepcopy(before), 'statements': [{'operation': 'INSERT' if case_id == 'REL-002' else 'DELETE', 'sql': 'INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES ($1, $2, $3, $4, $5, NULL, $6, $6, $7)' if case_id == 'REL-002' else 'DELETE FROM "user" WHERE id = $1', 'relationshipRowIdentity': checkpoint.ALIASES['t1' if case_id == 'REL-002' else 'alice'], **({'relationshipOwnerIdentity': '00000000-0000-4000-8000-000000000099'} if case_id == 'REL-002' else {})}]}]

    def reminder_raw(self, case_id):
        """Deterministic protocol-only doubles; never establish application passes."""
        case = self.cases[case_id]
        binding = 'job.overdue_reminders.delivery.reminder_v1'
        metadata = {'jobs': [{'job': 'overdue_reminders', 'delivery': {'binding': binding}}]}
        kinds = ['completed', 'deleted', 'undated', 'reminded', 'disabled-owner'] if case_id == 'JOB-003' else ['eligible']
        creations, prepared = [], []
        for index, kind in enumerate(kinds):
            row = {'id': f'00000000-0000-4000-8000-{100+index:012}', 'owner_id': checkpoint.ALIASES['bob' if kind == 'disabled-owner' else 'alice'], 'title': 'synthetic '+kind, 'due_at': None if kind == 'undated' else '2026-01-14T12:00:00Z', 'status': 'open', 'deleted_at': None, 'reminder_sent_at': None}
            schedule = {'source_entity': 'Todo', 'source_entity_id': row['id'], 'schedule_revision': '1'}
            creations.append({'kind': kind, 'row': deepcopy(row), 'schedule': schedule, 'validated': deepcopy(row), 'validator': 'validate_TodoView', 'creation': {'method': 'POST', 'path': '/todos', 'status': 201, 'at': '2026-01-13T12:00:00Z', 'body': deepcopy(row), 'statements': [{'operation': 'INSERT', 'sql': 'INSERT INTO "__jadpo_delivery_schedules_v1"'}]}})
            if kind == 'completed': row['status'] = 'completed'
            if kind == 'deleted': row['deleted_at'] = self.clock
            if kind == 'reminded': row['reminder_sent_at'] = '2026-01-14T12:00:00Z'
            prepared.append(row)
        mutations = []
        if case_id == 'JOB-003':
            for method, path, status, body in [('PATCH', '/todos/'+prepared[0]['id'], 200, {'status': 'completed'}), ('DELETE', '/todos/'+prepared[1]['id'], 204, None)]:
                mutations.append({'method': method, 'path': path, 'status': status, 'requestJson': body, 'at': self.clock, 'statements': [{'operation': 'UPDATE', 'sql': 'UPDATE "todo"'}]})
            mutations.append({'fixtureOnlyPreviouslyReminded': {'id': prepared[3]['id'], 'at': prepared[3]['reminder_sent_at'], 'statements': [{'operation': 'UPDATE', 'sql': 'UPDATE todo SET reminder_sent_at = $1 WHERE id = $2'}]}})
            mutations.append({'method': 'DELETE', 'path': '/users/'+checkpoint.ALIASES['bob'], 'status': 204, 'requestJson': None, 'at': self.clock, 'statements': [{'operation': 'UPDATE', 'sql': 'UPDATE "user"'}]})
        principal = {'kind': 'service', 'subject': 'reporting', 'authenticationStrength': 'primary', 'values': {'service_id': checkpoint.ALIASES['reporting']}}
        fixture = {'specification': {'fixture': case['fixture'], 'run': case['run']}, 'binding': binding, 'jobMetadata': metadata, 'fixtureRows': creations, 'preparedRows': prepared, 'mutations': mutations, 'service': {'id': checkpoint.ALIASES['reporting'], 'status': 'active'}, 'credential': {'service_id': checkpoint.ALIASES['reporting'], 'status': 'active', 'revoked_at': None, 'privateMetadataRevoked': False, 'expires_at': '2026-10-06T10:00:00Z'}, 'serviceIssuance': {'at': self.clock, 'actualAuthorityClock': '2026-10-06T09:00:00Z', 'expires': '2026-10-06T10:00:00Z', 'credentialsIssued': 1, 'authorityPrincipals': [principal], 'statements': [{'operation': 'INSERT', 'sql': 'INSERT INTO "service_credential"'}]}, 'ownerRows': [{'id': checkpoint.ALIASES[name], 'email': name+'@example.test', 'status': 'disabled' if name == 'bob' and case_id == 'JOB-003' else 'active'} for name in ('alice', 'bob')]}
        def run(label, at, selected, source=None):
            result = {'label': label, 'at': at, 'binding': binding, 'profiles': {'activation': {'executionMs': 60000, 'leaseMs': 40000}, 'delivery': {'executionMs': 60000, 'leaseMs': 40000}}, 'outcome': {'status': 'idle' if label == 'original_run' and case_id == 'JOB-002' else 'finished', 'selected': selected, 'attempted': selected, 'continued': False}, 'calls': [], 'intents': [], 'receipts': [], 'after': deepcopy(prepared), 'schedules': [deepcopy(item['schedule']) for item in creations], 'statements': [{'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'SELECT', 'sql': 'SELECT c FROM "service_credential"'}, {'operation': 'SELECT', 'sql': 'SELECT t."id" AS source_id FROM "todo" WHERE "deleted_at" IS NULL AND t."due_at" IS NOT NULL AND t."reminder_sent_at" IS NULL ORDER BY t."due_at" ASC, t."id" ASC LIMIT 500'}, {'operation': 'COMMIT', 'sql': 'COMMIT'}]}
            if selected:
                row = deepcopy(source or prepared[0]); row['reminder_sent_at'] = at
                identity = '00000000-0000-4000-8000-000000000999'
                body = {'to': 'alice@example.test', 'todo_title': row['title'], 'due_at': row['due_at'], 'idempotency_key': identity}
                result.update(calls=[{'method': 'POST', 'path': '/v1/messages', 'body': body, 'idempotencyKey': identity, 'authorizationCorrect': True, 'at': at, 'responseStatus': 202}], intents=[{'intent_id': identity, 'source_entity_id': row['id'], 'source_revision': '1', 'state': 'succeeded', 'payload': deepcopy(body)}], receipts=[{'intent_id': identity, 'accepted_at': at, 'observed_at': at}], after=[row], schedules=[{'source_entity': 'Todo', 'source_entity_id': row['id'], 'schedule_revision': '1'}], statements=[{'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'SELECT', 'sql': 'SELECT t."id" AS source_id FROM "todo" WHERE "deleted_at" IS NULL AND t."due_at" IS NOT NULL AND t."reminder_sent_at" IS NULL ORDER BY t."due_at" ASC, t."id" ASC LIMIT 500'}, {'operation': 'SELECT', 'sql': 'SELECT c FROM "service_credential" JOIN "reminder_service_membership" ON 1=1 WHERE "expires_at" > $1'}, {'operation': 'INSERT', 'sql': 'INSERT INTO "__jadpo_delivery_mail_receipts_v1"'}, {'operation': 'SELECT', 'sql': 'SELECT observed_at FROM "__jadpo_delivery_mail_receipts_v1"'}, {'operation': 'UPDATE', 'sql': 'UPDATE "todo" SET "reminder_sent_at" = $1, "updated_at" = $2 WHERE "id" = $3 AND "reminder_sent_at" IS NULL RETURNING *', 'reminderRowIdentity': row['id']}, {'operation': 'COMMIT', 'sql': 'COMMIT'}])
            return result
        raw = [{'reminderFixture': fixture}]
        if case_id == 'JOB-001': raw.append({'nativeReminderRun': run('original_run', case['run']['at'], 1)})
        elif case_id == 'JOB-002':
            completed = run('completed_JOB001_fixture', self.clock, 1)
            raw.append({'nativeReminderRun': completed})
            for label, at in [('original_run', case['run']['at']), ('independent_due_replay_control', '2026-01-15T12:15:00Z')]:
                replay = run(label, at, 0)
                for field in ('after', 'intents', 'receipts', 'schedules'): replay[field] = deepcopy(completed[field])
                raw.append({'nativeReminderRun': replay})
        else:
            raw.append({'nativeReminderRun': run('original_run', case['run']['at'], 0)})
            control = deepcopy(creations[0]); control['row'].update(id='00000000-0000-4000-8000-000000000200', title='synthetic positive control')
            control['schedule']['source_entity_id'] = control['row']['id']; control['creation']['body'] = deepcopy(control['row']); control['validated'] = deepcopy(control['row'])
            raw.append({'reminderEligibleControl': control})
            positive = run('eligible_positive_control', '2026-01-15T12:15:00Z', 1, control['row'])
            positive['eligibleControlAfter'] = positive['after'][0]; positive['eligibleControlSchedule'] = positive['schedules'][0]
            positive['after'] = deepcopy(prepared)
            raw.append({'nativeReminderRun': positive})
        return raw

    def maintenance_raw(self):
        case = self.cases['JOB-004']
        before = [{'id': checkpoint.ALIASES['t1'], 'owner_id': checkpoint.ALIASES['alice'], 'deleted_at': '2025-12-17T12:00:00Z'}, {'id': checkpoint.ALIASES['t2'], 'owner_id': checkpoint.ALIASES['alice'], 'deleted_at': '2025-12-15T12:00:00Z'}, {'id': '00000000-0000-4000-8000-000000000023', 'owner_id': checkpoint.ALIASES['alice'], 'deleted_at': None}]
        metadata = {'capabilities': [{'entity': 'Todo', 'audit_event': 'retention.purge.batch', 'batch_limit': 500, 'clause': 'Todo.lifecycle.purge', 'effect': 'retention_purge(Todo)', 'retention_binding': 'config.soft_delete_retention', 'source_callable': False}]}
        token = hashlib.sha256(('Todo:'+checkpoint.ALIASES['t2']).encode()).hexdigest()[:16]
        event = {'entity': 'Todo', 'sourceRevision': 'synthetic_revision', 'kind': 'maintenance_audit', 'eventName': 'retention.purge.batch', 'clause': 'Todo.lifecycle.purge', 'operation': 'retention_purge(Todo)', 'retentionBinding': 'config.soft_delete_retention', 'outcome': 'committed', 'selectedCount': 1, 'committedCount': 1, 'attemptNow': case['run']['at'], 'rowIdentityTokens': [token], 'bound': 500}
        statements = [{'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'SELECT', 'sql': 'SELECT "id", "deleted_at" FROM "todo" WHERE "deleted_at" IS NOT NULL AND "deleted_at" <= $1 ORDER BY "deleted_at" ASC, "id" ASC LIMIT 500', 'maintenanceCutoff': '2025-12-16T12:00:00Z'}, {'operation': 'DELETE', 'sql': 'DELETE FROM "todo" WHERE "id" = $2 AND ("deleted_at" IS NOT NULL AND "deleted_at" <= $1)', 'maintenanceRowIdentity': checkpoint.ALIASES['t2'], 'maintenanceCutoff': '2025-12-16T12:00:00Z'}, {'operation': 'COMMIT', 'sql': 'COMMIT'}]
        return [{'maintenanceFixture': {'before': before, 'metadata': metadata, 'sourceRevision': 'synthetic_revision', 'specification': {'fixture': case['fixture'], 'run': case['run']}}}, {'nativeMaintenance': {'callable': 'runLifecycleMaintenance', 'at': case['run']['at'], 'statements': statements, 'auditEvents': [event], 'after': deepcopy([before[0], before[2]])}}]

    def outputs(self):
        outputs = [{'backend': backend, 'results': [{'id': case_id, 'scenarios' if case_id == 'REL-001' else 'raw': self.raw(case_id)} for case_id in checkpoint.SUPPORTED]} for backend in harness.BACKENDS]
        for output in outputs:
            for result in output['results']:
                if result['id'] in ('CONFIG-001', 'CONFIG-002'): result['raw'][0]['nativeStartup']['selectedBackend'] = output['backend']
                if result['id'] == 'BOUNDARY-001':
                    cap = result['raw'][0]; cap['selectedBackend'] = output['backend']
                    cap['boundaryVariants'][1]['operationalEvents'][0]['database'] = output['backend']
                    cap['boundaryVariants'][1]['restart']['operationalEvents'][0]['database'] = output['backend']
                    for statement in cap['boundaryVariants'][1]['statements']:
                        if statement['operation'] in ('UPDATE', 'WITH'):
                            statement['sql'] = checkpoint.native_boundary_delivery_sql(output['backend'])[statement['operation'] == 'WITH']
                    if output['backend'] == 'postgres':
                        database = cap['boundaryVariants'][0]; database['outage'].update(mode='actual isolated PostgreSQL server stopped', stoppedCode=0)
                        for request in (database['healthyControl'], database['outage']['response']):
                            sql = request['statements'][0]['sql']
                            for index in range(1, 8): sql = sql.replace('?'+str(index), '$'+str(index)+('::TEXT' if index == 2 else '::TIMESTAMPTZ(3)' if index == 3 else '::text' if index in (4, 5) else ''))
                            request['statements'][0]['sql'] = sql
                if result['id'] == 'CONFIG-003':
                    capture = result['raw'][0]['nativeReadiness']; capture['selectedBackend'] = output['backend']
                    if output['backend'] == 'postgres':
                        capture['outage'].update(mode='actual isolated PostgreSQL stopped', stoppedCode=0, restartedCode=0)
                        for phase in capture['phases'][:2]: phase['statements'] = [{'operation': 'SELECT', 'sql': 'SELECT 1'}]
                        for request in (capture['healthyControl'], capture['recovery'], capture['outage']['triggeringRequest']):
                            sql = request['statements'][0]['sql']
                            for index in range(1, 8): sql = sql.replace('?'+str(index), '$'+str(index)+('::TEXT' if index == 2 else '::TIMESTAMPTZ(3)' if index == 3 else '::text' if index in (4, 5) else ''))
                            request['statements'][0]['sql'] = sql
        return outputs

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
            (failure_path.parent/'lifecycle-maintenance.json').write_text(json.dumps(self.maintenance_raw()[0]['maintenanceFixture']['metadata']))
            (failure_path.parent/'jobs.json').write_text(json.dumps(self.reminder_raw('JOB-001')[0]['reminderFixture']['jobMetadata']))
            (failure_path.parent/'configuration.json').write_text(json.dumps(self.preflight_raw('CONFIG-001')[0]['nativeStartup']['configurationMetadata']))
            (failure_path.parent/'services.json').write_text(json.dumps(self.readiness_raw()[0]['nativeReadiness']['serviceMetadata']))
            (evidence/'application/build/app.meta.json').write_text(json.dumps({'source_revision': 'synthetic_revision'}))
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
            output['partialEvidence'] = [{'id': case_id, 'raw': [{'accepted': False, 'fault': {'kind': 'constraint'}}]} for case_id in ('REL-003',)]
        report = harness.run_cases(self.root); checkpoint.apply_checkpoint(report, outputs, self.root)
        for entry in report['results']:
            if entry['id'] == 'REL-003':
                self.assertEqual(entry['status'], 'not_run'); self.assertNotIn('observations', entry)

    def test_relationship_acceptance_requires_native_public_normalizer_and_exact_original_sql_identity(self):
        for case_id in ('REL-002', 'REL-004'):
            raw = self.relationship_raw(case_id)
            self.assertFalse(harness.assess(self.cases[case_id], checkpoint.observations(self.cases[case_id], raw)))
            for mutation in ('operation', 'accepted', 'boundary', 'public_missing', 'sql', 'bind', 'mutated', 'fixture', 'broadened', 'placeholder', 'duplicate'):
                raw = self.relationship_raw(case_id); capture = raw[0]
                if mutation == 'operation': capture['persistenceOperation'] = {}
                elif mutation == 'accepted': capture['accepted'] = True
                elif mutation == 'boundary': capture['publicBoundary'] = 'harness mapping'
                elif mutation == 'public_missing': capture['publicFailure'] = None
                elif mutation == 'sql': capture['statements'] = []
                elif mutation == 'bind': capture['statements'][0]['relationshipRowIdentity'] = 'foreign'
                elif mutation == 'mutated': capture['after']['todos'].append({'id': 'unexpected'})
                elif mutation == 'broadened': capture['statements'][0]['sql'] += ' OR 1=1'
                elif mutation == 'placeholder': capture['statements'][0]['sql'] = capture['statements'][0]['sql'].replace('$1', '$2')
                elif mutation == 'duplicate': capture['statements'].append(deepcopy(capture['statements'][0]))
                elif case_id == 'REL-002':
                    capture['before']['users'].append({'id': '00000000-0000-4000-8000-000000000099'}); capture['after'] = deepcopy(capture['before'])
                else:
                    capture['before']['users'][0]['status'] = 'active'; capture['after'] = deepcopy(capture['before'])
                with self.subTest(case=case_id, mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases[case_id], raw)
            for mutation in ('backend_details', 'foreign_code', 'foreign_message', 'foreign_status'):
                raw = self.relationship_raw(case_id); public = raw[0]['publicFailure']
                if mutation == 'backend_details': public['constraint'] = 'private_constraint'
                elif mutation == 'foreign_code': public['code'] = '23503'
                elif mutation == 'foreign_message': public['message'] = 'FOREIGN KEY constraint failed'
                else: public['status'] = 500
                with self.subTest(case=case_id, mutation=mutation):
                    self.assertTrue(harness.assess(self.cases[case_id], checkpoint.observations(self.cases[case_id], raw)))

    def test_approved_job001_overlay_is_explicit_exact_and_preserves_other43_originals(self):
        frozen = harness.read_contract(self.root)
        accepted, descriptor = harness.accepted_job001_contract(self.root)
        original_ids = [case['id'] for case in frozen['cases']]
        self.assertEqual([case['id'] for case in accepted['cases']], original_ids)
        self.assertEqual(descriptor['sha256'], harness.JOB001_SUCCESSOR_SHA256)
        for original, successor in zip(frozen['cases'], accepted['cases']):
            if original['id'] != 'JOB-001': self.assertEqual(original, successor)
            else:
                self.assertEqual(original['expect']['idempotencyKey'], 'todo.id')
                self.assertEqual(successor['expect']['idempotencyKey'], 'persisted reminder_intent.id')
        report = harness.run_cases(self.root, accepted_job001=True)
        self.assertEqual(len(report['results']), 88)
        harness.validate_report(report, self.root)
        self.assertNotIn('acceptance_successor', harness.run_cases(self.root))
        for mutation in ('descriptor', 'original', 'foreign_original', 'file_drift', 'file_missing'):
            changed = deepcopy(report)
            path = self.root/harness.JOB001_SUCCESSOR; previous = path.read_bytes()
            if mutation == 'descriptor': changed['acceptance_successor']['sha256'] = 'foreign'
            elif mutation == 'original': next(entry for entry in changed['results'] if entry['id'] == 'JOB-001')['original_frozen_required_scenarios'] = []
            elif mutation == 'foreign_original': next(entry for entry in changed['results'] if entry['id'] != 'JOB-001')['original_frozen_required_scenarios'] = []
            elif mutation == 'file_drift': path.write_bytes(previous+b'\n')
            else: path.unlink()
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): harness.validate_report(changed, self.root)
            path.write_bytes(previous)
        self.assertEqual(harness.read_contract(self.root), frozen)

    def test_original_preflight_negative_cases_require_specific_native_diagnostics_before_driver_or_listen(self):
        for case_id in ('CONFIG-001', 'CONFIG-002'):
            self.assertFalse(harness.assess(self.cases[case_id], checkpoint.observations(self.cases[case_id], self.preflight_raw(case_id))))
            for mutation in ('environment', 'missing_original', 'control_order', 'driver', 'listen_attempt', 'listener', 'boolean_counter', 'diagnostic', 'duplicate', 'public_debug', 'declaration', 'binding', 'source_revision', 'output_digest'):
                raw = self.preflight_raw(case_id); startup = raw[0]['nativeStartup']; original = startup['probes'][1]; observer = original['jsonEvents'][0]
                diagnostic = next(event for event in original['jsonEvents'] if event.get('kind') == 'configuration_diagnostic')
                if mutation == 'environment': startup['specification'] = {}
                elif mutation == 'missing_original': startup['probes'].pop()
                elif mutation == 'control_order': startup['probes'].reverse()
                elif mutation == 'driver': observer['driverInitializations'] = 1
                elif mutation == 'listen_attempt': observer['listenerAttempts'] = 1
                elif mutation == 'listener': observer['listenerStarts'] = 1
                elif mutation == 'boolean_counter': observer['driverInitializations'] = False
                elif mutation == 'diagnostic': diagnostic['code'] = 'generic_startup_fault'
                elif mutation == 'duplicate': original['jsonEvents'].append(deepcopy(diagnostic))
                elif mutation == 'public_debug': diagnostic['debug'] = 'private-canary'
                elif mutation == 'declaration': diagnostic['declaration'] = 'Foreign.value'
                elif mutation == 'binding': startup['configurationMetadata']['fields'] = []
                elif mutation == 'source_revision': startup['sourceRevision'] = 'foreign'
                else: original['outputSha256'] = 'foreign'
                with self.subTest(case=case_id, mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases[case_id], raw)
            for mutation in ('exit_zero', 'disclosure'):
                raw = self.preflight_raw(case_id); original = raw[0]['nativeStartup']['probes'][1]
                if mutation == 'exit_zero': original['exitCode'] = original['jsonEvents'][0]['exitCode'] = 0
                else: original['credentialTextDisclosed'] = True
                self.assertTrue(harness.assess(self.cases[case_id], checkpoint.observations(self.cases[case_id], raw)))
        outputs = self.outputs(); next(item for item in outputs[0]['results'] if item['id'] == 'CONFIG-001')['raw'][0]['nativeStartup']['selectedBackend'] = 'postgres'
        with self.assertRaisesRegex(ValueError, 'adapter lineage'): checkpoint.apply_checkpoint(harness.run_cases(self.root), outputs, self.root)

    def test_preflight_retained_events_are_closed_and_cannot_hide_disclosure(self):
        for case_id in ('CONFIG-001', 'CONFIG-002'):
            for mutation in ('negative_bytes', 'empty_bytes', 'fault_secret', 'observer_secret', 'foreign_event', 'healthy_secret', 'fault_request', 'fault_operation', 'boolean_schema'):
                raw = self.preflight_raw(case_id); cap = raw[0]['nativeStartup']; probe = cap['probes'][1]
                fault = next(event for event in probe['jsonEvents'] if event['kind'] == 'operational_log_event')
                if mutation == 'negative_bytes': probe['outputBytes'] = -1
                elif mutation == 'empty_bytes': probe['outputBytes'] = 0
                elif mutation == 'fault_secret': fault['credential'] = 'private-canary'
                elif mutation == 'observer_secret': probe['jsonEvents'][0]['credential'] = 'private-canary'
                elif mutation == 'foreign_event': probe['jsonEvents'].append({'kind': 'foreign', 'credential': 'private-canary'})
                elif mutation == 'healthy_secret': cap['probes'][0]['jsonEvents'][0]['credential'] = 'private-canary'
                elif mutation == 'fault_request': fault['requestId'] = 'private-canary'
                elif mutation == 'fault_operation': fault['semanticOperationId'] = 'private-canary'
                else: fault['schemaVersion'] = True
                with self.subTest(case=case_id, mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases[case_id], raw)

    def test_readiness_checked_advisory_never_admits_arbitrary_public_values(self):
        for mutation in ('secret_literal', 'foreign_key', 'missing', 'non_object', 'foreign_service', 'missing_metadata'):
            raw = self.readiness_raw(); capture = raw[0]['nativeReadiness']; phase = capture['phases'][1]
            if mutation == 'foreign_service': capture['serviceMetadata']['effects'][0]['service'] = 'ForeignService'
            elif mutation == 'missing_metadata': capture.pop('serviceMetadata')
            else:
                advisories = {'reminder_mail': 'private-canary'} if mutation == 'secret_literal' else {'foreign': 'unprobed'} if mutation == 'foreign_key' else {} if mutation == 'missing' else ['unprobed']
                for response in phase['responses'][:2]: response['body']['advisories'] = deepcopy(advisories)
                phase['direct']['body']['advisories'] = deepcopy(advisories)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['CONFIG-003'], raw)

    def test_readiness_native_business_query_principal_and_causal_timeline_are_required(self):
        for mutation in ('healthy_query', 'recovery_query', 'foreign_probe', 'outage_queries', 'outage_route', 'outage_auth_fault', 'healthy_principal', 'recovery_principal', 'outage_principal', 'outage_time', 'healthy_time', 'recovery_time', 'owner_bind', 'broad_query', 'placeholder', 'duplicate_query', 'restoration_time', 'recovery_initialization'):
            raw = self.readiness_raw(); cap = raw[0]['nativeReadiness']; outage = cap['outage']['triggeringRequest']
            if mutation in ('healthy_query', 'recovery_query'): cap['healthyControl' if mutation == 'healthy_query' else 'recovery']['statements'][0]['sql'] = 'SELECT 1'
            elif mutation == 'foreign_probe': cap['phases'][1]['statements'][0].update(operation='SELECT', sql='SELECT 1 FROM foreign_fixture')
            elif mutation == 'outage_queries': outage['statements'] = []
            elif mutation == 'outage_route': outage.update(method='POST', path='/auth/exchange', principals=[], statements=[])
            elif mutation == 'outage_auth_fault': outage['faultConfiguration']['sessionStorageUnavailable'] = True
            elif mutation in ('healthy_principal', 'recovery_principal', 'outage_principal'):
                request = cap['healthyControl'] if mutation == 'healthy_principal' else cap['recovery'] if mutation == 'recovery_principal' else outage
                request['principals'][0]['subject'] = 'bob'
            elif mutation == 'outage_time': outage['at'] = '2026-01-15T12:01:00Z'
            elif mutation == 'healthy_time': cap['healthyControl']['at'] = cap['phases'][2]['at']
            elif mutation == 'recovery_time': cap['recovery']['at'] = cap['phases'][0]['at']
            elif mutation == 'owner_bind': outage['statements'][0]['todoOwnerIdentities']['7'] = checkpoint.ALIASES['bob']
            elif mutation == 'broad_query': outage['statements'][0]['sql'] += ' OR 1=1'
            elif mutation == 'placeholder': outage['statements'][0]['sql'] = outage['statements'][0]['sql'].replace('?7', '?8')
            elif mutation == 'duplicate_query': outage['statements'].append(deepcopy(outage['statements'][0]))
            elif mutation == 'restoration_time': cap['outage']['restoredAt'] = cap['phases'][0]['at']
            else: cap['phases'][2]['statements'] = [{'operation': 'SELECT', 'sql': 'SELECT 1'}]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['CONFIG-003'], raw)

    def test_readiness_case_requires_real_distinct_listeners_outage_recovery_and_liveness_controls(self):
        case = self.cases['CONFIG-003']
        self.assertFalse(harness.assess(case, checkpoint.observations(case, self.readiness_raw())))
        for mutation in ('fixture', 'listener_alias', 'hostname', 'callable', 'phase_order', 'phase_missing', 'port_drift', 'origin', 'path', 'direct', 'alias_body', 'private_ready_body', 'physical_probe', 'healthy_request', 'recovery_request', 'outage_request', 'lock_acquire', 'lock_release', 'app_ready', 'live_status', 'clock', 'missing_exit_observer'):
            raw = self.readiness_raw(); capture = raw[0]['nativeReadiness']; phase = capture['phases'][1]
            if mutation == 'fixture': capture['specification'] = []
            elif mutation == 'listener_alias': capture['deploymentPort'] = capture['applicationPort']
            elif mutation == 'hostname': capture['hostname'] = '0.0.0.0'
            elif mutation == 'callable': capture['deploymentCallable'] = 'harness readiness'
            elif mutation == 'phase_order': capture['phases'].reverse()
            elif mutation == 'phase_missing': capture['phases'].pop()
            elif mutation == 'port_drift': phase['deploymentPort'] = 30003
            elif mutation == 'origin': phase['responses'][0]['origin'] = 'http://127.0.0.1:30001'
            elif mutation == 'path': phase['responses'][0]['path'] = '/todos'
            elif mutation == 'direct': phase['direct']['status'] = 200
            elif mutation == 'alias_body': phase['responses'][1]['body']['checks'] = {}
            elif mutation == 'private_ready_body':
                for response in phase['responses'][:2]: response['body']['private'] = 'raw-driver-canary'
                phase['direct']['body']['private'] = 'raw-driver-canary'
            elif mutation == 'physical_probe': phase['statements'] = []
            elif mutation == 'healthy_request': capture['healthyControl']['status'] = 500
            elif mutation == 'recovery_request': capture['recovery']['statements'] = []
            elif mutation == 'outage_request': capture['outage']['triggeringRequest']['status'] = 200
            elif mutation == 'lock_acquire': capture['outage']['lockAcquireStatements'] = []
            elif mutation == 'lock_release': capture['outage']['lockReleaseStatements'] = []
            elif mutation == 'app_ready': phase['responses'][2]['status'] = 503
            elif mutation == 'live_status': phase['responses'][4]['status'] = 503
            elif mutation == 'clock': phase['at'] = capture['phases'][0]['at']
            else: phase.pop('processExitCalls')
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(case, raw)
        raw = self.readiness_raw(); raw[0]['nativeReadiness']['phases'][1]['processExitCalls'] = [1]
        self.assertTrue(harness.assess(case, checkpoint.observations(case, raw)))

    def test_native_reminder_observations_require_exact_identity_receipt_clock_authority_and_controls(self):
        accepted, _ = harness.accepted_job001_contract(self.root)
        cases = {case['id']: case for case in accepted['cases']}
        for case_id in ('JOB-001', 'JOB-002', 'JOB-003'):
            observations = checkpoint.observations(cases[case_id], self.reminder_raw(case_id))
            self.assertFalse(harness.assess(cases[case_id], observations))
            if case_id == 'JOB-001':
                self.assertTrue(harness.assess(self.cases[case_id], observations))
            for mutation in ('missing_run', 'reorder', 'credential_expiry', 'authority_principal', 'creation_identity', 'creation_validator', 'fixture_state', 'execution_profile', 'host_sql', 'selector_bound'):
                raw = self.reminder_raw(case_id); fixture = raw[0]['reminderFixture']
                runs = [item['nativeReminderRun'] for item in raw if 'nativeReminderRun' in item]
                if mutation == 'missing_run': raw.pop()
                elif mutation == 'reorder': raw.reverse()
                elif mutation == 'credential_expiry': fixture['credential']['expires_at'] = '2026-01-15T12:00:01Z'
                elif mutation == 'authority_principal': fixture['serviceIssuance']['authorityPrincipals'] = []
                elif mutation == 'creation_identity': fixture['fixtureRows'][0]['creation']['body']['id'] = 'foreign'
                elif mutation == 'creation_validator': fixture['fixtureRows'][0]['validator'] = 'guessed'
                elif mutation == 'fixture_state': fixture['preparedRows'][0]['status'] = 'foreign'
                elif mutation == 'execution_profile': runs[0]['profiles']['delivery']['executionMs'] = 1
                elif mutation == 'host_sql': runs[0]['statements'] = []
                else:
                    for statement in runs[0]['statements']: statement['sql'] = statement['sql'].replace('LIMIT 500', 'LIMIT 999')
                with self.subTest(case=case_id, mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(cases[case_id], raw)
        for mutation in ('key', 'payload', 'receipt_identity', 'sent_clock', 'source_revision', 'recipient', 'receipt_sql_order', 'receipt_sql_missing', 'provider_auth', 'provider_status'):
            raw = self.reminder_raw('JOB-001'); run = raw[1]['nativeReminderRun']
            if mutation == 'key': run['calls'][0]['idempotencyKey'] = checkpoint.ALIASES['t1']
            elif mutation == 'payload': run['intents'][0]['payload']['todo_title'] = 'foreign'
            elif mutation == 'receipt_identity': run['receipts'][0]['intent_id'] = 'foreign'
            elif mutation == 'sent_clock': run['after'][0]['reminder_sent_at'] = '2026-01-15T12:00:01Z'
            elif mutation == 'source_revision': run['schedules'][0]['schedule_revision'] = '2'
            elif mutation == 'recipient': run['calls'][0]['body']['to'] = 'bob@example.test'; run['intents'][0]['payload']['to'] = 'bob@example.test'
            elif mutation == 'receipt_sql_order': run['statements'][3], run['statements'][5] = run['statements'][5], run['statements'][3]
            elif mutation == 'receipt_sql_missing': run['statements'].pop(3)
            elif mutation == 'provider_auth': run['calls'][0]['authorizationCorrect'] = False
            else: run['calls'][0]['responseStatus'] = 401
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(cases['JOB-001'], raw)
        for mutation in ('replay_calls', 'replay_outcome', 'replay_receipt', 'replay_clock', 'excluded_rows', 'excluded_mutations', 'excluded_called', 'control_creation', 'control_identity', 'control_clock'):
            case_id = 'JOB-002' if mutation.startswith('replay') else 'JOB-003'
            raw = self.reminder_raw(case_id); runs = [item['nativeReminderRun'] for item in raw if 'nativeReminderRun' in item]
            if mutation == 'replay_calls': runs[1]['calls'] = deepcopy(runs[0]['calls'])
            elif mutation == 'replay_outcome': runs[1]['outcome']['selected'] = 1
            elif mutation == 'replay_receipt': runs[1]['receipts'][0]['observed_at'] = 'foreign'
            elif mutation == 'replay_clock': runs[2]['at'] = '2026-01-15T12:01:00Z'
            elif mutation == 'excluded_rows': runs[1]['after'] = []
            elif mutation == 'excluded_mutations': raw[0]['reminderFixture']['mutations'] = []
            elif mutation == 'excluded_called': runs[0]['calls'] = deepcopy(runs[1]['calls'])
            elif mutation == 'control_creation': next(item['reminderEligibleControl'] for item in raw if 'reminderEligibleControl' in item)['creation']['status'] = 500
            elif mutation == 'control_identity': runs[1]['eligibleControlAfter']['id'] = 'foreign'
            else: runs[1]['at'] = '2026-01-15T12:01:00Z'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(cases[case_id], raw)

    def test_native_maintenance_requires_original_retention_rows_audit_cutoff_and_predicate(self):
        raw = self.maintenance_raw()
        self.assertFalse(harness.assess(self.cases['JOB-004'], checkpoint.observations(self.cases['JOB-004'], raw, self.clock)))
        for mutation in ('missing', 'order', 'young_date', 'expired_date', 'active_date', 'retained_identity', 'retained_state', 'audit_count', 'audit_token', 'audit_clause', 'audit_clock', 'cutoff', 'predicate', 'limit', 'commit', 'authority', 'source_revision', 'begin_missing', 'begin_after_delete', 'begin_after_commit', 'bulk_delete', 'foreign_bind', 'entity_missing', 'entity_foreign'):
            raw = self.maintenance_raw(); fixture = raw[0]['maintenanceFixture']; run = raw[1]['nativeMaintenance']; audit = run['auditEvents'][0]
            if mutation == 'missing': raw.pop()
            elif mutation == 'order': raw.reverse()
            elif mutation == 'young_date': fixture['before'][0]['deleted_at'] = '2025-12-01T12:00:00Z'
            elif mutation == 'expired_date': fixture['before'][1]['deleted_at'] = None
            elif mutation == 'active_date': fixture['before'][2]['deleted_at'] = self.clock
            elif mutation == 'retained_identity': run['after'][0]['id'] = checkpoint.ALIASES['t2']
            elif mutation == 'retained_state': run['after'][0]['deleted_at'] = None
            elif mutation == 'audit_count': audit['committedCount'] = 0
            elif mutation == 'audit_token': audit['rowIdentityTokens'] = ['foreign']
            elif mutation == 'audit_clause': audit['clause'] = 'Foreign.lifecycle.purge'
            elif mutation == 'audit_clock': audit['attemptNow'] = '2030-01-01T00:00:00Z'
            elif mutation == 'cutoff': run['statements'][1]['maintenanceCutoff'] = self.clock
            elif mutation == 'predicate': run['statements'][2]['sql'] = 'DELETE FROM "todo"'
            elif mutation == 'limit': run['statements'][1]['sql'] = run['statements'][1]['sql'].replace('LIMIT 500', 'LIMIT 999')
            elif mutation == 'commit': run['statements'].pop()
            elif mutation == 'authority': fixture['metadata']['capabilities'][0]['source_callable'] = True
            elif mutation == 'source_revision': audit['sourceRevision'] = 'foreign'
            elif mutation == 'begin_missing': run['statements'].pop(0)
            elif mutation == 'begin_after_delete': run['statements'].insert(2, run['statements'].pop(0))
            elif mutation == 'begin_after_commit': run['statements'].append(run['statements'].pop(0))
            elif mutation == 'bulk_delete': run['statements'][2]['sql'] = run['statements'][2]['sql'].replace('"id" = $2 AND (', '(')
            elif mutation == 'foreign_bind': run['statements'][2]['maintenanceRowIdentity'] = checkpoint.ALIASES['t1']
            elif mutation == 'entity_missing': audit.pop('entity')
            elif mutation == 'entity_foreign': audit['entity'] = 'User'

            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['JOB-004'], raw, self.clock)

    def test_reminder_receipt_and_identity_scoped_marker_share_exact_successful_transaction(self):
        accepted, _ = harness.accepted_job001_contract(self.root)
        case = next(case for case in accepted['cases'] if case['id'] == 'JOB-001')
        for mutation in ('none', 'begin_missing', 'commit_missing', 'commit_rollback', 'split', 'bulk_marker', 'foreign_bind', 'missing_bind', 'sql_operation_disagrees'):
            raw = self.reminder_raw('JOB-001'); run = raw[1]['nativeReminderRun']
            # Earlier activation and later cleanup commits must never replace
            # the transaction owning the durable receipt and source marker.
            prefix = [{'operation': 'BEGIN', 'sql': 'BEGIN'}, {'operation': 'SELECT', 'sql': 'SELECT 1'}, {'operation': 'COMMIT', 'sql': 'COMMIT'}]
            suffix = deepcopy(prefix)
            statements = run['statements']
            if mutation == 'begin_missing': statements.pop(0)
            elif mutation == 'commit_missing': statements.pop()
            elif mutation == 'commit_rollback': statements[-1] = {'operation': 'ROLLBACK', 'sql': 'ROLLBACK'}
            elif mutation == 'split': statements[5:5] = [{'operation': 'COMMIT', 'sql': 'COMMIT'}, {'operation': 'BEGIN', 'sql': 'BEGIN'}]
            elif mutation == 'bulk_marker': statements[5]['sql'] = statements[5]['sql'].replace('WHERE "id" = $3 AND ', 'WHERE ')
            elif mutation == 'foreign_bind': statements[5]['reminderRowIdentity'] = checkpoint.ALIASES['bob']
            elif mutation == 'missing_bind': statements[5].pop('reminderRowIdentity')
            elif mutation == 'sql_operation_disagrees': statements[0]['sql'] = 'ROLLBACK'
            run['statements'] = prefix + statements + suffix
            if mutation == 'none':
                self.assertFalse(harness.assess(case, checkpoint.observations(case, raw)))
            else:
                with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(case, raw)

    def partial_outputs_fixture(self):
        return {'partialEvidence': [{'id': 'REL-003', 'adapterFailure': True, 'raw': []}]}

    def test_partial_inventory_preserves_exact_remaining_original_owner_case(self):
        checkpoint.validate_partial_outputs(self.partial_outputs_fixture(), self.cases)
        for mutation in ('missing', 'duplicate', 'foreign'):
            output = self.partial_outputs_fixture()
            if mutation == 'missing': output['partialEvidence'] = []
            elif mutation == 'duplicate': output['partialEvidence'].append(deepcopy(output['partialEvidence'][0]))
            else: output['partialEvidence'][0]['id'] = 'BOUNDARY-001'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.validate_partial_outputs(output, self.cases)

    def test_generated_relative_modules_must_exist_before_native_startup(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory); (target/'app.ts').write_text('import { initialize } from "./initialization-control.js";')
            with self.assertRaisesRegex(ValueError, 'Missing generated relative runtime module'): checkpoint.verify_generated_local_modules(target)
            (target/'initialization-control.ts').write_text('export const initialize = true;')
            checkpoint.verify_generated_local_modules(target)

    def test_native_boundary_requires_both_original_variants_exact_envelope_event_and_no_resend(self):
        case = self.cases['BOUNDARY-001']
        for mutation in ('missing_variant', 'variant_order', 'missing_id', 'extra_error', 'event_id', 'event_source', 'event_operation', 'event_secret', 'typed_body', 'provider_path', 'uncalled', 'native_host', 'sent_marker', 'receipt', 'state', 'intent_key', 'source_identity', 'claim_dispatch', 'resend', 'changed_after', 'foreign_http', 'foreign_sql', 'owner_bind', 'profile'):
            raw = self.boundary_raw(); cap = raw[0]; database, provider = cap['boundaryVariants']
            if mutation == 'missing_variant': cap['boundaryVariants'].pop()
            elif mutation == 'variant_order': cap['boundaryVariants'].reverse()
            elif mutation == 'missing_id': provider['nativeHostOutcome']['failures'][0]['error'].pop('request_id')
            elif mutation == 'extra_error': provider['nativeHostOutcome']['failures'][0]['error']['private'] = 'canary'
            elif mutation == 'event_id': provider['operationalEvents'][-1]['requestId'] = 'foreign'
            elif mutation == 'event_source': provider['operationalEvents'][-1]['sourceRevision'] = 'foreign'
            elif mutation == 'event_operation': provider['operationalEvents'][-1]['semanticOperationId'] = 'foreign'
            elif mutation == 'event_secret': provider['operationalEvents'][0]['credential'] = 'canary'
            elif mutation == 'typed_body': provider['providerCalls'][0]['responseBody']['private'] = 'canary'
            elif mutation == 'provider_path': provider['providerCalls'][0]['path'] = '/foreign'
            elif mutation == 'uncalled': provider['providerCalls'] = []
            elif mutation == 'native_host': provider['nativeHostOutcome']['attempted'] = 0
            elif mutation == 'sent_marker': provider['persisted']['reminder_sent_at'] = '2026-01-15T12:00:00Z'
            elif mutation == 'receipt': provider['durableAfter']['receipts'].append({'intent_id': 'foreign'})
            elif mutation == 'state': provider['durableAfter']['intents'][0]['state'] = 'completed'
            elif mutation == 'intent_key': provider['providerCalls'][0]['idempotencyKey'] = 'foreign'
            elif mutation == 'source_identity': provider['durableAfter']['intents'][0]['source_entity_id'] = checkpoint.ALIASES['bob']
            elif mutation == 'claim_dispatch': provider['durableAfter']['claims'][0]['possible_dispatch'] = 2
            elif mutation == 'resend': provider['providerCalls'].append(deepcopy(provider['providerCalls'][0]))
            elif mutation == 'changed_after': provider['restart']['durable']['intents'][0]['state'] = 'completed'
            elif mutation == 'foreign_http': database['outage']['response']['path'] = '/auth/exchange'
            elif mutation == 'foreign_sql': database['outage']['response']['statements'][0]['sql'] = 'SELECT 1'
            elif mutation == 'owner_bind': database['outage']['response']['statements'][0]['todoOwnerIdentities']['7'] = checkpoint.ALIASES['bob']
            else: provider['profiles']['delivery']['executionMs'] = 1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(case, raw)

    def test_boundary_native_dispatch_and_unknown_completion_require_owning_commits_and_identity(self):
        for mutation in ('no_begin', 'late_begin', 'no_commit', 'rollback', 'broad_predicate', 'identity', 'state', 'duplicate', 'collapsed', 'resend_sql'):
            raw = self.boundary_raw(); provider = raw[0]['boundaryVariants'][1]; rows = provider['statements']
            if mutation == 'no_begin': rows.pop(3)
            elif mutation == 'late_begin': rows[3], rows[4] = rows[4], rows[3]
            elif mutation == 'no_commit': rows.pop(5)
            elif mutation == 'rollback': rows[5] = {'operation': 'ROLLBACK', 'sql': 'ROLLBACK'}
            elif mutation == 'broad_predicate': rows[4]['sql'] = rows[4]['sql'].replace('WHERE "intent_id" = $2 AND', 'WHERE 1=1 AND')
            elif mutation == 'identity': rows[4]['deliveryIntentIdentity'] = checkpoint.ALIASES['bob']
            elif mutation == 'state': rows[4]['deliveryOutcomeState'] = 'succeeded'
            elif mutation == 'duplicate': rows.append(deepcopy(rows[4]))
            elif mutation == 'collapsed': rows[2:4] = []
            else: provider['restart']['statements'].append(deepcopy(rows[1]))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['BOUNDARY-001'], raw)

    def test_boundary_activation_events_are_closed_ordered_and_owned_by_separate_native_runs(self):
        for mutation in ('missing_first', 'missing_replay', 'extra_secret', 'foreign_backend', 'foreign_binding', 'uncommitted', 'same_id', 'replay_service'):
            raw = self.boundary_raw(); provider = raw[0]['boundaryVariants'][1]
            if mutation == 'missing_first': provider['operationalEvents'].pop(0)
            elif mutation == 'missing_replay': provider['restart']['operationalEvents'] = []
            elif mutation == 'extra_secret': provider['operationalEvents'][0]['credential'] = 'private'
            elif mutation == 'foreign_backend': provider['operationalEvents'][0]['database'] = 'postgres'
            elif mutation == 'foreign_binding': provider['operationalEvents'][0]['operation'] = 'foreign'
            elif mutation == 'uncommitted': provider['operationalEvents'][0]['commitAcknowledged'] = False
            elif mutation == 'same_id': provider['restart']['operationalEvents'][0]['operationId'] = provider['operationalEvents'][0]['operationId']
            else: provider['restart']['operationalEvents'].append(deepcopy(provider['operationalEvents'][1]))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['BOUNDARY-001'], raw)

    def test_boundary_refuses_unbalanced_journal_nonfinite_event_and_foreign_revision(self):
        for mutation in ('nested_begin', 'dangling_begin', 'unmatched_commit', 'nan_attempt', 'inf_activation', 'foreign_revision', 'missing_schedule'):
            raw = self.boundary_raw(); provider = raw[0]['boundaryVariants'][1]
            if mutation == 'nested_begin': provider['statements'].insert(4, {'operation': 'BEGIN', 'sql': 'BEGIN'})
            elif mutation == 'dangling_begin': provider['statements'].append({'operation': 'BEGIN', 'sql': 'BEGIN'})
            elif mutation == 'unmatched_commit': provider['statements'].insert(0, {'operation': 'COMMIT', 'sql': 'COMMIT'})
            elif mutation == 'nan_attempt': provider['operationalEvents'][1]['elapsedMs'] = float('nan')
            elif mutation == 'inf_activation': provider['operationalEvents'][0]['elapsedMs'] = float('inf')
            elif mutation == 'foreign_revision':
                for snapshot in (provider['durableAfter'], provider['restart']['durable']): snapshot['intents'][0]['source_revision'] = 'foreign'
            else:
                for snapshot in (provider['durableAfter'], provider['restart']['durable']): snapshot['schedules'] = []
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): checkpoint.observations(self.cases['BOUNDARY-001'], raw)

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
