#!/usr/bin/env python3
"""Supporting migrated HTTP checkpoint; does not activate the final golden gate."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
import json
import hashlib
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile
from urllib.parse import parse_qs, quote, urlsplit

import golden_cases as harness

ROOT = Path(__file__).resolve().parents[1]
SUPPORTED = ('AUTH-001', 'AUTH-005', 'AUTH-012', 'PUBLIC-001', 'CREATE-002',
             'CREATE-004', 'READ-002', 'READ-003', 'LIST-004', 'PATCH-001',
             'PATCH-002', 'PATCH-003', 'PATCH-004', 'CREATE-001', 'CREATE-003',
             'READ-001', 'PATCH-005', 'DELETE-001', 'LIST-001', 'LIST-002',
             'LIST-003', 'REL-001', 'DELETE-002', 'AUTH-002', 'AUTH-006',
             'AUTH-010', 'USER-001', 'AUTH-003', 'AUTH-004', 'AUTH-007', 'AUTH-008',
             'AUTH-009', 'AUTH-011', 'JOB-004', 'JOB-001', 'JOB-002', 'JOB-003', 'REL-002', 'REL-004')
ALIASES = {'alice': '00000000-0000-4000-8000-000000000001',
           'bob': '00000000-0000-4000-8000-000000000002',
           't1': '00000000-0000-4000-8000-000000000021',
           't2': '00000000-0000-4000-8000-000000000022',
           'reporting': '00000000-0000-4000-8000-000000000041'}


def request_path(path: str) -> str:
    for alias, value in ALIASES.items():
        path = path.replace('/todos/'+alias, '/todos/'+value)
        path = path.replace('/users/'+alias+'/todos', '/users/'+value+'/todos')
        if path == '/users/'+alias:
            path = '/users/'+value
    return path


def instant(value):
    # Portable SQL drivers represent the same Instant as epoch milliseconds or
    # Date strings. Normalize zero fractional seconds without inventing dates.
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError('Missing portable persistence Instant')
    return value.replace('.000Z', 'Z')


def hashes(directory: Path, exclude=()) -> dict:
    return {str(path.relative_to(directory)): harness.digest(path)
            for path in sorted(directory.rglob('*')) if path.is_file()
            and not any(part in exclude for part in path.relative_to(directory).parts)}


def safe_run(command: list[str], **kwargs):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)
    if result.returncode:
        # Never publish provider/credential text from process exceptions.
        raise RuntimeError(f'Checkpoint process failed ({Path(command[0]).name}, exit {result.returncode}); raw process output withheld')
    return result


def prepare_owner_partial(output: Path, project: Path, environment: dict):
    """Real refusal/review observations; does not satisfy REL-003 disposition."""
    compiler = str(ROOT/'jadpo/target/debug/jadpo')
    mutation = output/'owner-mutation-source'
    shutil.copytree(project, mutation, ignore=shutil.ignore_patterns('build'))
    path = mutation/'entities/todo.jadpo'
    authored = path.read_text()
    action = '''
    action checkpoint_reassign_owner(todo_id: Todo.id, owner_id: User.id)
        fails TodoNotFound, TodoMutationConflict -> Todo
    {
        return attempt update required Todo {
            where: id == todo_id
            set: { owner_id: Todo.owner_id(owner_id) }
            missing: TodoNotFound { todo_id: todo_id }
            conflict: TodoMutationConflict
        }
    }
'''
    path.write_text(authored.rstrip()[:-1]+action+'}\n')
    checked = subprocess.run([compiler, 'check', str(mutation), '--diagnostic-format=json'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)
    diagnostic = json.loads(checked.stdout)
    if checked.returncode == 0 or diagnostic.get('status') != 'failed':
        raise ValueError('Ordinary owner transfer was not refused')
    projection = output/'owner-policy-projection'
    shutil.copytree(project, projection, ignore=shutil.ignore_patterns('build'))
    owner = projection/'entities/todo.jadpo'
    before = '        role: TodoRole.owner\n        immutable: true'
    if owner.read_text().count(before) != 1:
        raise ValueError('Owner policy partial projection source changed')
    owner.write_text(owner.read_text().replace(before, before+'\n        policy { TodoRole.owner: [create, read] }'))
    approval = json.loads(safe_run([compiler, 'approval', str(projection), '--before', str(project)], env=environment).stdout)
    probe = {'scope': 'Partial owner-transfer refusal and separate owner-field policy review projection; exact transfer human-decision disposition is not emitted',
             'semanticDisposition': None, 'mutationSource': hashes(mutation), 'mutationCheck': diagnostic,
             'projectionSource': hashes(projection), 'projectionApproval': approval}
    (output/'owner-partial-probe.json').write_text(json.dumps(probe, indent=2)+'\n')
    return {name: hashes(output/name) for name in ('owner-mutation-source', 'owner-policy-projection')}


@contextmanager
def postgres(environment: dict):
    """Always provision a disposable database; never consume an ambient URL."""
    with tempfile.TemporaryDirectory(prefix='jadpo-checkpoint-postgres-') as temporary:
        directory = Path(temporary)
        safe_run(['initdb', '-D', str(directory/'data'), '--username=golden_checkpoint',
                  '--auth=trust', '--no-locale', '--encoding=UTF8'], env=environment)
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        started = False
        try:
            safe_run(['pg_ctl', '-D', str(directory/'data'), '-l', str(directory/'server.log'),
                      '-o', f'-h 127.0.0.1 -p {port} -k {directory}', '-w', 'start'], env=environment)
            started = True
            safe_run(['createdb', '-h', '127.0.0.1', '-p', str(port), '-U', 'golden_checkpoint',
                      'golden_checkpoint'], env=environment)
            yield {**environment, 'DATABASE_URL': f'postgres://golden_checkpoint@127.0.0.1:{port}/golden_checkpoint', 'GOLDEN_POSTGRES_DATA': str(directory/'data'), 'GOLDEN_POSTGRES_SOCKET': str(directory), 'GOLDEN_POSTGRES_PORT': str(port)}
        finally:
            if started:
                safe_run(['pg_ctl', '-D', str(directory/'data'), '-m', 'immediate', '-w', 'stop'], env=environment)


def public_shape(body):
    if isinstance(body, dict):
        return {key: '<request-id>' if key == 'request_id' else public_shape(value)
                for key, value in body.items()}
    if isinstance(body, list):
        return [public_shape(value) for value in body]
    return body


def validated_output_schema(raw: list, reply: dict, route: str):
    validations = [item['outputValidation'] for item in raw if 'outputValidation' in item]
    if len(validations) != 1:
        raise ValueError('Missing generated output validation')
    validation = validations[0]
    if validation.get('validator') != 'validate_' + str(validation.get('schema')):
        raise ValueError('Foreign generated output validator')
    if validation.get('route') != route:
        raise ValueError('Foreign generated output route')
    return validation.get('schema') if validation.get('accepted') is True and harness.same_json(validation.get('validatedBody'), reply['body']) else None


def collection_fixture(raw: list):
    fixtures = [item for item in raw if 'fixtureRows' in item]
    if len(fixtures) != 1 or not isinstance(fixtures[0].get('queryMetadata'), list):
        raise ValueError('Missing collection fixture/query metadata')
    rows = fixtures[0]['fixtureRows']
    ids = [row.get('id') for row in rows]
    if not rows or len(ids) != len(set(ids)) or any(not isinstance(row.get('created_at'), str) or row.get('owner_id') not in (ALIASES['alice'], ALIASES['bob']) for row in rows):
        raise ValueError('Incomplete/duplicate collection fixture rows')
    return rows, {row['id']: row for row in rows}, fixtures[0]['queryMetadata']


def validate_auth_observation(event: dict):
    if not isinstance(event.get('principals'), list) or type(event.get('credentialsIssued')) is not int or event['credentialsIssued'] < 0:
        raise ValueError('Missing native principal/credential issuance instrumentation')
    for principal in event['principals']:
        if set(principal) != {'kind', 'subject', 'authenticationStrength', 'values'} or principal['kind'] not in ('user', 'service') or not isinstance(principal['subject'], str) or principal['authenticationStrength'] not in ('primary', 'multi_factor'):
            raise ValueError('Malformed native principal observation')
        if not isinstance(principal['values'], dict) or set(principal['values']) != {principal['kind']+'_id'}:
            raise ValueError('Malformed native principal identity')
    if type(event.get('authenticationAttempts')) is not int or event['authenticationAttempts'] < 0 or not isinstance(event.get('statements'), list) or any(not isinstance(row.get('sql'), str) or not isinstance(row.get('operation'), str) for row in event['statements']):
        raise ValueError('Missing physical authentication statement instrumentation')


def authority_query_accounting(event: dict):
    actual = {'principal': [index for index, row in enumerate(event['statements']) if row['operation'] == 'SELECT' and 'FROM "user" WHERE "authentication_subject"' in row['sql']],
              'credential': [index for index, row in enumerate(event['statements']) if row['operation'] == 'SELECT' and 'FROM "__jadpo_auth_sessions"' in row['sql']]}
    if not harness.same_json(event.get('authorityQueries'), actual):
        raise ValueError('Missing/inconsistent actual authority query accounting')
    return actual


def authentication_fixture(case: dict, raw: list, reply: dict, clock: str):
    captures = [item for item in raw if 'authenticationFixture' in item]
    if len(captures) != 1 or raw.index(captures[0]) >= raw.index(reply):
        raise ValueError('Missing/reordered native authentication fixture')
    fixture = captures[0]['authenticationFixture']
    if not harness.same_json(fixture.get('specification'), {'fixture': case['fixture'], 'timeline': case.get('timeline')}):
        raise ValueError('Changed original authentication fixture/timeline specification')
    user = fixture.get('user', {})
    session = fixture.get('sessionRecord', {})
    timeline = case.get('timeline', {})
    strategy = 'api_bearer' if case['id'] in ('AUTH-010', 'AUTH-008') else 'browser_session'
    issuance = fixture.get('issuanceObservation', {})
    if issuance.get('operation') != 'compiler-owned authentication issuance' or issuance.get('strategy') != strategy or type(issuance.get('credentialsIssued')) is not int or issuance['credentialsIssued'] != 1 or not isinstance(issuance.get('statements'), list) or not any(row.get('operation') == 'INSERT' and 'INTO "__jadpo_auth_sessions"' in row.get('sql', '') for row in issuance['statements']):
        raise ValueError('Missing native issuance positive control/session persistence')
    if user.get('id') != ALIASES['alice'] or user.get('authentication_subject') != 'alice' or user.get('status') != 'active' or user.get('disabled_at') is not None:
        raise ValueError('Missing active authentication issuance fixture')
    if session.get('strategy') != strategy or session.get('subject') != 'alice' or session.get('userId') != user['id'] or session.get('revoked') is not False:
        raise ValueError('Invalid native credential session fixture')
    if instant(fixture.get('issuedAt')) != instant(timeline.get('issuedAt', clock)) or instant(fixture.get('credentialExpiresAt')) != instant(session.get('expires')):
        raise ValueError('Changed credential issuance clock/expiry')
    if timeline and instant(fixture['credentialExpiresAt']) != instant(timeline['credentialExpiresAt']):
        raise ValueError('Changed original credential timeline expiry')
    if fixture['issuedAt'] >= fixture['credentialExpiresAt']:
        raise ValueError('Invalid issued credential lifetime')
    return fixture


def jwt_fixture(case: dict, raw: list, reply: dict, clock: str):
    fixtures = [item for item in raw if 'jwtFixture' in item]
    if len(fixtures) != 1 or raw.index(fixtures[0]) >= raw.index(reply):
        raise ValueError('Missing/reordered real JWT fixture')
    fixture = fixtures[0]['jwtFixture']
    if not harness.same_json(fixture.get('specification'), {'fixture': case['fixture'], 'request': case['request'], 'clock': clock}):
        raise ValueError('Changed original JWT fixture/request/clock')
    issuer = urlsplit(fixture.get('issuer', ''))
    if issuer.scheme != 'https' or issuer.hostname != '127.0.0.1' or not issuer.port or issuer.path or issuer.query or issuer.username or not isinstance(fixture.get('certificateSha256'), str):
        raise ValueError('Missing isolated real HTTPS JWT issuer')
    credentials = fixture.get('credentials')
    if not isinstance(credentials, list) or len(credentials) != 1:
        raise ValueError('Missing real signed JWT credential fixture')
    token = credentials[0]; claims = token.get('claims', {}); header = token.get('header', {}); key = token.get('publicJwk', {})
    subject = 'bob' if case['id'] == 'AUTH-004' else 'alice'
    if token.get('name') != subject+'-jwt' or claims.get('iss') != fixture['issuer'] or claims.get('aud') != fixture.get('audience') or claims.get('sub') != subject or type(claims.get('iat')) is not int or type(claims.get('exp')) is not int or claims['exp'] <= claims['iat']:
        raise ValueError('Foreign or invalid JWT claim fixture')
    if header.get('alg') != 'RS256' or header.get('kid') != key.get('kid') or key.get('alg') != header['alg'] or key.get('kty') != 'RSA' or any(field in key for field in ('d', 'p', 'q', 'dp', 'dq', 'qi', 'oth', 'k')):
        raise ValueError('Missing public-only actual JWT signing key fixture')
    users = fixture.get('users', [])
    if not any(user.get('id') == ALIASES[subject] and user.get('authentication_subject') == subject and user.get('status') == 'active' for user in users):
        raise ValueError('Missing active JWT authority fixture')
    return fixture, token


def validate_real_jwt_verification(reply: dict, fixture: dict, token: dict):
    validate_auth_observation(reply)
    claims = token['claims']
    verified = [{'strategy': 'api_bearer', 'subject': claims['sub'], 'issuer': claims['iss'], 'audience': claims['aud'], 'issuedAt': claims['iat'], 'expires': claims['exp']}]
    if not harness.same_json(reply.get('jwtVerifications'), verified):
        raise ValueError('Missing actual native JOSE verification')
    expected_provider = [{'method': 'GET', 'path': '/.well-known/openid-configuration', 'authorizationPresent': False, 'status': 200, 'body': {'issuer': fixture['issuer'], 'jwks_uri': fixture['issuer']+'/jwks'}},
                         {'method': 'GET', 'path': '/jwks', 'authorizationPresent': False, 'status': 200, 'body': {'keys': [token['publicJwk']]}}]
    if not harness.same_json(reply.get('providerRequests'), expected_provider):
        raise ValueError('Missing real local discovery/JWKS request/response observations')
    if len(reply['principals']) != 1 or reply['principals'][0]['kind'] != 'user' or reply['principals'][0]['subject'] != claims['sub'] or reply['principals'][0]['values'].get('user_id') != ALIASES[claims['sub']]:
        raise ValueError('Missing real JWT native principal authority mapping')
    if len(authority_query_accounting(reply)['principal']) != 1:
        raise ValueError('Missing actual JWT principal authority SQL')


def validate_cache_fault_control(case: dict, raw: list, reply: dict, clock: str):
    controls = [item for item in raw if 'cacheFaultControl' in item]
    if len(controls) != 1 or not raw.index(next(item for item in raw if 'authenticationFixture' in item)) < raw.index(controls[0]) < raw.index(reply):
        raise ValueError('Missing/reordered real cache-unavailable control')
    control = controls[0]['cacheFaultControl']
    if control.get('method') != 'DELETE' or control.get('path') != request_path('/users/alice') or control.get('auth') != 'alice-bounded-bearer' or control.get('requestJson') is not None or instant(control.get('at')) != instant(clock):
        raise ValueError('Foreign cache-unavailable control request/clock')
    validate_auth_observation(control)
    if control.get('status') != 503 or (control.get('body') or {}).get('error', {}).get('code') != 'authentication_unavailable' or control['statements'] or control['principals'] or control['credentialsIssued'] != 0 or control['authenticationAttempts'] != 1 or control.get('storageReads') != [{'operation': 'session.get', 'outcome': 'faulted'}]:
        raise ValueError('Cache fault did not fail native authentication before authority/business SQL')
    if control.get('faultConfiguration') != {'sessionStorageUnavailable': True} or reply.get('faultConfiguration') != control['faultConfiguration'] or reply.get('storageReads') != []:
        raise ValueError('Missing active/unaccessed cache fault for bounded authentication')


def user_timeline_observations(case: dict, raw: list, clock: str):
    events = [item for item in raw if 'method' in item or 'operation' in item]
    followups = case['followUps']
    if len(events) != len(followups)+1:
        raise ValueError('Missing disable/refresh/expiry timeline step')
    authentication_fixture(case, raw, events[0], clock)
    steps = []
    requests = [(case['request'], case['timeline']['disabledAt']), *[(follow.get('request', follow), follow['at']) for follow in followups]]
    for event, (request, at) in zip(events, requests):
        if instant(event.get('at')) != instant(at):
            raise ValueError('Changed ordered authentication timeline clock')
        validate_auth_observation(event)
        accounting = authority_query_accounting(event)
        if event is not events[-1] and (len(accounting['principal']) != 1 or len(accounting['credential']) != 1):
            raise ValueError('Missing real fresh/refresh session and principal authority reads')
        if 'operation' in request:
            if event.get('operation') != request['operation'] or event.get('auth') != request['auth'] or 'method' in event:
                raise ValueError('Changed compiler-owned refresh timeline operation')
            fault = event.get('fault')
            metadata = event.get('failureMetadata')
            if event.get('completed') is not False or not isinstance(fault, dict) or not isinstance(metadata, dict) or fault.get('failure') != metadata.get('failure') or fault.get('nativeCode') != 'principal_inactive':
                raise ValueError('Missing native refresh fault/public failure mapping')
            steps.append({'failure': fault['failure'], 'code': metadata['code'], 'credentialsIssued': event['credentialsIssued']})
        else:
            if event.get('method') != request['method'] or event.get('path') != request_path(request['path']) or not harness.same_json(event.get('auth'), request.get('auth')) or not harness.same_json(event.get('requestJson'), request.get('json')):
                raise ValueError('Changed authentication timeline request')
            step = {'status': event['status'], 'writes': sum(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in event['statements'])}
            code = (event.get('body') or {}).get('error', {}).get('code')
            if code is not None: step['code'] = code
            steps.append(step)
        steps[-1].update(databaseQueries=len(event['statements']), principalAuthorityQueries=len(accounting['principal']), credentialAuthorityQueries=len(accounting['credential']), authenticationAttempts=event['authenticationAttempts'])
    primary_principals = events[0]['principals']
    if len(primary_principals) != 1 or primary_principals[0]['kind'] != 'user' or primary_principals[0]['subject'] != 'alice' or primary_principals[0]['values'].get('user_id') != ALIASES['alice'] or any(event['principals'] for event in events[1:]):
        raise ValueError('Missing native primary principal or unexpected follow-up principal')
    fixtures = [item for item in raw if 'disableFixture' in item]
    retained = [item for item in raw if 'disabledPersistence' in item]
    if len(fixtures) != 1 or len(retained) != 1 or not raw.index(fixtures[0]) < raw.index(events[0]) < raw.index(retained[0]) < raw.index(events[1]):
        raise ValueError('Missing/reordered disable fixture/request/persistence/follow-up')
    before, after = fixtures[0]['disableFixture'], retained[0]['disabledPersistence']
    if before['user']['id'] != ALIASES['alice'] or before['user']['status'] != 'active' or before['todo']['id'] != ALIASES['t1'] or before['todo']['owner_id'] != ALIASES['alice'] or before['todo']['deleted_at'] is not None or after['user']['id'] != before['user']['id'] or after['todo']['id'] != before['todo']['id']:
        raise ValueError('Foreign disable user/todo fixture')
    if not harness.same_json(before['todo'], after['todo']) or instant(after['user'].get('disabled_at')) != instant(case['timeline']['disabledAt']):
        steps[0]['checkpoint_failures'] = ['Disablement changed todo or recorded a foreign disable clock']
    fixture = next(item['authenticationFixture'] for item in raw if 'authenticationFixture' in item)
    if not harness.same_json(fixture['sessionRecord'], after.get('sessionRecord')):
        raise ValueError('Disablement did not retain otherwise valid session')
    steps[0].update(userStatus=after['user']['status'], todoRowExists=after['todo']['id'] == ALIASES['t1'])
    return [{'variant_index': 0, 'steps': steps}]


def todo_statements(reply: dict):
    return [row for row in reply['statements'] if row['operation'] == 'SELECT' and re.search(r'FROM\s+"todo"', row['sql'], re.I)]


def observed_order(statement: dict):
    match = re.search(r'ORDER BY\s+"created_at"\s+(ASC|DESC),\s*"id"\s+(ASC|DESC)\s+LIMIT', statement['sql'], re.I)
    return ['created_at '+match[1].lower(), 'id '+match[2].lower()] if match else []


def native_patch_fixture(case: dict, raw: list):
    creations = [item for item in raw if 'fixtureCreation' in item]
    enrollments = [item for item in raw if 'fixtureEnrollment' in item]
    fixtures = [item for item in raw if 'fixture' in item]
    replies = [item for item in raw if 'method' in item]
    retained = [item for item in raw if 'persistence' in item]
    if len(creations) != 1 or len(enrollments) != 1 or len(fixtures) != 1 or len(replies) != 1 or len(retained) != 1 or not raw.index(creations[0]) < raw.index(enrollments[0]) < raw.index(fixtures[0]) < raw.index(replies[0]) < raw.index(retained[0]):
        raise ValueError('Missing/reordered real native-created patch fixture')
    creation = creations[0]['fixtureCreation']; capture = enrollments[0]['fixtureEnrollment']
    body = creation.get('body', {}); identity = body.get('id')
    if creation.get('method') != 'POST' or creation.get('path') != '/todos' or creation.get('auth') != 'alice-session' or creation.get('status') != 201 or instant(creation.get('at')) != '2026-01-15T11:00:00Z' or not harness.same_json(creation.get('requestJson'), {'title': 'original fixture title', 'due_at': '2026-02-01T00:00:00Z'}) or not isinstance(identity, str) or not re.fullmatch('[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}', identity):
        raise ValueError('Changed actual native fixture creation request/clock/identity')
    validate_auth_observation(creation)
    if capture.get('logicalAlias') != 't1' or capture.get('validator') != 'validate_TodoView' or not harness.same_json(capture.get('validatedBody'), body) or not any(row.get('operation') == 'INSERT' and 'INTO "todo"' in row.get('sql', '') for row in creation['statements']) or not any(row.get('operation') == 'INSERT' and 'INTO "__jadpo_delivery_schedules_v1"' in row.get('sql', '') for row in creation['statements']) or not any(row.get('operation') == 'COMMIT' for row in creation['statements']):
        raise ValueError('Missing actual generated validated Todo creation/schedule transaction')
    expected_principal = {'kind': 'user', 'subject': 'alice', 'authenticationStrength': 'primary', 'values': {'user_id': ALIASES['alice']}}
    if creation.get('principals') != [expected_principal] or creation.get('authenticationAttempts') != 1 or creation.get('credentialsIssued') != 0:
        raise ValueError('Native fixture creation did not authenticate original Alice principal')
    statements = creation['statements']
    todo_insert = next(index for index, row in enumerate(statements) if row.get('operation') == 'INSERT' and 'INTO "todo"' in row.get('sql', ''))
    schedule_insert = next(index for index, row in enumerate(statements) if row.get('operation') == 'INSERT' and 'INTO "__jadpo_delivery_schedules_v1"' in row.get('sql', ''))
    begin = [index for index, row in enumerate(statements) if row.get('operation') == 'BEGIN']
    commit = [index for index, row in enumerate(statements) if row.get('operation') == 'COMMIT']
    if not begin or not commit or not begin[0] < todo_insert < schedule_insert < commit[-1]:
        raise ValueError('Native fixture creation/schedule observations escaped ordered source transaction')
    before, after = capture.get('before', {}), capture.get('after', {})
    if before.get('id') != identity or before.get('owner_id') != ALIASES['alice'] or before.get('status') != 'open' or before.get('deleted_at') is not None or before.get('reminder_sent_at') is not None or any(not harness.same_json(before.get(field), body.get(field)) for field in ('id', 'title', 'status', 'due_at', 'created_at', 'updated_at')):
        raise ValueError('Native-created authoritative row differs from validated output')
    expected = deepcopy(before)
    setup = capture.get('setupStatements')
    if case['id'] == 'PATCH-005':
        expected['reminder_sent_at'] = '2026-01-15T11:59:00.000Z'
        if not isinstance(setup, list) or len(setup) != 1 or setup[0].get('operation') != 'UPDATE' or not re.search(r'UPDATE todo SET reminder_sent_at =', setup[0].get('sql', '')):
            raise ValueError('Missing explicit fixture-only prior-reminder setup')
    elif setup != []:
        raise ValueError('Unexpected native patch fixture setup mutation')
    if not harness.same_json(after, expected) or not harness.same_json(fixtures[0]['fixture'], after):
        raise ValueError('Fixture setup changed native identity/state outside recorded prior reminder')
    issuance = capture.get('requestCredentialIssuance', {})
    if issuance.get('strategy') != 'browser_session' or issuance.get('credentialsIssued') != 1 or instant(issuance.get('at')) != '2026-01-15T12:00:00Z' or not isinstance(issuance.get('expires'), str) or instant(issuance['expires']) <= instant(issuance['at']) or not any(row.get('operation') == 'INSERT' and 'INTO "__jadpo_auth_sessions"' in row.get('sql', '') for row in issuance.get('statements', [])):
        raise ValueError('Missing real current-clock credential issuance after historical fixture creation')
    enrollment = {'source_entity': 'Todo', 'source_entity_id': identity, 'schedule_revision': '1'}
    if not harness.same_json(capture.get('enrollment'), enrollment) or not harness.same_json(capture.get('enrollmentAfterSetup'), enrollment):
        raise ValueError('Missing native schedule identity or setup altered checked revision')
    if not harness.same_json(retained[0].get('enrollmentAfterPrimary'), {**enrollment, 'schedule_revision': '2'}) or sum(row.get('operation') == 'UPDATE' and '"__jadpo_delivery_schedules_v1"' in row.get('sql', '') for row in replies[0]['statements']) != 1:
        raise ValueError('Due patch did not advance native checked schedule revision')
    return identity


def reminder_observations(case: dict, raw: list):
    fixtures = [item for item in raw if 'reminderFixture' in item]
    runs = [item['nativeReminderRun'] for item in raw if 'nativeReminderRun' in item]
    labels = {'JOB-001': ['original_run'], 'JOB-002': ['completed_JOB001_fixture', 'original_run', 'independent_due_replay_control'], 'JOB-003': ['original_run', 'eligible_positive_control']}[case['id']]
    if len(fixtures) != 1 or [run.get('label') for run in runs] != labels or raw.index(fixtures[0]) >= raw.index(next(item for item in raw if 'nativeReminderRun' in item)):
        raise ValueError('Missing/reordered original native reminder fixture/run/control')
    fixture = fixtures[0]['reminderFixture']
    if not harness.same_json(fixture.get('specification'), {'fixture': case['fixture'], 'run': case['run']}) or fixture.get('binding') != 'job.overdue_reminders.delivery.reminder_v1':
        raise ValueError('Changed original reminder fixture/run/binding')
    job_metadata = fixture.get('jobMetadata', {}).get('jobs', [])
    if len(job_metadata) != 1 or job_metadata[0].get('job') != 'overdue_reminders' or job_metadata[0].get('delivery', {}).get('binding') != fixture['binding']:
        raise ValueError('Missing emitted checked reminder binding metadata')
    issuance = fixture.get('serviceIssuance', {}); credential = fixture.get('credential', {}); service = fixture.get('service', {})
    principal = {'kind': 'service', 'subject': 'reporting', 'authenticationStrength': 'primary', 'values': {'service_id': ALIASES['reporting']}}
    if service.get('id') != ALIASES['reporting'] or service.get('status') != 'active' or credential.get('service_id') != service['id'] or credential.get('status') != 'active' or credential.get('revoked_at') is not None or credential.get('privateMetadataRevoked') is not False or issuance.get('credentialsIssued') != 1 or issuance.get('authorityPrincipals') != [principal] or not isinstance(issuance.get('actualAuthorityClock'), str) or not isinstance(credential.get('expires_at'), str) or instant(credential['expires_at']) <= instant(issuance['actualAuthorityClock']) or instant(issuance.get('expires')) != instant(credential['expires_at']) or instant(issuance.get('at')) != '2026-01-15T12:00:00Z' or not any(row.get('operation') == 'INSERT' and 'INTO "service_credential"' in row.get('sql', '') for row in issuance.get('statements', [])):
        raise ValueError('Missing genuinely issued native credential valid at actual DB authority clock')
    creations = fixture.get('fixtureRows', []); prepared = fixture.get('preparedRows', [])
    kinds = ['completed', 'deleted', 'undated', 'reminded', 'disabled-owner'] if case['id'] == 'JOB-003' else ['eligible']
    if [item.get('kind') for item in creations] != kinds or len(prepared) != len(kinds) or len({row.get('id') for row in prepared}) != len(kinds):
        raise ValueError('Missing/duplicate original reminder fixture states')
    for created, row in zip(creations, prepared):
        creation = created.get('creation', {}); original = created.get('row', {}); owner = ALIASES['bob' if created['kind'] == 'disabled-owner' else 'alice']
        if creation.get('status') != 201 or creation.get('method') != 'POST' or creation.get('path') != '/todos' or instant(creation.get('at')) != '2026-01-13T12:00:00Z' or created.get('validator') != 'validate_TodoView' or not harness.same_json(created.get('validated'), creation.get('body')) or original.get('id') != row.get('id') or original.get('id') != creation.get('body', {}).get('id') or original.get('owner_id') != owner or row.get('owner_id') != owner or created.get('schedule') != {'source_entity': 'Todo', 'source_entity_id': row['id'], 'schedule_revision': '1'} or not any(statement.get('operation') == 'INSERT' and 'INTO "__jadpo_delivery_schedules_v1"' in statement.get('sql', '') for statement in creation.get('statements', [])):
            raise ValueError('Missing actual native-created reminder row/owner/schedule/output')
        kind = created['kind']
        if kind != 'undated' and instant(row.get('due_at')) != '2026-01-14T12:00:00Z' or kind == 'undated' and row.get('due_at') is not None or row.get('status') != ('completed' if kind == 'completed' else 'open') or (row.get('deleted_at') is not None) != (kind == 'deleted') or (row.get('reminder_sent_at') is not None) != (kind == 'reminded'):
            raise ValueError('Original reminder exclusion/eligible row state absent')
    mutations = fixture.get('mutations', [])
    if case['id'] == 'JOB-003':
        if len(mutations) != 4:
            raise ValueError('Missing recorded original native exclusion fixture mutations')
        for mutation, method, path, status, body in ((mutations[0], 'PATCH', '/todos/'+prepared[0]['id'], 200, {'status': 'completed'}), (mutations[1], 'DELETE', '/todos/'+prepared[1]['id'], 204, None), (mutations[3], 'DELETE', '/users/'+ALIASES['bob'], 204, None)):
            if mutation.get('method') != method or mutation.get('path') != path or mutation.get('status') != status or mutation.get('requestJson') != body or instant(mutation.get('at')) != '2026-01-15T12:00:00Z' or not any(row.get('operation') == 'UPDATE' for row in mutation.get('statements', [])):
                raise ValueError('Missing actual native completion/deletion/owner-disable fixture operation')
        previously = mutations[2].get('fixtureOnlyPreviouslyReminded', {})
        if previously.get('id') != prepared[3]['id'] or instant(previously.get('at')) != instant(prepared[3]['reminder_sent_at']) or not any(row.get('operation') == 'UPDATE' and 'UPDATE todo SET reminder_sent_at' in row.get('sql', '') for row in previously.get('statements', [])):
            raise ValueError('Missing explicit fixture-only prior reminder mutation')
    elif mutations:
        raise ValueError('Unspecified reminder fixture mutation')
    owners = {row.get('id'): row for row in fixture.get('ownerRows', [])}
    if owners.get(ALIASES['alice'], {}).get('status') != 'active' or owners.get(ALIASES['bob'], {}).get('status') != ('disabled' if case['id'] == 'JOB-003' else 'active'):
        raise ValueError('Missing original active/disabled-owner authority states')
    original = next(run for run in runs if run['label'] == 'original_run')
    if instant(original.get('at')) != instant(case['run']['at']):
        raise ValueError('Changed exact original native reminder run clock')
    for run in runs:
        if run.get('binding') != fixture['binding'] or not harness.same_json(run.get('profiles'), {'activation': {'executionMs': 60000, 'leaseMs': 40000}, 'delivery': {'executionMs': 60000, 'leaseMs': 40000}}) or not isinstance(run.get('calls'), list) or not isinstance(run.get('statements'), list):
            raise ValueError('Missing actual native job host profile/call/SQL observations')
        statements = run['statements']
        if not any(row.get('operation') == 'SELECT' and 'FROM "service_credential"' in row.get('sql', '') for row in statements) or not any(row.get('operation') == 'BEGIN' for row in statements) or not any(row.get('operation') == 'COMMIT' for row in statements):
            raise ValueError('Missing actual native credential/transaction host execution')
        if run.get('outcome', {}).get('status') == 'finished' and not any(row.get('operation') == 'SELECT' and all(token in row.get('sql', '') for token in ('AS source_id', 'FROM "todo"', '"deleted_at" IS NULL', 't."due_at" IS NOT NULL', 't."reminder_sent_at" IS NULL', 'ORDER BY t."due_at" ASC, t."id" ASC LIMIT 500')) for row in statements):
            raise ValueError('Missing actual checked eligible reminder selector/predicate/order/bound')
    def completed(run):
        if run.get('outcome') != {'status': 'finished', 'selected': 1, 'attempted': 1, 'continued': False}:
            raise ValueError('Missing actual unique native host selection/attempt/completion outcome')
        calls, intents, receipts, after, schedules = (run.get(key, []) for key in ('calls', 'intents', 'receipts', 'after', 'schedules'))
        if len(calls) != 1 or len(intents) != 1 or len(receipts) != 1 or len(after) != 1 or len(schedules) != 1:
            raise ValueError('Missing unique actual provider/intent/receipt/source completion')
        call, intent, receipt, row, schedule = calls[0], intents[0], receipts[0], after[0], schedules[0]
        identity = intent.get('intent_id')
        if not isinstance(identity, str) or not re.fullmatch('[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}', identity) or identity == row.get('id') or intent.get('state') != 'succeeded' or intent.get('source_entity_id') != row.get('id') or intent.get('source_revision') != schedule.get('schedule_revision') or schedule.get('source_entity_id') != row.get('id') or schedule.get('source_entity') != 'Todo' or not harness.same_json(intent.get('payload'), call.get('body')) or receipt.get('intent_id') != identity or call.get('idempotencyKey') != identity or call.get('body', {}).get('idempotency_key') != identity or call.get('authorizationCorrect') is not True or call.get('method') != 'POST' or call.get('path') != '/v1/messages' or call.get('responseStatus') != 202 or instant(call.get('at')) != instant(run.get('at')):
            raise ValueError('Actual immutable intent/provider receipt/source-version key binding absent')
        if row.get('owner_id') != ALIASES['alice'] or call['body'].get('to') != owners[ALIASES['alice']].get('email') or call['body'].get('todo_title') != row.get('title') or instant(call['body'].get('due_at')) != instant(row.get('due_at')) or row.get('reminder_sent_at') is None or instant(row['reminder_sent_at']) != instant(receipt.get('observed_at')) or not any(statement.get('operation') == 'SELECT' and 'observed_at FROM "__jadpo_delivery_mail_receipts_v1"' in statement.get('sql', '') for statement in run['statements']) or not any(statement.get('operation') == 'UPDATE' and 'UPDATE "todo" SET "reminder_sent_at"' in statement.get('sql', '') for statement in run['statements']):
            raise ValueError('Actual recipient/source/receipt-observation sent-mark completion absent')
        statements = run['statements']
        inserts = [index for index, row in enumerate(statements) if row.get('operation') == 'INSERT' and 'INTO "__jadpo_delivery_mail_receipts_v1"' in row.get('sql', '')]
        reads = [index for index, row in enumerate(statements) if row.get('operation') == 'SELECT' and 'SELECT observed_at FROM "__jadpo_delivery_mail_receipts_v1"' in row.get('sql', '')]
        marks = [index for index, row in enumerate(statements) if row.get('operation') == 'UPDATE' and 'UPDATE "todo" SET "reminder_sent_at"' in row.get('sql', '')]
        if len(inserts) != 1 or len(reads) != 1 or len(marks) != 1 or not inserts[0] < reads[0] < marks[0] or not any(row.get('operation') == 'BEGIN' for row in statements[:inserts[0]]) or not any(row.get('operation') == 'COMMIT' for row in statements[marks[0]+1:]) or not any(row.get('operation') == 'SELECT' and 'JOIN "reminder_service_membership"' in row.get('sql', '') and '"expires_at" >' in row['sql'] for row in statements):
            raise ValueError('Missing causal native authority/receipt insertion/read/sent-mark/commit sequence')
        # The receipt and source marker must share their owning transaction;
        # earlier activation/later cleanup transactions cannot substitute.
        if any(not isinstance(statement.get('sql'), str) or statement.get('operation') != (re.match(r'^\s*([a-z]+)', statement['sql'], re.I)[1].upper() if re.match(r'^\s*([a-z]+)', statement['sql'], re.I) else 'UNKNOWN') for statement in statements):
            raise ValueError('Reminder statement operation differs from actual SQL')
        controls = [(index, statement['operation']) for index, statement in enumerate(statements) if statement['operation'] in ('BEGIN', 'COMMIT', 'ROLLBACK')]
        preceding = [(index, operation) for index, operation in controls if index < inserts[0]]
        following = [(index, operation) for index, operation in controls if index > marks[0]]
        if not preceding or not following or preceding[-1][1] != 'BEGIN' or following[0][1] != 'COMMIT':
            raise ValueError('Reminder completion lost its owning successful BEGIN/COMMIT bracket')
        begin, commit = preceding[-1][0], following[0][0]
        if any(begin < index < commit for index, _ in controls):
            raise ValueError('Reminder receipt/read/marker split transactions or rolled back')
        mark = statements[marks[0]]
        if not re.fullmatch(r'UPDATE "todo" SET "reminder_sent_at" = [?$]1, "updated_at" = [?$]2 WHERE "id" = [?$]3 AND "reminder_sent_at" IS NULL RETURNING \*', mark['sql']) or mark.get('reminderRowIdentity') != row['id']:
            raise ValueError('Reminder source marker lost exact identity predicate/bind')
        return {'mailCalls': len(calls), 'recipient': 'alice.email', 'idempotencyKey': 'persisted reminder_intent.id', 'reminderSentAtPresent': row['reminder_sent_at'] is not None, 'idempotencyKeyEqualsBodyAndHeader': call['body']['idempotency_key'] == call['idempotencyKey'] == identity, 'reminderSentAtSource': 'compiler receipt observation time', 'sentMarkMatchesCurrentScheduleRevision': intent['source_revision'] == schedule['schedule_revision']}
    if case['id'] == 'JOB-001':
        return [{'variant_index': 0, 'steps': [completed(original)]}]
    if case['id'] == 'JOB-002':
        completed(runs[0])
        if runs[1].get('outcome') != {'status': 'idle', 'selected': 0, 'attempted': 0, 'continued': False} or runs[2].get('outcome') != {'status': 'finished', 'selected': 0, 'attempted': 0, 'continued': False} or instant(runs[0].get('at')) != '2026-01-15T12:00:00Z' or instant(runs[2].get('at')) != '2026-01-15T12:15:00Z' or any(run['calls'] for run in runs[1:]) or any(not harness.same_json(run.get(field), runs[0].get(field)) for run in runs[1:] for field in ('after', 'intents', 'receipts', 'schedules')):
            raise ValueError('Replay changed durable completion identity/receipt/source or dispatched again')
        return [{'variant_index': 0, 'steps': [{'additionalMailCalls': len(original['calls'])}]}]
    control = [item for item in raw if 'reminderEligibleControl' in item]
    if original['calls'] or not harness.same_json(original.get('after'), prepared) or len(control) != 1 or not raw.index(next(item for item in raw if item.get('nativeReminderRun', {}).get('label') == 'original_run')) < raw.index(control[0]) < raw.index(next(item for item in raw if item.get('nativeReminderRun', {}).get('label') == 'eligible_positive_control')) or instant(runs[1].get('at')) != '2026-01-15T12:15:00Z' or len(runs[1]['calls']) != 1 or runs[1]['calls'][0].get('body', {}).get('todo_title') != control[0]['reminderEligibleControl'].get('row', {}).get('title') or len(runs[1].get('after', [])) != len(prepared) or any(not harness.same_json(row, prepared[index]) for index, row in enumerate(runs[1].get('after', []))):
        raise ValueError('Excluded rows changed/called provider or missing real eligible positive control')
    if original.get('outcome') != {'status': 'finished', 'selected': 0, 'attempted': 0, 'continued': False} or original.get('intents') != [] or original.get('receipts') != [] or runs[1].get('eligibleControlAfter', {}).get('id') != control[0]['reminderEligibleControl'].get('row', {}).get('id'):
        raise ValueError('Missing original zero-selection/no-effect or foreign eligible source control')
    eligible = control[0]['reminderEligibleControl']; creation = eligible.get('creation', {}); row = eligible.get('row', {})
    if eligible.get('validator') != 'validate_TodoView' or not harness.same_json(eligible.get('validated'), creation.get('body')) or creation.get('status') != 201 or creation.get('method') != 'POST' or creation.get('path') != '/todos' or instant(creation.get('at')) != '2026-01-13T12:00:00Z' or row.get('id') != creation.get('body', {}).get('id') or row.get('owner_id') != ALIASES['alice'] or row.get('status') != 'open' or row.get('deleted_at') is not None or row.get('reminder_sent_at') is not None or instant(row.get('due_at')) != '2026-01-14T12:00:00Z' or eligible.get('schedule') != {'source_entity': 'Todo', 'source_entity_id': row['id'], 'schedule_revision': '1'} or not any(statement.get('operation') == 'INSERT' and 'INTO "__jadpo_delivery_schedules_v1"' in statement.get('sql', '') for statement in creation.get('statements', [])):
        raise ValueError('Missing genuinely native-created eligible positive control')
    completed({**runs[1], 'after': [runs[1].get('eligibleControlAfter')], 'schedules': [runs[1].get('eligibleControlSchedule')]})
    return [{'variant_index': 0, 'steps': [{'mailCalls': len(original['calls'])}]}]


def maintenance_observations(case: dict, raw: list):
    fixtures = [item for item in raw if 'maintenanceFixture' in item]
    runs = [item for item in raw if 'nativeMaintenance' in item]
    if len(fixtures) != 1 or len(runs) != 1 or raw.index(fixtures[0]) >= raw.index(runs[0]):
        raise ValueError('Missing/reordered native maintenance fixture and execution')
    fixture, run = fixtures[0]['maintenanceFixture'], runs[0]['nativeMaintenance']
    if not harness.same_json(fixture.get('specification'), {'fixture': case['fixture'], 'run': case['run']}) or run.get('callable') != 'runLifecycleMaintenance' or instant(run.get('at')) != instant(case['run']['at']):
        raise ValueError('Changed original maintenance fixture/run/clock')
    before, after = fixture.get('before', []), run.get('after', [])
    by_id = {row['id']: row for row in before}
    if len(before) != 3 or set(by_id) != {ALIASES['t1'], ALIASES['t2'], '00000000-0000-4000-8000-000000000023'} or any(row.get('owner_id') != ALIASES['alice'] for row in before) or instant(by_id[ALIASES['t1']].get('deleted_at')) != '2025-12-17T12:00:00Z' or instant(by_id[ALIASES['t2']].get('deleted_at')) != '2025-12-15T12:00:00Z' or by_id['00000000-0000-4000-8000-000000000023'].get('deleted_at') is not None:
        raise ValueError('Missing exact29day/31day/active original retention fixture')
    if len(after) != 2 or {row.get('id') for row in after} != {ALIASES['t1'], '00000000-0000-4000-8000-000000000023'} or any(not harness.same_json(row, by_id[row['id']]) for row in after):
        raise ValueError('Native retention altered young/active rows or missed expired row')
    events = run.get('auditEvents', [])
    if len(events) != 1:
        raise ValueError('Missing native retention maintenance audit')
    event = events[0]
    capabilities = fixture.get('metadata', {}).get('capabilities', [])
    if len(capabilities) != 1:
        raise ValueError('Missing generated maintenance authority metadata')
    capability = capabilities[0]
    if event.get('sourceRevision') != fixture.get('sourceRevision') or not isinstance(fixture.get('sourceRevision'), str) or event.get('entity') != 'Todo' or capability.get('entity') != 'Todo' or event.get('kind') != 'maintenance_audit' or event.get('eventName') != capability.get('audit_event') or event.get('clause') != capability.get('clause') or event.get('operation') != capability.get('effect') or event.get('retentionBinding') != capability.get('retention_binding') or capability.get('source_callable') is not False or event.get('outcome') != 'committed' or event.get('selectedCount') != 1 or event.get('committedCount') != len(before) - len(after) or instant(event.get('attemptNow')) != instant(case['run']['at']):
        raise ValueError('Missing native committed maintenance authority/clause/count/clock binding')
    expected_token = hashlib.sha256(('Todo:'+ALIASES['t2']).encode()).hexdigest()[:16]
    if event.get('rowIdentityTokens') != [expected_token]:
        raise ValueError('Native maintenance audit identifies a foreign deleted row')
    statements = run.get('statements', [])
    selected = [row for row in statements if row.get('operation') == 'SELECT' and 'FROM "todo"' in row.get('sql', '')]
    deleted = [row for row in statements if row.get('operation') == 'DELETE' and 'FROM "todo"' in row.get('sql', '')]
    if len(selected) != 1 or len(deleted) != 1 or not re.search(r'ORDER BY "deleted_at" ASC, "id" ASC LIMIT (\d+)', selected[0]['sql']):
        raise ValueError('Missing actual bounded native retention selection/delete')
    begins = [index for index, row in enumerate(statements) if row.get('operation') == 'BEGIN']
    commits = [index for index, row in enumerate(statements) if row.get('operation') == 'COMMIT']
    selected_index, deleted_index = statements.index(selected[0]), statements.index(deleted[0])
    if len(begins) != 1 or len(commits) != 1 or not begins[0] < selected_index < deleted_index < commits[0] or any(row.get('operation') == 'ROLLBACK' for row in statements):
        raise ValueError('Native retention BEGIN/select/delete/COMMIT order is missing or changed')
    if not re.search(r'^DELETE FROM "todo" WHERE "id" = [?$]2 AND \(', deleted[0]['sql']) or deleted[0].get('maintenanceRowIdentity') != ALIASES['t2']:
        raise ValueError('Native retention DELETE lost its selected-row identity predicate/bind')
    limit = int(re.search(r'LIMIT (\d+)', selected[0]['sql'])[1])
    if limit != event.get('bound') or limit != capability.get('batch_limit') or any(instant(row.get('maintenanceCutoff')) != '2025-12-16T12:00:00Z' or '"deleted_at" IS NOT NULL' not in row['sql'] or '"deleted_at" <=' not in row['sql'] for row in selected+deleted) or not statements.index(selected[0]) < statements.index(deleted[0]) or not any(row.get('operation') == 'COMMIT' for row in statements[statements.index(deleted[0])+1:]):
        raise ValueError('Native retention predicate/cutoff/transaction/batch disagrees with audit')
    return [{'variant_index': 0, 'steps': [{'hardDeletedCount': len(before)-len(after), 'retainedCount': len(after), 'batchLimit': limit}]}]


def relationship_observations(case: dict, raw: list):
    if len(raw) != 1:
        raise ValueError('Missing exact native persistence relationship operation capture')
    capture = raw[0]
    if not harness.same_json(capture.get('persistenceOperation'), case['operation']) or capture.get('accepted') is not False or capture.get('publicBoundaryObserved') is not True or capture.get('publicBoundary') != 'generated persistence.normalizePersistenceFailure':
        raise ValueError('Missing exact refused persistence operation/native public normalizer')
    before, after = capture.get('before', {}), capture.get('after', {})
    if not isinstance(before.get('users'), list) or not isinstance(before.get('todos'), list) or not harness.same_json(before, after):
        raise ValueError('Rejected relationship operation mutated persistence or lost observations')
    statements = capture.get('statements', [])
    if case['id'] == 'REL-002':
        if any(row.get('id') in (ALIASES['t1'], '00000000-0000-4000-8000-000000000099') for row in before['users']+before['todos']) or not any(row.get('operation') == 'INSERT' and 'INSERT INTO todo (id, owner_id,' in row.get('sql', '') and row.get('relationshipRowIdentity') == ALIASES['t1'] and row.get('relationshipOwnerIdentity') == '00000000-0000-4000-8000-000000000099' for row in statements):
            raise ValueError('Missing genuinely absent owner/orphan native INSERT')
    else:
        owners = [row for row in before['users'] if row.get('id') == ALIASES['alice']]
        retained = [row for row in after['todos'] if row.get('id') == ALIASES['t1'] and row.get('owner_id') == ALIASES['alice']]
        if len(owners) != 1 or owners[0].get('status') != 'disabled' or len(retained) != 1 or not any(row.get('operation') == 'DELETE' and 'DELETE FROM "user" WHERE id = ' in row.get('sql', '') and row.get('relationshipRowIdentity') == ALIASES['alice'] for row in statements):
            raise ValueError('Missing exact disabled retained owner/restricted native DELETE')
    public = capture.get('publicFailure')
    if not isinstance(public, dict) or not isinstance(public.get('code'), str):
        raise ValueError('Missing actual emitted native public failure result')
    safe_surface = set(public) == {'status', 'code', 'message'} and public.get('status') == 409 and public.get('code') == 'relationship_conflict' and public.get('message') == 'The operation conflicts with a relationship.'
    step = {'accepted': capture['accepted'], 'normalisedFailure': public['code'], 'rawDatabaseDetailsPublic': not safe_surface}
    if case['id'] == 'REL-004': step['todoRowExists'] = bool(retained)
    return [{'variant_index': 0, 'steps': [step]}]


def observations(case: dict, raw: list, clock: str | None = None) -> list:
    """Derive only fields supported by retained real response/statement evidence."""
    if case['id'] == 'JOB-004': return maintenance_observations(case, raw)
    if case['id'] in ('REL-002', 'REL-004'): return relationship_observations(case, raw)
    if case['id'] in ('JOB-001', 'JOB-002', 'JOB-003'): return reminder_observations(case, raw)
    if case['id'] == 'USER-001' and case.get('followUps'):
        if clock is None: raise ValueError('Authentication timeline requires original controlled clock')
        return user_timeline_observations(case, raw, clock)
    if case['id'] == 'REL-001' and case.get('fixtureVariants'):
        variants = case['fixtureVariants']
        if len(raw) != len(variants):
            raise ValueError('Missing relationship fixture variants')
        result = []
        for index, (variant, scenario) in enumerate(zip(variants, raw)):
            if type(scenario.get('variant_index')) is not int or scenario['variant_index'] != index or scenario.get('fixture_variant') != variant:
                raise ValueError('Foreign/reordered relationship fixture variant')
            if scenario.get('adapterFailure'):
                raise ValueError('Relationship fixture adapter failure')
            single = deepcopy(case); single.pop('fixtureVariants'); single['fixture_variant'] = variant
            result.append({'variant_index': index, 'steps': observations(single, scenario['raw'], clock)[0]['steps']})
        return result
    if case['id'] not in SUPPORTED or case.get('fixtureVariants') or case.get('followUps'):
        raise ValueError('Unsupported case/scenario')
    native_identity = native_patch_fixture(case, raw) if case['id'] in ('PATCH-002', 'PATCH-005') else None
    expected_requests = case.get('requests', [case.get('request')])
    if case['id'] == 'DELETE-001':
        expected_requests = [case['request'], {**case['request'], 'method': 'GET'}]
    if case['id'] == 'LIST-002':
        expected_requests = [*case['requests'], case['requests'][0]]
    replies = [item for item in raw if 'method' in item]
    if len(replies) != len(expected_requests):
        raise ValueError('Missing request observation')
    for index, (request, reply) in enumerate(zip(expected_requests, replies)):
        path = request_path(request['path'])
        if native_identity is not None: path = path.replace(ALIASES['t1'], native_identity)
        if case['id'] == 'LIST-002':
            for token, previous in (('{first.next}', 0), ('{second.next}', 1)):
                if token in path:
                    path = path.replace(token, quote(json.dumps(replies[previous]['body']['next'], separators=(',', ':')), safe="~()*!.'-"))
            if reply.get('requestedPath') != request['path']:
                raise ValueError('Changed cursor request template')
        if request['method'] != reply['method'] or path != reply['path']:
            raise ValueError('Foreign request observation')
        if not harness.same_json(request.get('auth'), reply.get('auth')) or not harness.same_json(request.get('json'), reply.get('requestJson')):
            raise ValueError('Changed request credential selection/body')
        if clock is not None and instant(reply.get('at')) != instant(clock):
            raise ValueError('Changed or missing controlled request clock')
        if type(reply.get('authenticationAttempts')) is not int or reply['authenticationAttempts'] < 0:
            raise ValueError('Missing authentication instrumentation')
        if not isinstance(reply.get('statements'), list) or any(not isinstance(row.get('sql'), str) or not isinstance(row.get('operation'), str) for row in reply['statements']):
            raise ValueError('Missing physical statement instrumentation')
    if case['id'] == 'READ-002':
        step = {'sameStatus': replies[0]['status'] if replies[0]['status'] == replies[1]['status'] else None,
                'sameCode': replies[0]['body'].get('error', {}).get('code') if replies[0]['body'].get('error', {}).get('code') == replies[1]['body'].get('error', {}).get('code') else None,
                'samePublicShape': public_shape(replies[0]['body']) == public_shape(replies[1]['body'])}
    else:
        reply = replies[0]
        step = {'status': reply['status'], 'databaseQueries': len(reply['statements']),
                'authenticationAttempts': reply['authenticationAttempts'],
                'writes': sum(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in reply['statements'])}
        code = (reply['body'] or {}).get('error', {}).get('code')
        if code is not None:
            step['code'] = code
        if case['id'] in ('AUTH-012', 'PUBLIC-001'):
            step['body'] = reply['body']
        if case['id'] in ('AUTH-009', 'AUTH-011'):
            captures = [item for item in raw if 'serviceFixture' in item]
            if len(captures) != 1 or raw.index(captures[0]) >= raw.index(reply) or clock is None:
                raise ValueError('Missing/reordered native service credential fixture')
            fixture = captures[0]['serviceFixture']; issued = fixture.get('issuedService', {}); service = fixture.get('service', {}); credential = fixture.get('credential', {})
            if not harness.same_json(fixture.get('specification'), {'fixture': case['fixture'], 'request': case['request'], 'clock': clock}) or instant(fixture.get('at')) != instant(clock):
                raise ValueError('Changed original service fixture/request/clock')
            if issued.get('id') != ALIASES['reporting'] or issued.get('owner_id') != ALIASES['alice'] or issued.get('name') != 'reporting' or issued.get('status') != 'active' or service.get('id') != issued['id'] or service.get('name') != issued['name'] or credential.get('service_id') != service['id'] or credential.get('status') != 'active' or credential.get('revoked_at') is not None or credential.get('privateMetadataRevoked') is not False or not isinstance(credential.get('expires_at'), str) or instant(credential['expires_at']) <= instant(clock):
                raise ValueError('Invalid active-at-issuance otherwise-valid service credential fixture')
            expected_principal = {'kind': 'service', 'subject': service['name'], 'authenticationStrength': 'primary', 'values': {'service_id': service['id']}}
            issuance, control = fixture.get('issuance', {}), fixture.get('validityControl', {})
            if type(issuance.get('credentialsIssued')) is not int or issuance['credentialsIssued'] != 1 or issuance.get('authorityPrincipals') != [expected_principal] or not any(row.get('operation') == 'INSERT' and 'service_credential' in row.get('sql', '') for row in issuance.get('statements', [])):
                raise ValueError('Missing native service credential issuance positive control')
            if control.get('completed') is not True or control.get('authorityPrincipals') != [expected_principal] or instant(control.get('at')) != instant(clock) or not any(row.get('operation') == 'SELECT' and 'FROM "service_credential"' in row.get('sql', '') for row in control.get('statements', [])):
                raise ValueError('Missing actual cryptographically valid service-key control')
            service_queries = [row for row in reply['statements'] if row['operation'] == 'SELECT' and 'FROM "service" WHERE "name"' in row['sql']]
            credential_queries = [row for row in reply['statements'] if row['operation'] == 'SELECT' and 'FROM "service_credential"' in row['sql']]
            metadata_queries = [row for row in reply['statements'] if row['operation'] == 'SELECT' and '"__jadpo_auth_service_credentials"' in row['sql']]
            if len(service_queries) != 1 or len(credential_queries) != 1:
                raise ValueError('Missing actual declared service credential/principal authority reads')
            step.update(serviceAuthorityQueries=len(service_queries), credentialAuthorityQueries=len(credential_queries), credentialMetadataQueries=len(metadata_queries), credentialsIssued=reply.get('credentialsIssued'))
            if case['id'] == 'AUTH-009':
                if service['status'] != 'active' or reply.get('authorityPrincipals') != [expected_principal] or reply.get('credentialsIssued') != 1:
                    raise ValueError('Missing actual native service exchange/issuance principal')
                disclosure = reply.get('exchangeDisclosure', {})
                token = disclosure.get('returnedCredential', {})
                if type(disclosure.get('incomingSecretInResponse')) is not bool or disclosure.get('responseFields') != ['access_token', 'token_type', 'expires_at'] or not harness.same_json(token, (reply.get('body') or {}).get('access_token')) or set(token) != {'sha256', 'byteLength', 'representation'} or not isinstance(token.get('sha256'), str) or not re.fullmatch('[0-9a-f]{64}', token['sha256']) or type(token.get('byteLength')) is not int or token['byteLength'] <= 0 or token.get('representation') != 'jdx1':
                    raise ValueError('Missing safe observed exchanged-credential response/disclosure')
                step.update(principalKind=expected_principal['kind'], principalService=next((alias for alias in ('reporting',) if ALIASES[alias] == reply['authorityPrincipals'][0]['values'].get('service_id')), None), secretInOutput=disclosure['incomingSecretInResponse'])
                revocations = [item for item in raw if 'serviceRevocation' in item]; revoked = [item for item in raw if 'serviceRevokedControl' in item]
                if len(revocations) != 1 or len(revoked) != 1 or not raw.index(reply) < raw.index(revocations[0]) < raw.index(revoked[0]):
                    raise ValueError('Missing/reordered actual service revocation control')
                mutation = revocations[0]['serviceRevocation']; after = mutation.get('after', {})
                if not harness.same_json(mutation.get('before'), credential) or not harness.same_json({key: value for key, value in after.items() if key != 'revoked_at'}, {key: value for key, value in credential.items() if key != 'revoked_at'}) or instant(after.get('revoked_at')) != instant(clock) or not any(row.get('operation') == 'UPDATE' for row in mutation.get('statements', [])):
                    raise ValueError('Missing real declared credential revocation storage change')
                rejection = revoked[0]['serviceRevokedControl']
                if rejection.get('method') != 'POST' or rejection.get('path') != '/auth/exchange' or rejection.get('auth') != 'reporting-service-key' or rejection.get('requestJson') is not None or instant(rejection.get('at')) != instant(clock) or rejection.get('status') != 401 or (rejection.get('body') or {}).get('error', {}).get('code') != 'invalid_credentials' or rejection.get('authorityPrincipals') != [] or rejection.get('credentialsIssued') != 0 or not any(row.get('operation') == 'SELECT' and 'FROM "service_credential"' in row.get('sql', '') for row in rejection.get('statements', [])) or any(row.get('operation') == 'SELECT' and 'FROM "service" WHERE "name"' in row.get('sql', '') for row in rejection.get('statements', [])):
                    raise ValueError('Revoked service key did not fail actual credential verification')
            elif service.get('status') != 'disabled' or instant(service.get('disabled_at')) != instant(clock) or reply.get('authorityPrincipals') != [] or reply.get('credentialsIssued') != 0:
                raise ValueError('Missing disabled-service native authority refusal')
        if case['id'] in ('AUTH-003', 'AUTH-004', 'AUTH-007'):
            if clock is None: raise ValueError('JWT case requires original controlled clock')
            fixture, token = jwt_fixture(case, raw, reply, clock)
            if case['id'] == 'AUTH-004':
                controls = [item for item in raw if 'jwtValidityControl' in item]
                if len(controls) != 1 or not raw.index(next(item for item in raw if 'jwtFixture' in item)) < raw.index(controls[0]) < raw.index(reply):
                    raise ValueError('Missing/reordered real conflicting JWT validity control')
                control = controls[0]['jwtValidityControl']
                if control.get('method') != 'GET' or control.get('path') != '/todos' or control.get('auth') != 'bob-jwt' or control.get('status') != 200 or instant(control.get('at')) != instant(clock):
                    raise ValueError('Invalid conflicting JWT validity control')
                validate_real_jwt_verification(control, fixture, token)
                validate_auth_observation(reply)
                if reply.get('jwtVerifications') != [] or reply.get('providerRequests') != [] or reply.get('storageReads') != [] or reply['principals'] != []:
                    raise ValueError('Conflicting credentials reached validation/resolution')
            else:
                validate_real_jwt_verification(reply, fixture, token)
                if case['id'] == 'AUTH-003':
                    principal = reply['principals'][0]
                    step.update(principalKind=principal['kind'], principalUser=next((alias for alias in ('alice', 'bob') if ALIASES[alias] == principal['values'].get('user_id')), None), providerFieldsInOutput=validated_output_schema(raw, reply, 'GET /todos') != 'TodoPage')
                else:
                    persisted = [item for item in raw if 'provisioningPersistence' in item]
                    if len(persisted) != 1 or raw.index(persisted[0]) <= raw.index(reply) or token['claims'].get('email') != 'attacker@example.test' or not any(user.get('id') == ALIASES['alice'] and user.get('email') == 'alice@example.test' for user in fixture['users']):
                        raise ValueError('Missing original provisioning/provider-email fixture and persistence')
                    step['privateFieldsAbsent'] = ['email'] if 'email' not in reply['body'] else []
                    step['providerEmailUsed'] = not harness.same_json(fixture['users'], persisted[0]['provisioningPersistence']) or token['claims']['email'] in json.dumps(reply['body'])
                    if validated_output_schema(raw, reply, 'GET /users/{user_id}/todos') != 'UserWithTodos' or reply['body'].get('user_id') != ALIASES['alice']:
                        step['checkpoint_failures'] = ['Foreign self-service output validator/identity']
        if case['id'] in ('AUTH-002', 'AUTH-006', 'AUTH-010', 'AUTH-008'):
            if clock is None:
                raise ValueError('Authentication case requires original controlled clock')
            fixture = authentication_fixture(case, raw, reply, clock)
            validate_auth_observation(reply)
            accounting = authority_query_accounting(reply)
            if case['id'] in ('AUTH-002', 'AUTH-008'):
                if len(reply['principals']) != 1:
                    raise ValueError('Missing/unbounded native authenticated principal')
                principal = reply['principals'][0]
                step['principalKind'] = principal['kind']
                step['principalUser'] = next((alias for alias in ('alice', 'bob') if ALIASES[alias] == principal['values'].get('user_id')), None)
                if principal['subject'] != fixture['user']['authentication_subject']:
                    step['checkpoint_failures'] = ['Native principal has foreign provider-independent subject']
                schema = validated_output_schema(raw, reply, 'GET /todos') if case['id'] == 'AUTH-002' else None
                # The actual generated closed validator rejects undeclared
                # provider payload fields anywhere in the returned page.
                if case['id'] == 'AUTH-002': step['providerFieldsInOutput'] = schema != 'TodoPage'
                else:
                    validate_cache_fault_control(case, raw, reply, clock)
                    step.update(principalAuthorityQueries=len(accounting['principal']), credentialAuthorityQueries=len(accounting['credential']))
            if case['id'] == 'AUTH-006':
                if len(accounting['principal']) != 1 or len(accounting['credential']) != 1:
                    raise ValueError('Missing real disabled-user session/principal authority reads')
                disabled = [item for item in raw if 'disabledUser' in item]
                if len(disabled) != 1 or not raw.index(next(item for item in raw if 'authenticationFixture' in item)) < raw.index(disabled[0]) < raw.index(reply):
                    raise ValueError('Missing/reordered disabled authority fixture')
                item = disabled[0]
                if item['disabledUser'].get('id') != fixture['user']['id'] or item['disabledUser'].get('status') != 'disabled' or instant(item['disabledUser'].get('disabled_at')) != instant(case['timeline']['disabledAt']) or instant(item.get('at')) != instant(case['timeline']['disabledAt']) or not harness.same_json(item.get('sessionRecord'), fixture['sessionRecord']):
                    raise ValueError('Invalid disabled authority/unexpired native session fixture')
            if case['id'] == 'AUTH-010':
                step['principalAuthorityQueries'] = len(accounting['principal'])
                step['credentialAuthorityQueries'] = len(accounting['credential'])
                step['credentialAuthorityQueriesRecorded'] = isinstance(reply.get('authorityQueries'), dict) and 'credential' in reply['authorityQueries']
        if case['id'] in ('CREATE-001', 'READ-001'):
            route = 'POST /todos' if case['id'] == 'CREATE-001' else 'GET /todos/{todo_id}'
            step['outputSchema'] = validated_output_schema(raw, reply, route)
            if case['id'] == 'CREATE-001':
                persisted = [item['persistence'] for item in raw if 'persistence' in item]
                if len(persisted) != 1 or persisted[0].get('id') != reply['body'].get('id'):
                    raise ValueError('Missing created row owner observation')
                step['storedOwner'] = next((alias for alias in ('alice', 'bob') if ALIASES[alias] == persisted[0].get('owner_id')), None)
            else:
                fixtures = [item['fixture'] for item in raw if 'fixture' in item]
                if len(fixtures) != 1 or fixtures[0].get('id') != ALIASES['t1'] or fixtures[0].get('owner_id') != ALIASES['alice'] or fixtures[0].get('deleted_at') is not None or reply['body'].get('id') != fixtures[0]['id']:
                    raise ValueError('Missing or foreign owned read fixture/output identity')
                step['privateFieldsAbsent'] = [field for field in ('owner_id', 'deleted_at', 'reminder_sent_at') if field not in reply['body']]
        if case['id'] in ('CREATE-001', 'CREATE-003', 'PATCH-005', 'DELETE-001') and clock is None:
            raise ValueError('Temporal case requires original controlled clock')
        if case['id'] == 'CREATE-004':
            persisted = [item.get('persistence') for item in raw if 'persistence' in item]
            if len(persisted) != 1 or persisted[0]['id'] != reply['body'].get('id'):
                raise ValueError('Missing persisted observation')
            step['storedDueAt'] = persisted[0]['storedDueAt']
            step['outputDueAt'] = reply['body']['due_at']
        if case['id'].startswith('PATCH-'):
            before = [item['fixture'] for item in raw if 'fixture' in item]
            after = [item['persistence'] for item in raw if 'persistence' in item]
            todo_id = native_identity or ALIASES['t2' if case['id'] == 'PATCH-004' else 't1']
            if len(before) != 1 or len(after) != 1 or before[0].get('id') != todo_id or after[0].get('id') != todo_id:
                raise ValueError('Missing or foreign patch fixture/persistence observation')
            if before[0].get('owner_id') != ALIASES['bob' if case['id'] == 'PATCH-004' else 'alice']:
                raise ValueError('Changed patch fixture owner')
            if case['id'] in ('PATCH-001', 'PATCH-002', 'PATCH-005'):
                if instant(before[0].get('due_at')) != '2026-02-01T00:00:00Z':
                    raise ValueError('Missing originally due-dated patch fixture')
                step['storedDueAt'] = instant(after[0]['due_at'])
            if case['id'] == 'PATCH-005':
                if before[0].get('reminder_sent_at') is None:
                    raise ValueError('Missing previously reminded patch fixture')
                step['storedReminderSentAt'] = instant(after[0]['reminder_sent_at'])
            if case['id'] == 'PATCH-001':
                step['storedTitle'] = after[0]['title']
            if not raw.index(next(item for item in raw if 'fixture' in item)) < raw.index(reply) < raw.index(next(item for item in raw if 'persistence' in item)):
                raise ValueError('Reordered patch fixture/request/persistence observations')
            if case['id'] in ('PATCH-003', 'PATCH-004') and step['writes'] == 0 and not harness.same_json(before[0], after[0]):
                raise ValueError('Rejected patch storage changed despite zero traced writes')
        if case['id'] == 'DELETE-001':
            fixtures = [item['fixture'] for item in raw if 'fixture' in item]
            retained = [item['persistence'] for item in raw if 'persistence' in item]
            if len(fixtures) != 1 or fixtures[0].get('id') != ALIASES['t1'] or fixtures[0].get('owner_id') != ALIASES['alice'] or fixtures[0].get('deleted_at') is not None:
                raise ValueError('Missing originally active delete fixture')
            if len(retained) != 1 or retained[0].get('id') != ALIASES['t1']:
                raise ValueError('Missing retained soft-delete row')
            fixture_index = raw.index(next(item for item in raw if 'fixture' in item))
            persistence_index = raw.index(next(item for item in raw if 'persistence' in item))
            if not fixture_index < raw.index(replies[0]) < persistence_index < raw.index(replies[1]):
                raise ValueError('Reordered delete fixture/request/persistence/follow-up')
            step['rowExists'] = True  # Presence of this actual SELECT result.
            step['deletedAt'] = instant(retained[0]['deleted_at'])
            step['subsequentGetStatus'] = replies[1]['status']
        if case['id'] == 'DELETE-002':
            fixtures = [item['fixture'] for item in raw if 'fixture' in item]
            retained = [item['persistence'] for item in raw if 'persistence' in item]
            if len(fixtures) != 1 or len(retained) != 1 or fixtures[0].get('id') != ALIASES['t1'] or fixtures[0].get('owner_id') != ALIASES['alice'] or fixtures[0].get('deleted_at') is None:
                raise ValueError('Missing originally deleted todo fixture')
            if not raw.index(next(item for item in raw if 'fixture' in item)) < raw.index(reply) < raw.index(next(item for item in raw if 'persistence' in item)):
                raise ValueError('Reordered repeated-delete fixture/request/persistence observations')
            if not harness.same_json(fixtures[0], retained[0]):
                step['checkpoint_failures'] = ['Repeated delete changed retained storage']
        if case['id'] in ('LIST-001', 'LIST-002', 'LIST-003', 'REL-001'):
            rows, by_id, metadata = collection_fixture(raw)
            eligible = [row for row in rows if row['owner_id'] == ALIASES['alice'] and row['deleted_at'] is None]
            items = reply['body']['todos'] if case['id'] == 'REL-001' else reply['body']['items']
            if any(item.get('id') not in by_id for item in items):
                raise ValueError('Unobserved collection output row')
            owners = {by_id[item['id']]['owner_id'] for item in items}
            if case['id'] != 'REL-001':
                step['onlyOwner'] = next((alias for alias in ('alice', 'bob') if owners == {ALIASES[alias]}), None)
            statements = todo_statements(reply)
            if len(statements) != 1:
                raise ValueError('Missing single physical collection query')
            order = observed_order(statements[0])
            if case['id'] == 'LIST-001':
                if not any(row['owner_id'] == ALIASES['bob'] and row['deleted_at'] is None for row in rows) or not any(row['owner_id'] == ALIASES['alice'] and row['deleted_at'] is not None for row in rows) or len(eligible) < 3 or len({row['created_at'] for row in eligible}) != 1:
                    raise ValueError('Incomplete tied-owner/deleted list fixture')
                step.update(deletedAbsent=all(by_id[item['id']]['deleted_at'] is None for item in items), order=order, nextCursorPresent=reply['body'].get('next') is not None)
                if [item['id'] for item in items] != [row['id'] for row in sorted(eligible, key=lambda row: (row['created_at'], row['id']), reverse=True)[:len(items)]]:
                    step['checkpoint_failures'] = ['Returned list does not follow its observed keyset ordering']
            elif case['id'] == 'LIST-002':
                ids = [item['id'] for page in replies[:3] for item in page['body']['items']]
                step.update(uniqueItemCount=len(set(ids)), stableAcrossRepeat=harness.same_json(replies[0]['body'], replies[3]['body']))
                if len(ids) != len(set(ids)) or set(ids) != {row['id'] for row in eligible} or any(page['status'] != 200 for page in replies):
                    step['checkpoint_failures'] = ['Cursor continuation has duplicates/gaps or a failed page']
            elif case['id'] == 'LIST-003':
                query = parse_qs(urlsplit(case['request']['path']).query)
                before = query['due_before'][0]
                statuses = {item['status'] for item in items}
                step.update(onlyTodoStatus=next(iter(statuses)) if len(statuses) == 1 else None,
                            allDueBefore=before if items and all(item['due_at'] is not None and instant(item['due_at']) <= before for item in items) else None,
                            undatedAbsent=all(item['due_at'] is not None for item in items))
                matched = {row['id'] for row in eligible if row['status'] == query['status'][0] and row['due_at'] is not None and instant(row['due_at']) <= before}
                if {item['id'] for item in items} != matched:
                    step['checkpoint_failures'] = ['Composed list filters omit or add fixture rows']
            else:
                match = re.search(r'LIMIT\s+[$?](\d+)', statements[0]['sql'], re.I)
                parameter = statements[0].get('numericParameters', {}).get(match[1]) if match else None
                if type(parameter) is not int or not any(plan.get('fetch_limit') == 'page_size_plus_one' for plan in metadata):
                    raise ValueError('Missing observed bounded child query limit')
                step.update(outputSchema=validated_output_schema(raw, reply, 'GET /users/{user_id}/todos'),
                            childLimit=parameter-1, childOrder=order,
                            crossParentMixing=bool(owners - {ALIASES['alice']}) or reply['body']['user_id'] != ALIASES['alice'],
                            privateFieldsAbsent=['email'] if 'email' not in reply['body'] else [])
                required = {'alice zero todos': 0, 'alice one todo': 1, 'alice 100 todos': 100}.get(case.get('fixture_variant'))
                if required is None or len(eligible) != required or not any(row['owner_id'] == ALIASES['bob'] for row in rows) or not any(row['owner_id'] == ALIASES['alice'] and row['deleted_at'] is not None for row in rows):
                    raise ValueError('Incomplete relationship fixture variant')
                expected_ids = [row['id'] for row in sorted(eligible, key=lambda row: (row['created_at'], row['id']))[:parameter-1]]
                if [item['id'] for item in items] != expected_ids:
                    step['checkpoint_failures'] = ['Child collection has missing/extra/misordered fixture rows']
    return [{'variant_index': 0, 'steps': [step]}]


def apply_checkpoint(report: dict, outputs: list[dict], root: Path):
    contract = harness.report_contract(report, root)
    cases = {case['id']: case for case in contract['cases']}
    observed = {}
    if len(outputs) != 2 or {output['backend'] for output in outputs} != set(harness.BACKENDS):
        raise ValueError('Missing/duplicate backend')
    for output in outputs:
        ids = [item['id'] for item in output['results']]
        if len(ids) != len(set(ids)) or set(ids) != set(SUPPORTED):
            raise ValueError('Missing/duplicate/foreign supported result')
        for item in output['results']:
            observed[(output['backend'], item['id'])] = None if item.get('adapterFailure') is True else observations(cases[item['id']], item['scenarios'] if item['id'] == 'REL-001' else item['raw'], contract.get('clock'))
    for entry in report['results']:
        key = (entry['backend'], entry['id'])
        if key not in observed:
            entry['reason'] = 'Unsupported by the migrated HTTP checkpoint; full application/gate prerequisites remain'
            continue
        entry.pop('reason', None)
        if observed[key] is None:
            entry['status'] = 'failed'
            entry.pop('observations', None)
            entry['failures'] = ['Checkpoint adapter failure; no raw exception text retained']
            continue
        entry['observations'] = observed[key]
        entry['failures'] = harness.assess(cases[entry['id']], observed[key])
        entry['failures'].extend(failure for scenario in observed[key] for step in scenario['steps'] for failure in step.get('checkpoint_failures', []))
        entry['status'] = 'failed' if entry['failures'] else 'passed'
    report['status'] = 'failed' if any(entry['status'] == 'failed' for entry in report['results']) else 'not_run'
    harness.validate_report(report, root)


def validate_partial_outputs(output: dict, cases: dict):
    """Integrity of retained causal probes, never application acceptance passes."""
    partial = output.get('partialEvidence')
    ids = ('REL-003', 'CONFIG-001', 'CONFIG-002', 'BOUNDARY-001')
    if not isinstance(partial, list) or len(partial) != len(ids) or {item.get('id') for item in partial} != set(ids):
        raise ValueError('Missing/duplicate original partial probe inventory')
    for item in partial:
        if item.get('adapterFailure') is True: continue
        case = cases[item['id']]
        if item['id'].startswith('CONFIG-'):
            startup = [event['nativeStartup'] for event in item['raw'] if 'nativeStartup' in event]
            if len(startup) != 1 or not harness.same_json(startup[0].get('specification'), case['environment']):
                raise ValueError('Changed original configuration probe environment')
            probes = startup[0].get('probes', [])
            if len(probes) != 2 or [probe.get('label') for probe in probes] != ['healthy_control', 'original_environment']:
                raise ValueError('Missing/reordered actual startup control')
            for probe, overrides in zip(probes, ({}, case['environment'])):
                if not harness.same_json(probe.get('originalEnvironment'), overrides) or type(probe.get('exitCode')) is not int or type(probe.get('credentialTextDisclosed')) is not bool or not isinstance(probe.get('jsonEvents'), list) or not re.fullmatch('[a-f0-9]{64}', str(probe.get('outputSha256'))) or type(probe.get('outputBytes')) is not int:
                    raise ValueError('Incomplete native startup output/exit/disclosure observation')
                observers = [event for event in probe['jsonEvents'] if event.get('kind') == 'checkpoint_startup_observation']
                if not probe['credentialTextDisclosed'] and (len(observers) != 1 or observers[0].get('exitCode') != probe['exitCode'] or type(observers[0].get('listenerStarts')) is not int):
                    raise ValueError('Missing native listener-boundary observation')
        elif item['id'] == 'BOUNDARY-001':
            captures = [event for event in item['raw'] if 'boundaryVariants' in event]
            if len(captures) != 1 or not harness.same_json(captures[0].get('specification'), {'fixtureVariants': case['fixtureVariants'], 'requestOrRun': case['requestOrRun']}):
                raise ValueError('Changed original boundary variant inventory/operations')
            variants = captures[0]['boundaryVariants']
            if len(variants) != len(case['fixtureVariants']) or any(variant.get('variant_index') != index or variant.get('fixture_variant') != case['fixtureVariants'][index] for index, variant in enumerate(variants)):
                raise ValueError('Missing/reordered original native boundary variant')
            outage, provider = variants
            if outage.get('healthyControl', {}).get('status') != 200 or outage.get('outage', {}).get('response', {}).get('status', 0) < 500:
                raise ValueError('Missing healthy/outage actual HTTP causal control')
            calls = provider.get('providerCalls', [])
            if len(calls) != 1 or calls[0].get('responseStatus') != 401 or calls[0].get('authorizationPresent') is not True or not harness.same_json(provider.get('profiles'), {'activation': {'executionMs': 60000, 'leaseMs': 40000}, 'delivery': {'executionMs': 60000, 'leaseMs': 40000}}) or provider.get('nativeFixtureCreation', {}).get('status') != 201 or provider.get('nativeHostOutcome', {}).get('selected') != 1 or provider.get('nativeHostOutcome', {}).get('attempted') != 1:
                raise ValueError('Missing actual native provider401/host execution/fixture/profile evidence')


def validate_checkpoint(report: dict, evidence: Path, dependencies: Path, root=ROOT):
    harness.validate_report(report, root)
    checkpoint = report['checkpoint']
    if checkpoint['supported_ids'] != list(SUPPORTED):
        raise ValueError('Changed supported checkpoint IDs')
    instrumented = hashes(evidence/'application/build', ('node_modules',))
    if instrumented != checkpoint['generated_instrumented']:
        raise ValueError('Changed generated instrumented artifacts')
    original = dict(instrumented)
    for name, retained in (('persistence.ts', 'original-persistence.ts'),
                           ('first-party-authentication.ts', 'original-authentication.ts'),
                           ('jwt-authentication.ts', 'original-jwt-authentication.ts'),
                           ('app.ts', 'original-app.ts')):
        original['target/'+name] = harness.digest(evidence/retained)
    if original != checkpoint['generated_original']:
        raise ValueError('Changed retained generated original artifacts')
    if hashes(dependencies/'node_modules/jose') != checkpoint['dependency_files']:
        raise ValueError('Changed actual pinned JWT dependency files')
    if {name: harness.digest(evidence/name) for name in ('jwks-fixture-cert.pem', 'jwks-fixture-key.pem')} != checkpoint['tls_fixture']:
        raise ValueError('Changed ephemeral local JWKS TLS fixture')
    if {name: hashes(evidence/name) for name in ('owner-mutation-source', 'owner-policy-projection')} != checkpoint['owner_partial_sources'] or harness.digest(evidence/'owner-partial-probe.json') != checkpoint['owner_partial_probe_sha256']:
        raise ValueError('Changed partial owner refusal/review evidence')
    outputs = []
    for backend in harness.BACKENDS:
        path = evidence/f'{backend}-raw.json'
        if harness.digest(path) != checkpoint['raw_files'][path.name]:
            raise ValueError('Changed raw observations')
        outputs.append(json.loads(path.read_text()))
    cases = {case['id']: case for case in harness.read_contract(root)['cases']}
    for output in outputs:
        if 'partialEvidence' in output: validate_partial_outputs(output, cases)
    plan_path = evidence/'application/build/persistence/entities.json'
    expected_plans = [plan for plan in json.loads(plan_path.read_text())['query_plans'] if plan.get('entity') == 'Todo' and plan.get('strategy') == 'indexed_keyset_page']
    expected_failures = json.loads((evidence/'application/build/audit/failures.json').read_text())['failures']
    expected_maintenance = json.loads((evidence/'application/build/audit/lifecycle-maintenance.json').read_text())
    expected_source_revision = json.loads((evidence/'application/build/app.meta.json').read_text())['source_revision']
    expected_jobs = json.loads((evidence/'application/build/audit/jobs.json').read_text())
    for output in outputs:
        for item in output['results']:
            journals = [scenario['raw'] for scenario in item['scenarios']] if item['id'] == 'REL-001' else [item['raw']]
            for raw in journals:
                for event in raw:
                    if 'reminderFixture' in event and not harness.same_json(event['reminderFixture'].get('jobMetadata'), expected_jobs):
                        raise ValueError('Raw reminder binding differs from generated artifact')
                    if 'maintenanceFixture' in event and (not harness.same_json(event['maintenanceFixture'].get('metadata'), expected_maintenance) or event['maintenanceFixture'].get('sourceRevision') != expected_source_revision):
                        raise ValueError('Raw maintenance authority differs from generated artifact')
                    if 'queryMetadata' in event and not harness.same_json(event['queryMetadata'], expected_plans):
                        raise ValueError('Raw query metadata differs from generated artifact')
                    if event.get('failureMetadata') is not None and not any(harness.same_json(event['failureMetadata'], item) for item in expected_failures):
                        raise ValueError('Raw refresh failure mapping differs from generated artifact')
                    if 'jwtFixture' in event and event['jwtFixture'].get('certificateSha256') != checkpoint['tls_fixture']['jwks-fixture-cert.pem']:
                        raise ValueError('Raw local JWT issuer trust differs from captured TLS fixture')
    reconstructed = deepcopy(report)
    apply_checkpoint(reconstructed, outputs, root)
    if not harness.same_json(reconstructed['results'], report['results']):
        raise ValueError('Ledger observations differ from raw observations')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path, help='New evidence directory; never overwrites')
    parser.add_argument('--jwt-dependencies', type=Path, default=ROOT/'build/validation/jwt-dependencies')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    environment = {key: value for key, value in os.environ.items() if key not in
                   ('DATABASE_URL', 'SQLITE_PATH', 'JADPO_BIN', 'JADPO_DEBUG_TARGET_STACKS')}
    canonical = ROOT/'jadpo/crates/core/src/runtime/jwt'
    dependencies = args.jwt_dependencies.resolve()
    for name in ('package.json', 'bun.lock'):
        if (canonical/name).read_bytes() != (dependencies/name).read_bytes():
            raise ValueError('Pinned dependency declaration/lock differs')
    installed = json.loads((dependencies/'node_modules/jose/package.json').read_text())
    if installed['version'] != json.loads((canonical/'dependency.json').read_text())['version']:
        raise ValueError('Pinned jose version differs')
    dependency_binding = hashes(dependencies/'node_modules/jose')
    source_before = harness.provenance(ROOT)['files']
    safe_run(['cargo', 'build', '--locked', '-p', 'jadpo-cli'], cwd=ROOT/'jadpo', env=environment)
    binding = harness.provenance(ROOT)
    if binding['files'] != source_before:
        raise ValueError('Source changed during compiler build')
    project = output/'application'
    shutil.copytree(ROOT/'examples/golden-todo-migration', project, ignore=shutil.ignore_patterns('build'))
    external = project/'tests/assurance/service-reference-mail-v0.1.json'
    external.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT/'tests/assurance/service-reference-mail-v0.1.json', external)
    safe_run([str(ROOT/'jadpo/target/debug/jadpo'), 'build', str(project)], env=environment)
    generated = hashes(project/'build')
    owner_partial_sources = prepare_owner_partial(output, project, environment)
    target = project/'build/target'
    original = (target/'persistence.ts').read_text()
    declaration = 'import { SQL } from "bun";'
    if original.count(declaration) != 1:
        raise ValueError('Generated SQL instrumentation import changed')
    (output/'original-persistence.ts').write_text(original)
    storage_entry = '  async get(id: string) {'
    if original.count(storage_entry) != 1:
        raise ValueError('Generated native session storage observation boundary changed')
    (target/'persistence.ts').write_text('import { observeAuthenticationStorageRead } from ' + json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';\n' + original.replace(declaration,
        'import { SQL } from ' + json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';').replace(storage_entry, storage_entry+'\n    observeAuthenticationStorageRead();') + '\nexport { persistenceSync, persistenceAsync };\n')
    authentication = (target/'first-party-authentication.ts').read_text()
    entry = 'async authenticate(request: Request, freshAuthority: boolean, operationNow: number, mutates = false): Promise<AuthPrincipal> {'
    if authentication.count(entry) != 1:
        raise ValueError('Generated authentication instrumentation entry changed')
    (output/'original-authentication.ts').write_text(authentication)
    replacements = {
        entry: entry + '\n      recordAuthenticationAttempt();',
        'if (externallyVerified) return principal;': 'if (externallyVerified) { recordAuthenticatedPrincipal(principal); return principal; }',
        '      return principal;\n    },\n    async refresh(': '      recordAuthenticatedPrincipal(principal);\n      return principal;\n    },\n    async refresh(',
        '    return { credential, csrfToken, setCookie, expires };': '    recordCredentialIssued();\n    return { credential, csrfToken, setCookie, expires };',
        '    return result.principal;': '    recordResolvedAuthorityPrincipal(result.principal);\n    return result.principal;',
        '      return { credential, credentialId: id, serviceId: record.serviceId, expires };': '      recordCredentialIssued();\n      return { credential, credentialId: id, serviceId: record.serviceId, expires };',
        '    return { credential: `${message}.${await sign(key, message)}`, expires, credentialId: record.id, serviceId: record.serviceId };': '    const credential = `${message}.${await sign(key, message)}`;\n    recordCredentialIssued();\n    return { credential, expires, credentialId: record.id, serviceId: record.serviceId };',
    }
    for before, after in replacements.items():
        if authentication.count(before) != 1:
            raise ValueError('Generated authentication observation boundary changed')
        authentication = authentication.replace(before, after)
    (target/'first-party-authentication.ts').write_text('import { recordAuthenticationAttempt, recordAuthenticatedPrincipal, recordCredentialIssued, recordResolvedAuthorityPrincipal } from ' +
        json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';\n' + authentication)
    jwt = (target/'jwt-authentication.ts').read_text()
    jwt_return = '        return Object.freeze({ kind: "valid", subject: payload.sub, authenticationStrength: "jwt" });'
    if jwt.count(jwt_return) != 1:
        raise ValueError('Generated native JWT verification observation boundary changed')
    (output/'original-jwt-authentication.ts').write_text(jwt)
    (target/'jwt-authentication.ts').write_text('import { recordJwtVerification } from ' + json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';\n' + jwt.replace(jwt_return,
        '        recordJwtVerification({ strategy: name, subject: payload.sub, issuer: payload.iss, audience: payload.aud, issuedAt: payload.iat, expires: payload.exp });\n' + jwt_return))
    application = (target/'app.ts').read_text()
    if any(application.count('function '+name+'(') != 1 for name in ('validate_TodoView', 'validate_UserWithTodos', 'validate_TodoPage')):
        raise ValueError('Generated output validation instrumentation changed')
    (output/'original-app.ts').write_text(application)
    (target/'app.ts').write_text(application + '\nexport { validate_TodoView, validate_UserWithTodos, validate_TodoPage };\n')
    (target/'node_modules').symlink_to(dependencies/'node_modules', target_is_directory=True)
    safe_run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
              '-keyout', str(output/'jwks-fixture-key.pem'), '-out', str(output/'jwks-fixture-cert.pem'),
              '-days', '2', '-subj', '/CN=127.0.0.1', '-addext', 'subjectAltName=IP:127.0.0.1'], env=environment)
    tls_binding = {name: harness.digest(output/name) for name in ('jwks-fixture-cert.pem', 'jwks-fixture-key.pem')}
    instrumented_binding = hashes(project/'build', ('node_modules',))
    outputs = []
    for backend in harness.BACKENDS:
        raw_path = output/f'{backend}-raw.json'
        command = ['bun', '--no-install', '--env-file=/dev/null', str(ROOT/'tools/golden_http_checkpoint.ts'),
                   str(project), backend, str(ROOT/harness.ACCEPTANCE), str(raw_path)]
        if backend == 'postgres':
            with postgres(environment) as isolated:
                safe_run(command, env=isolated)
        else:
            safe_run(command, env=environment)
        outputs.append(json.loads(raw_path.read_text()))
    report = harness.run_cases(ROOT, accepted_job001='JOB-001' in SUPPORTED)
    apply_checkpoint(report, outputs, ROOT)
    report['checkpoint'] = {'scope': 'Migrated application checkpoint; original frozen source is not executing',
                            'supported_ids': list(SUPPORTED), 'fresh_build_command': ['cargo', 'build', '--locked', '-p', 'jadpo-cli'],
                            'generated_original': generated, 'generated_instrumented': instrumented_binding,
                            'raw_files': {f'{backend}-raw.json': harness.digest(output/f'{backend}-raw.json') for backend in harness.BACKENDS},
                            'dependency_files': dependency_binding,
                            'tls_fixture': tls_binding,
                            'owner_partial_sources': owner_partial_sources,
                            'owner_partial_probe_sha256': harness.digest(output/'owner-partial-probe.json'),
                            'remaining': 'Scoped supporting checkpoint; unobserved cases, complete accepted-successor parity and final exact-source require-golden evidence are not claimed'}
    if harness.provenance(ROOT) != binding:
        raise ValueError('Source/compiler changed during checkpoint')
    validate_checkpoint(report, output, dependencies)
    (output/'golden-checkpoint.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    print(f'Checkpoint: {sum(entry["status"] == "passed" for entry in report["results"])} passed, '
          f'{sum(entry["status"] == "failed" for entry in report["results"])} failed, '
          f'{sum(entry["status"] == "not_run" for entry in report["results"])} not_run; {output}')
    return 1 if report['status'] == 'failed' else 0


if __name__ == '__main__':
    raise SystemExit(main())
