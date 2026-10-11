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
                     'tools/golden_checkpoint_fetch.ts', 'tools/golden_sql_observer.ts',
                     'tests/runtime/golden-auth-http-jwt.test.ts',
                     'tests/runtime/golden-checkpoint-fetch.test.ts',
                     'tests/validation/test_golden_checkpoint.py'):
            destination = self.root/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT/name, destination)
        self.compiler = self.root/'jadpo/target/debug/jadpo'
        self.compiler.parent.mkdir(parents=True)
        self.compiler.write_text('synthetic compiler digest fixture; never executable')
        self.cases = {case['id']: case for case in harness.read_contract(self.root)['cases']}

    def raw(self, case_id, backend='sqlite'):
        case = self.cases[case_id]
        if case_id == 'REL-001':
            alice = '00000000-0000-4000-8000-000000000001'
            bob = '00000000-0000-4000-8000-000000000002'
            variants = ['alice zero todos', 'alice one todo', 'alice 100 todos']
            raw = []
            for index, (variant, count) in enumerate(zip(variants, [0, 1, 100])):
                rows = []
                for todo_index in range(count):
                    created = f'2026-01-15T10:{todo_index // 2:02d}:00.000Z'
                    rows.append({'id': f'00000000-0000-4000-8000-{3000 + todo_index:012d}',
                                 'ownerId': alice, 'title': f'alice todo {todo_index + 1}', 'status': 'open',
                                 'dueAt': None, 'createdAt': created, 'updatedAt': created, 'deletedAt': None})
                if index == 2:
                    rows.append({'id': '00000000-0000-4000-8000-000000000201', 'ownerId': alice,
                                 'title': 'deleted Alice todo', 'status': 'open', 'dueAt': None,
                                 'createdAt': '2026-01-15T09:00:00.000Z', 'updatedAt': '2026-01-15T09:00:00.000Z',
                                 'deletedAt': '2026-01-16T00:00:00.000Z'})
                rows.append({'id': '00000000-0000-4000-8000-000000000202', 'ownerId': bob,
                             'title': 'Bob foreign todo', 'status': 'open', 'dueAt': None,
                             'createdAt': '2026-01-15T08:00:00.000Z', 'updatedAt': '2026-01-15T08:00:00.000Z',
                             'deletedAt': None})
                raw.append({'fixture': {'variantIndex': index, 'variant': variant, 'rows': rows}})
                active = [row for row in rows if row['ownerId'] == alice and row['deletedAt'] is None]
                items = [{'id': row['id'], 'title': row['title'], 'status': row['status'],
                          'due_at': row['dueAt'], 'created_at': row['createdAt'], 'updated_at': row['updatedAt']}
                         for row in active]
                raw.append({'method': 'GET', 'path': f'/users/{alice}/todos', 'auth': 'alice-session',
                            'requestJson': None, 'requestStartedAt': 1768478400000 + index,
                            'responseCompletedAt': 1768478400000 + index, 'status': 200,
                            'body': {'user_id': alice, 'todos': items}, 'authenticationAttempts': 1,
                            'principal': {'kind': 'user', 'subject': 'alice', 'identityId': alice},
                            'todoLimitBindings': [101],
                            'statements': [
                                {'sql': 'SELECT "id" FROM "user" WHERE "id" = ? AND ("status" = \'<literal>\') LIMIT 2', 'operation': 'SELECT'},
                                {'sql': 'SELECT "id", "owner_id", "title", "status", "due_at", "created_at", "updated_at" FROM "todo" WHERE "owner_id" = ?1 AND "deleted_at" IS NULL AND ("deleted_at" IS NULL) ORDER BY "created_at" ASC, "id" ASC LIMIT ?4', 'operation': 'SELECT'}]})
            return raw
        if case_id == 'REL-002':
            alice = '00000000-0000-4000-8000-000000000001'
            missing_owner = '00000000-0000-4000-8000-000000009999'
            control_id = '00000000-0000-4000-8000-000000000021'
            orphan_id = '00000000-0000-4000-8000-000000000099'
            created = '2026-01-15T10:00:00.000Z'
            control = {'id': control_id, 'ownerId': alice, 'title': 'Alice relationship control',
                       'status': 'open', 'dueAt': None, 'reminderSentAt': None,
                       'createdAt': created, 'updatedAt': created, 'deletedAt': None}
            values = ', '.join(f'?{index}' if backend == 'sqlite' else '?'
                               for index in range(1, 10))
            insert = ('INSERT INTO "todo" ("id", "owner_id", "title", "status", "due_at", "reminder_sent_at", "created_at", "updated_at", "deleted_at") '
                      f'VALUES ({values}) RETURNING "id", "owner_id", "title", "status", "due_at", "reminder_sent_at", "created_at", "updated_at", "deleted_at"')
            return [
                {'fixture': {'alice': {'id': alice, 'subject': 'alice', 'status': 'active'},
                             'missingOwnerId': missing_owner, 'missingOwnerRowsBefore': [],
                             'duplicateTodoRowsBefore': [], 'controlTodo': control}},
                {'persistenceAttempt': {'operation': 'create.Todo', 'todoId': orphan_id,
                    'ownerId': missing_owner, 'backend': backend, 'accepted': False,
                    'faultKind': 'constraint', 'constraintIdentity': None,
                    'normalisedFailure': 'relationship_conflict', 'rawDatabaseDetailsPublic': False,
                    'statements': [{'sql': 'BEGIN IMMEDIATE' if backend == 'sqlite' else 'BEGIN', 'operation': 'BEGIN'},
                                   {'sql': insert, 'operation': 'INSERT'},
                                   {'sql': 'ROLLBACK', 'operation': 'ROLLBACK'}]}},
                {'readback': {'orphanRows': [], 'aliceControlTodos': [control]}},
            ]
        if case_id == 'REL-004':
            alice = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            disabled_at = '2026-01-15T11:00:00.000Z'
            created = '2026-01-15T10:00:00.000Z'
            user = {'id': alice, 'subject': 'alice', 'status': 'disabled', 'disabledAt': disabled_at}
            todo = {'id': todo_id, 'ownerId': alice, 'title': 'Alice retained todo', 'status': 'open',
                    'dueAt': None, 'reminderSentAt': None, 'createdAt': created,
                    'updatedAt': created, 'deletedAt': None}
            delete = 'DELETE FROM "user" WHERE "id" = ?1' if backend == 'sqlite' else 'DELETE FROM "user" WHERE "id" = $1'
            return [
                {'fixture': {'user': user, 'referencingServicesBefore': [], 'retainedTodosBefore': [todo]}},
                {'persistenceAttempt': {'operation': 'delete.User', 'userId': alice, 'backend': backend,
                    'accepted': False, 'faultKind': 'constraint', 'relationshipConstraintMatched': True,
                    'normalisedFailure': 'relationship_conflict', 'todoRowExists': True,
                    'rawDatabaseDetailsPublic': False,
                    'statements': [{'sql': 'BEGIN IMMEDIATE' if backend == 'sqlite' else 'BEGIN', 'operation': 'BEGIN'},
                                   {'sql': delete, 'operation': 'DELETE'},
                                   {'sql': 'ROLLBACK', 'operation': 'ROLLBACK'}]}},
                {'readback': {'user': user, 'retainedTodos': [todo]}},
            ]
        if case_id in ('PATCH-001', 'PATCH-002', 'PATCH-005'):
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            created = '2026-01-15T10:00:00.000Z'
            due = '2026-02-01T00:00:00.000Z'
            reminder = '2026-01-20T00:00:00.000Z'
            updated = '2026-01-15T10:01:00.000Z'
            explicit_none = case_id == 'PATCH-002'
            changed_due = case_id == 'PATCH-005'
            stored_title = 'alice todo t1' if explicit_none or changed_due else 'Changed'
            stored_due = None if explicit_none else '2026-02-15T09:00:00.000Z' if changed_due else due
            stored_reminder = None if explicit_none or changed_due else reminder
            request_json = ({'due_at': None} if explicit_none else
                            {'due_at': '2026-02-15T09:00:00Z'} if changed_due else {'title': 'Changed'})
            updated = '2026-01-22T00:00:00.000Z' if changed_due else updated
            request_started = 1769040000000 if changed_due else 1768478400000
            patch_sql = ('UPDATE "todo" SET "title" = CASE WHEN ?1 THEN ?2 ELSE "title" END, '
                         '"status" = CASE WHEN ?3 THEN ?4 ELSE "status" END, '
                         '"due_at" = CASE WHEN ?5 THEN ?6 ELSE "due_at" END, '
                         '"reminder_sent_at" = CASE WHEN ?5 THEN ?7 ELSE "reminder_sent_at" END '
                         'WHERE "id" = ?8 AND ("deleted_at" IS NULL) AND (("todo"."owner_id" = ?9)) RETURNING "id"')
            raw = [{'fixture': {'id': todo_id, 'ownerId': user_id, 'title': 'alice todo t1', 'status': 'open',
                                'dueAt': due, 'reminderSentAt': reminder, 'createdAt': created,
                                'updatedAt': created, 'deletedAt': None}},
                   {'method': 'PATCH', 'path': f'/todos/{todo_id}', 'auth': 'alice-session',
                    'requestJson': request_json, 'requestStartedAt': request_started,
                    'responseCompletedAt': request_started if changed_due else request_started + 10, 'status': 200,
                    'body': {'id': todo_id, 'title': stored_title, 'status': 'open', 'due_at': stored_due,
                             'created_at': created, 'updated_at': updated},
                    'authenticationAttempts': 1,
                    'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                    'statements': [
                        {'sql': 'BEGIN IMMEDIATE', 'operation': 'BEGIN'},
                        {'sql': 'SELECT "id", "owner_id", "title", "status", "due_at", "reminder_sent_at", "created_at", "updated_at", "deleted_at" FROM "todo" WHERE "id" = ?1 AND "owner_id" = ?2 AND "deleted_at" IS NULL LIMIT 2', 'operation': 'SELECT'},
                        {'sql': 'SELECT "id", "owner_id", "title", "status", "due_at", "reminder_sent_at", "created_at", "updated_at", "deleted_at" FROM "todo" WHERE "id" = ?1 AND "owner_id" = ?2 AND "deleted_at" IS NULL LIMIT 2 FOR UPDATE', 'operation': 'SELECT'},
                        {'sql': patch_sql, 'operation': 'UPDATE'},
                        {'sql': 'UPDATE "todo" SET "updated_at" = ?1 WHERE "id" = ?2 AND ("deleted_at" IS NULL) RETURNING "id"', 'operation': 'UPDATE'},
                        {'sql': 'COMMIT', 'operation': 'COMMIT'},
                    ]},
                   {'persistence': {'id': todo_id, 'ownerId': user_id, 'title': stored_title, 'status': 'open',
                                    'dueAt': stored_due, 'reminderSentAt': stored_reminder, 'createdAt': created,
                                    'updatedAt': updated, 'deletedAt': None}}]
            return raw
        if case_id == 'USER-001':
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            issued = '2026-01-15T11:59:00.000Z'
            disabled = '2026-01-15T12:00:00.000Z'
            expires = '2026-01-15T12:04:00.000Z'
            todo = {'id': todo_id, 'ownerId': user_id, 'title': 'retained user todo', 'status': 'open',
                    'dueAt': None, 'createdAt': issued, 'updatedAt': issued, 'deletedAt': None}
            fixture = {'user': {'id': user_id, 'subject': 'alice', 'status': 'active', 'disabledAt': None},
                       'todo': deepcopy(todo),
                       'session': {'issuedAt': 1768478340000, 'expiresAt': 1768478640000,
                                   'available': True, 'csrfAvailable': True}}
            primary = {'method': 'DELETE', 'path': f'/users/{user_id}', 'auth': 'alice-session',
                       'requestJson': None, 'requestStartedAt': 1768478400000, 'responseCompletedAt': 1768478400000,
                       'status': 204, 'body': None, 'authenticationAttempts': 1,
                       'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                       'statements': [
                           {'sql': 'SELECT "id" FROM "__jadpo_auth_sessions" WHERE "id" = ?', 'operation': 'SELECT'},
                           {'sql': 'SELECT "id" FROM "user" WHERE "authentication_subject" = ? AND "status" = \'<literal>\'', 'operation': 'SELECT'},
                           {'sql': 'BEGIN', 'operation': 'BEGIN'},
                           {'sql': 'SELECT "id" FROM "user" WHERE "id" = ? AND ("status" = \'<literal>\') LIMIT 2', 'operation': 'SELECT'},
                           {'sql': 'UPDATE "user" SET "status" = ?, "disabled_at" = ? WHERE "id" = ? AND "status" = ?', 'operation': 'UPDATE'},
                           {'sql': 'COMMIT', 'operation': 'COMMIT'}]}
            after = {'stage': 'after_disable', 'user': {'id': user_id, 'subject': 'alice', 'status': 'disabled',
                      'disabledAt': disabled}, 'todo': deepcopy(todo)}
            repeated = {'method': 'DELETE', 'path': f'/users/{user_id}', 'auth': 'alice-session',
                        'requestJson': None, 'requestStartedAt': 1768478401000, 'responseCompletedAt': 1768478401000,
                        'status': 403, 'body': {'error': {'code': 'user_disabled', 'request_id': 'req_repeat'}},
                        'authenticationAttempts': 1,
                        'statements': [
                            {'sql': 'SELECT "id" FROM "__jadpo_auth_sessions" WHERE "id" = ?', 'operation': 'SELECT'},
                            {'sql': 'SELECT "id" FROM "user" WHERE "authentication_subject" = ? AND "status" = \'<literal>\'', 'operation': 'SELECT'}]}
            refresh = {'at': 1768478402000, 'outcome': 'rejected', 'declaredFailure': 'UserDisabled',
                       'internalCode': 'principal_inactive', 'credentialReturned': False,
                       'publicFailureCode': 'user_disabled',
                       'statements': [
                           {'sql': 'SELECT "id" FROM "__jadpo_auth_sessions" WHERE "id" = ?', 'operation': 'SELECT'},
                           {'sql': 'SELECT "id", "status" FROM "user" WHERE "authentication_subject" = ? AND "status" = \'<literal>\'', 'operation': 'SELECT'}]}
            expired = {'method': 'GET', 'path': '/todos', 'auth': 'alice-session', 'requestJson': None,
                       'requestStartedAt': 1768478640000, 'responseCompletedAt': 1768478640000,
                       'status': 401, 'body': {'error': {'code': 'invalid_credentials', 'request_id': 'req_expired'}},
                       'authenticationAttempts': 1, 'statements': []}
            final = {**after, 'stage': 'final'}
            return [{'fixture': fixture}, primary, {'persistence': after}, repeated,
                    {'refresh': refresh}, expired, {'persistence': final}]
        if case_id == 'LIST-001':
            alice = '00000000-0000-4000-8000-000000000001'
            bob = '00000000-0000-4000-8000-000000000002'
            tie = '2026-01-15T11:00:00.000Z'
            older = '2026-01-15T10:00:00.000Z'
            fixture = {'rows': [
                {'id': '00000000-0000-4000-8000-000000000031', 'ownerId': alice, 'title': 'alice tie lower id', 'status': 'open', 'createdAt': tie, 'deletedAt': None},
                {'id': '00000000-0000-4000-8000-000000000032', 'ownerId': alice, 'title': 'alice tie higher id', 'status': 'open', 'createdAt': tie, 'deletedAt': None},
                {'id': '00000000-0000-4000-8000-000000000033', 'ownerId': alice, 'title': 'alice older active', 'status': 'open', 'createdAt': older, 'deletedAt': None},
                {'id': '00000000-0000-4000-8000-000000000034', 'ownerId': bob, 'title': 'bob newer active', 'status': 'open', 'createdAt': tie, 'deletedAt': None},
                {'id': '00000000-0000-4000-8000-000000000099', 'ownerId': alice, 'title': 'alice newer deleted', 'status': 'open', 'createdAt': tie, 'deletedAt': tie},
            ]}
            reply = {'method': 'GET', 'path': '/todos?page_size=2', 'auth': 'alice-session', 'requestJson': None,
                     'requestStartedAt': 1768478400000, 'responseCompletedAt': 1768478400000, 'status': 200,
                     'body': {'items': [
                         {'id': '00000000-0000-4000-8000-000000000032', 'title': 'alice tie higher id', 'status': 'open', 'due_at': None, 'created_at': tie, 'updated_at': tie},
                         {'id': '00000000-0000-4000-8000-000000000031', 'title': 'alice tie lower id', 'status': 'open', 'due_at': None, 'created_at': tie, 'updated_at': tie}],
                         'next': {'created_at': tie, 'id': '00000000-0000-4000-8000-000000000031'}},
                     'authenticationAttempts': 1, 'principal': {'kind': 'user', 'subject': 'alice', 'identityId': alice},
                     'statements': [
                         {'sql': 'SELECT "id", "owner_id", "title", "status", "due_at", "created_at", "updated_at" FROM "todo" WHERE ("owner_id" = ? AND "deleted_at" IS NULL) ORDER BY "created_at" DESC, "id" DESC LIMIT ?', 'operation': 'SELECT'}]}
            return [{'fixture': fixture}, reply]
        if case_id == 'LIST-002':
            alice = '00000000-0000-4000-8000-000000000001'
            ids = [f'00000000-0000-4000-8000-0000000000{number}' for number in range(41, 46)]
            rows = [
                {'id': ids[index], 'ownerId': alice, 'title': f'cursor row {index + 1}', 'status': 'open',
                 'createdAt': f'2026-01-15T{10 + index:02d}:00:00.000Z', 'deletedAt': None}
                for index in range(5)
            ]
            ordered = sorted(rows, key=lambda row: (row['createdAt'], row['id']), reverse=True)
            replies = []
            for traversal in range(2):
                cursor = None
                for page_index in range(3):
                    request = case['requests'][page_index]
                    path = request['path']
                    if page_index:
                        marker = '{first.next}' if page_index == 1 else '{second.next}'
                        path = path.replace(marker, quote(json.dumps(cursor, separators=(',', ':')), safe=''))
                    page = ordered[page_index * 2:(page_index + 1) * 2]
                    items = [{'id': row['id'], 'title': row['title'], 'status': row['status'], 'due_at': None,
                              'created_at': row['createdAt'], 'updated_at': row['createdAt']} for row in page]
                    cursor = {'created_at': page[-1]['createdAt'], 'id': page[-1]['id']} if page_index < 2 else None
                    after = '' if page_index == 0 else ' AND (("created_at", "id") < (?, ?))'
                    sql = ('SELECT "id", "owner_id", "title", "status", "due_at", "created_at", "updated_at" '
                           'FROM "todo" WHERE ("owner_id" = ? AND "deleted_at" IS NULL' + after + ') '
                           'ORDER BY "created_at" DESC, "id" DESC LIMIT ?')
                    replies.append({'method': request['method'], 'path': path, 'auth': request.get('auth'),
                                    'requestJson': None, 'requestStartedAt': 1768478400000 + len(replies),
                                    'responseCompletedAt': 1768478400000 + len(replies), 'status': 200,
                                    'body': {'items': items, 'next': cursor}, 'authenticationAttempts': 1,
                                    'principal': {'kind': 'user', 'subject': 'alice', 'identityId': alice},
                                    'statements': [{'sql': sql, 'operation': 'SELECT'}]})
            return [{'fixture': {'rows': rows}}, *replies]
        if case_id == 'LIST-003':
            alice = '00000000-0000-4000-8000-000000000001'
            bob = '00000000-0000-4000-8000-000000000002'
            created = [
                '2026-01-15T10:00:00.000Z', '2026-01-15T11:00:00.000Z',
                '2026-01-15T12:00:00.000Z', '2026-01-15T13:00:00.000Z',
                '2026-01-15T14:00:00.000Z', '2026-01-15T15:00:00.000Z',
                '2026-01-15T16:00:00.000Z',
            ]
            due = ['2026-01-30T00:00:00.000Z', '2026-01-31T00:00:00.000Z',
                   '2026-01-20T00:00:00.000Z', '2026-02-02T00:00:00.000Z', None,
                   '2026-01-25T00:00:00.000Z', '2026-01-31T00:00:00.000Z']
            rows = [
                {'id': f'00000000-0000-4000-8000-0000000000{61 + index}',
                 'ownerId': bob if index == 5 else alice,
                 'title': f'filter fixture {index + 1}',
                 'status': 'closed' if index == 2 else 'open',
                 'dueAt': due[index], 'createdAt': created[index],
                 'deletedAt': '2026-01-16T00:00:00.000Z' if index == 6 else None}
                for index in range(7)
            ]
            expected = [rows[1], rows[0]]
            items = [{'id': row['id'], 'title': row['title'], 'status': row['status'],
                      'due_at': row['dueAt'], 'created_at': row['createdAt'],
                      'updated_at': row['createdAt']} for row in expected]
            reply = {'method': 'GET', 'path': case['request']['path'],
                     'auth': case['request']['auth'], 'requestJson': None,
                     'requestStartedAt': 1768478400000, 'responseCompletedAt': 1768478400000,
                     'status': 200, 'body': {'items': items, 'next': None},
                     'authenticationAttempts': 1,
                     'principal': {'kind': 'user', 'subject': 'alice', 'identityId': alice},
                     'statements': [{'sql': 'SELECT "id", "owner_id", "title", "status", "due_at", "created_at", "updated_at" FROM "todo" WHERE ("owner_id" = ? AND "deleted_at" IS NULL AND "status" = ? AND "due_at" <= ?) ORDER BY "created_at" DESC, "id" DESC LIMIT ?', 'operation': 'SELECT'}]}
            return [{'fixture': {'rows': rows}}, reply]
        if case_id == 'LIST-004':
            return [{'method': 'GET', 'path': case['request']['path'],
                     'auth': case['request']['auth'], 'requestJson': None,
                     'requestStartedAt': 1768478400000, 'responseCompletedAt': 1768478400000,
                     'status': 422,
                     'body': {'error': {'code': 'invalid_value', 'request_id': 'req_invalid_page_size'}},
                     'authenticationAttempts': 1,
                     'principal': {'kind': 'user', 'subject': 'alice',
                                   'identityId': '00000000-0000-4000-8000-000000000001'},
                     'statements': []}]
        replies = []
        requests = case.get('requests', [case.get('request')])
        if case_id == 'DELETE-001':
            request = case['request']
            requests = [request, {'method': 'GET', 'path': request['path'], 'auth': request.get('auth')}]
        for request in requests:
            path = request['path'].replace('/todos/t1', '/todos/00000000-0000-4000-8000-000000000021').replace('/todos/t2', '/todos/00000000-0000-4000-8000-000000000022')
            if case_id == 'AUTH-007':
                path = path.replace('/users/alice', '/users/00000000-0000-4000-8000-000000000001')
            if case_id == 'AUTH-010':
                path = path.replace('/users/alice', '/users/00000000-0000-4000-8000-000000000001')
            replies.append({'method': request['method'], 'path': path,
                            'auth': request.get('auth'), 'requestJson': request.get('json'),
                            'status': 404 if case_id in ('READ-002', 'PATCH-004', 'DELETE-002') else 403 if case_id in ('AUTH-006', 'AUTH-011') else 401,
                            'body': {'error': {'code': 'todo_not_found' if case_id in ('READ-002', 'PATCH-004', 'DELETE-002') else 'user_disabled' if case_id == 'AUTH-006' else 'service_disabled' if case_id == 'AUTH-011' else 'authentication_required', 'request_id': f'req_{len(replies)}'}},
                            'statements': [], 'authenticationAttempts': 1})
        if case_id == 'AUTH-006':
            replies[0]['requestStartedAt'] = 3000
            replies.insert(0, {'fixture': {'subject': 'alice', 'status': 'disabled', 'disabledAt': 2000,
                                           'issuedAt': 1000, 'credentialExpiresAt': 4000}})
        if case_id == 'AUTH-008':
            user_id = '00000000-0000-4000-8000-000000000001'
            claims = {'signatureValid': True, 'version': 1, 'audience': 'todo-api', 'strategy': 'api_bearer',
                      'credentialId': 'session-1', 'subject': 'alice', 'userId': user_id,
                      'issuedAt': 1000, 'expiresAt': 301000}
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3500, 'status': 200,
                               'body': {'todos': []}, 'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [{'sql': 'SELECT id FROM "todo" WHERE "owner_id" = ?', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': {'subject': 'alice', 'userId': user_id, 'status': 'active',
                                           'credentialId': 'session-1', 'credentialIssuedAt': 1000,
                                           'credentialExpiresAt': 301000, 'tokenClaims': claims,
                                           'cacheAdapter': 'unavailable; no cache adapter is configured'}})
        if case_id == 'AUTH-002':
            user_id = '00000000-0000-4000-8000-000000000001'
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3500, 'status': 200,
                               'body': {'todos': []},
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [{'sql': 'SELECT id FROM "__jadpo_auth_sessions" WHERE "id" = ?', 'operation': 'SELECT'},
                                              {'sql': 'SELECT id FROM "user" WHERE "id" = ?', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': {'subject': 'alice', 'userId': user_id, 'status': 'active',
                                           'sessionIssuedAt': 1000, 'sessionExpiresAt': 301000,
                                           'sessionAvailable': True, 'csrfAvailable': True}})
        if case_id == 'AUTH-003':
            user_id = '00000000-0000-4000-8000-000000000001'
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3500, 'status': 200,
                               'body': {'items': [], 'next': None},
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [
                                   {'sql': 'SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = ? LIMIT 2', 'operation': 'SELECT'},
                                   {'sql': 'SELECT id, title, status FROM "todo" WHERE owner_id = ? AND deleted_at IS NULL ORDER BY created_at ASC, id ASC LIMIT ?', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': {'subject': 'alice', 'userId': user_id, 'status': 'active',
                                           'tokenClaims': {'signatureValid': True, 'issuer': 'https://issuer.test',
                                               'audience': 'todo', 'subject': 'alice',
                                               'email': 'attacker@example.test', 'issuedAt': 1000, 'expiresAt': 121000},
                                           'providerCalls': ['https://issuer.test/.well-known/openid-configuration',
                                                             'https://issuer.test/jwks'],
                                           'rejectedProviderCalls': []}})
        if case_id == 'AUTH-004':
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3000,
                               'status': 401, 'body': {'error': {'code': 'ambiguous_credentials', 'request_id': 'req_0'}},
                               'authenticationAttempts': 1, 'statements': []})
            replies.insert(0, {'fixture': {
                'aliceSession': {'subject': 'alice', 'userId': '00000000-0000-4000-8000-000000000001',
                                 'status': 'active', 'issuedAt': 1000, 'expiresAt': 301000,
                                 'sessionAvailable': True},
                'bobJwt': {'subject': 'bob', 'userId': '00000000-0000-4000-8000-000000000002',
                           'status': 'active', 'tokenClaims': {'signatureValid': True,
                               'issuer': 'https://issuer.test', 'audience': 'todo', 'subject': 'bob',
                               'issuedAt': 1000, 'expiresAt': 121000}},
                'providerCalls': [], 'rejectedProviderCalls': []}})
        if case_id == 'AUTH-007':
            user_id = '00000000-0000-4000-8000-000000000001'
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3500, 'status': 200,
                               'body': {'user_id': user_id, 'todos': []},
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'authenticationAttempts': 1})
            replies.insert(0, {'fixture': {'subject': 'alice', 'userId': user_id, 'status': 'active',
                                           'storedEmail': 'alice@example.test',
                                           'tokenClaims': {'signatureValid': True, 'issuer': 'https://issuer.test',
                                               'audience': 'todo', 'subject': 'alice',
                                               'email': 'attacker@example.test', 'issuedAt': 1000, 'expiresAt': 121000},
                                           'providerCalls': ['https://issuer.test/.well-known/openid-configuration',
                                                             'https://issuer.test/jwks'],
                                           'rejectedProviderCalls': []}})
        if case_id == 'AUTH-011':
            replies[0]['requestStartedAt'] = 3000
            replies.insert(0, {'fixture': {'subject': 'reporting', 'status': 'disabled', 'disabledAt': 2000,
                                           'credentialIssuedAt': 1000, 'credentialExpiresAt': 4000}})
        if case_id == 'AUTH-009':
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3500, 'status': 200,
                               'body': {'access_token': '<redacted>', 'token_type': 'Bearer',
                                        'expires_at': '1970-01-01T00:00:05.000Z'},
                               'statements': [
                                   {'sql': 'SELECT "data", "revoked" FROM "__jadpo_auth_service_credentials" WHERE "id" = ?', 'operation': 'SELECT'},
                                   {'sql': 'SELECT * FROM "service_credential" WHERE "id" = ?', 'operation': 'SELECT'},
                                   {'sql': 'SELECT "id", "name", "status" FROM "service" WHERE "name" = ?', 'operation': 'SELECT'}],
                               'serviceExchange': {'signatureValid': True, 'version': 1,
                                   'audience': 'todo-service-token', 'strategy': 'api_bearer',
                                   'credentialId': 'credential-1', 'subject': 'reporting',
                                   'serviceId': 'service-1', 'issuedAt': 3200, 'expiresAt': 5000,
                                   'principalKind': 'service', 'authenticationStrength': 'api_key',
                                   'secretInOutput': False, 'outputTokenRedacted': True}})
            replies.insert(0, {'fixture': {'subject': 'reporting', 'serviceId': 'service-1',
                                           'status': 'active', 'disabledAt': None,
                                           'credentialId': 'credential-1', 'credentialIssuedAt': 1000,
                                           'credentialExpiresAt': 6000}})
        if case_id == 'AUTH-010':
            user_id = '00000000-0000-4000-8000-000000000001'
            claims = {'signatureValid': True, 'version': 1, 'audience': 'todo-api', 'strategy': 'api_bearer',
                      'credentialId': 'session-2', 'subject': 'alice', 'userId': user_id,
                      'issuedAt': 1000, 'expiresAt': 301000}
            replies[0].update({'requestStartedAt': 3000, 'responseCompletedAt': 3500, 'status': 204,
                               'body': None, 'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [
                                   {'sql': 'SELECT id, authentication_subject, status FROM "user" WHERE authentication_subject = ? AND status = \'active\'', 'operation': 'SELECT'},
                                   {'sql': 'UPDATE "user" SET status = ?, disabled_at = ? WHERE id = ?', 'operation': 'UPDATE'}]})
            replies.insert(0, {'fixture': {'subject': 'alice', 'userId': user_id, 'status': 'active',
                                           'credentialId': 'session-2', 'credentialIssuedAt': 1000,
                                           'credentialExpiresAt': 301000, 'tokenClaims': claims}})
            replies.append({'persistence': {'userId': user_id, 'subject': 'alice', 'status': 'disabled', 'disabledAt': 3200}})
        if case_id == 'PATCH-003':
            replies.insert(0, {'fixture': {'id': '00000000-0000-4000-8000-000000000021',
                                           'ownerId': '00000000-0000-4000-8000-000000000001',
                                           'status': 'open', 'deletedAt': None}})
            replies[1]['status'] = 422
            replies[1]['body'] = {'error': {'code': 'empty_patch', 'request_id': 'req_0'}}
            replies[1]['statements'] = [{'sql':'BEGIN','operation':'BEGIN'}, {'sql':'SELECT todo','operation':'SELECT'}, {'sql':'ROLLBACK','operation':'ROLLBACK'}]
        if case_id == 'PATCH-004':
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000022'
            bob_id = '00000000-0000-4000-8000-000000000002'
            row = {'id': todo_id, 'ownerId': bob_id, 'title': 'private bob todo',
                   'status': 'open', 'deletedAt': None}
            replies[0].update({'status': 404, 'body': {'error': {'code': 'todo_not_found', 'request_id': 'req_0'}},
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [{'sql': 'SELECT "id", "owner_id", "title", "status", "deleted_at" FROM "todo" WHERE "id" = ? AND ("deleted_at" IS NULL) AND (("todo"."owner_id" = ?)) LIMIT 2', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': deepcopy(row)})
            replies.append({'persistence': deepcopy(row)})
        if case_id == 'DELETE-001':
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            clock_now = '2026-01-15T12:00:00.000Z'
            fixture = {'id': todo_id, 'ownerId': user_id, 'title': 'active fixture todo',
                       'status': 'open', 'createdAt': clock_now, 'updatedAt': clock_now,
                       'deletedAt': None, 'clockNow': clock_now}
            persisted = {key: value for key, value in fixture.items() if key != 'clockNow'}
            persisted['deletedAt'] = clock_now
            principal = {'kind': 'user', 'subject': 'alice', 'identityId': user_id}
            replies[0].update({'status': 204, 'body': None,
                               'requestStartedAt': 1768478400000, 'responseCompletedAt': 1768478400000,
                               'principal': deepcopy(principal),
                               'statements': [{'sql': 'BEGIN IMMEDIATE', 'operation': 'BEGIN'},
                                              {'sql': 'UPDATE "todo" SET "deleted_at" = ?, "updated_at" = ? WHERE "id" = ? AND "owner_id" = ? AND "deleted_at" IS NULL', 'operation': 'UPDATE'},
                                              {'sql': 'COMMIT', 'operation': 'COMMIT'}]})
            replies[1].update({'status': 404,
                               'body': {'error': {'code': 'todo_not_found', 'request_id': 'req_hidden'}},
                               'principal': deepcopy(principal),
                               'statements': [{'sql': 'SELECT "id", "owner_id", "title", "status", "deleted_at" FROM "todo" WHERE "id" = ? AND "owner_id" = ? AND "deleted_at" IS NULL', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': fixture})
            replies.insert(2, {'persistence': persisted})
        if case_id == 'DELETE-002':
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            row = {'id': todo_id, 'ownerId': user_id, 'title': 'previously deleted todo',
                   'status': 'open', 'createdAt': '2026-01-15T11:00:00.000Z',
                   'updatedAt': '2026-01-15T11:59:00.000Z',
                   'deletedAt': '2026-01-15T11:59:00.000Z'}
            replies[0].update({'status': 404, 'body': {'error': {'code': 'todo_not_found', 'request_id': 'req_repeat'}},
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [{'sql': 'SELECT "id", "owner_id", "title", "status", "deleted_at" FROM "todo" WHERE "id" = ? AND ("deleted_at" IS NULL) AND (("todo"."owner_id" = ?)) LIMIT 2', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': deepcopy(row)})
            replies.append({'persistence': deepcopy(row)})
        if case_id == 'READ-001':
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            replies[0].update({'status': 200,
                               'body': {'id': todo_id, 'title': 'active fixture todo', 'status': 'open',
                                        'due_at': None, 'created_at': '2026-10-10T20:00:00.000Z',
                                        'updated_at': '2026-10-10T20:00:00.000Z'},
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [{'sql': 'SELECT * FROM "todo" WHERE "id" = ? AND "owner_id" = ? AND "deleted_at" IS NULL', 'operation': 'SELECT'}]})
            replies.insert(0, {'fixture': {'id': todo_id, 'ownerId': user_id, 'title': 'active fixture todo',
                                           'status': 'open', 'deletedAt': None}})
        if case_id == 'CREATE-004':
            replies[0]['body'] = {'id': 'synthetic-id', 'due_at': None}
            replies.append({'persistence': {'id': 'synthetic-id', 'storedDueAt': None}})
        if case_id == 'CREATE-001':
            user_id = '00000000-0000-4000-8000-000000000001'
            todo_id = '00000000-0000-4000-8000-000000000021'
            request = case['request']['json']
            replies[0].update({'status': 201,
                               'body': {'id': todo_id, 'title': request['title'], 'status': 'open',
                                        'due_at': request['due_at'],
                                        'created_at': '2026-01-15T12:00:00.000Z',
                                        'updated_at': '2026-01-15T12:00:00.000Z'},
                               'requestStartedAt': 1768478400000,
                               'responseCompletedAt': 1768478400000,
                               'principal': {'kind': 'user', 'subject': 'alice', 'identityId': user_id},
                               'statements': [{'sql': 'BEGIN IMMEDIATE', 'operation': 'BEGIN'},
                                              {'sql': 'INSERT INTO "todo" ("id", "owner_id", "title") VALUES (?, ?, ?)', 'operation': 'INSERT'},
                                              {'sql': 'COMMIT', 'operation': 'COMMIT'}]})
            replies.insert(0, {'fixture': {'subject': 'alice', 'userId': user_id, 'status': 'active',
                                           'clockNow': '2026-01-15T12:00:00.000Z',
                                           'sessionAvailable': True, 'csrfAvailable': True}})
            replies.append({'persistence': {'id': todo_id, 'ownerId': user_id,
                                            'title': request['title'], 'status': 'open',
                                            'dueAt': '2026-01-20T09:00:00Z',
                                            'createdAt': '2026-01-15T12:00:00.000Z',
                                            'updatedAt': '2026-01-15T12:00:00.000Z',
                                            'deletedAt': None}})
        if case_id == 'CREATE-003':
            replies[0].update({'status': 422,
                               'body': {'error': {'code': 'due_date_in_past', 'request_id': 'req_0'}},
                               'requestStartedAt': 1768478400000,
                               'responseCompletedAt': 1768478400000,
                               'statements': [{'sql': 'BEGIN IMMEDIATE', 'operation': 'BEGIN'},
                                              {'sql': 'ROLLBACK', 'operation': 'ROLLBACK'}]})
            replies.insert(0, {'fixture': {'clockNow': '2026-01-15T12:00:00.000Z'}})
        return replies

    def outputs(self):
        return [{'backend': backend, 'results': [{'id': case_id, 'raw': self.raw(case_id, backend)} for case_id in checkpoint.SUPPORTED]} for backend in harness.BACKENDS]

    def test_all_inventory_entries_retained_and_selected_observations_do_not_infer_pass(self):
        report = harness.run_cases(self.root)
        checkpoint.apply_checkpoint(report, self.outputs(), self.root)
        self.assertEqual(len(report['results']), 88)
        self.assertEqual(sum(entry['status'] == 'not_run' for entry in report['results']), 18)
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

    def test_owned_read_requires_persisted_alice_fixture_and_exact_public_projection(self):
        raw = self.raw('READ-001')
        observed = checkpoint.observations(self.cases['READ-001'], raw)
        self.assertEqual(harness.assess(self.cases['READ-001'], observed), [])
        self.assertEqual(observed[0]['steps'][0]['outputSchema'], 'TodoView')
        self.assertEqual(observed[0]['steps'][0]['privateFieldsAbsent'],
                         ['owner_id', 'deleted_at', 'reminder_sent_at'])
        for mutation in ('foreign_owner', 'foreign_response', 'foreign_principal', 'missing_owner_predicate', 'missing_deleted_predicate', 'missing_principal'):
            changed = deepcopy(raw)
            if mutation == 'foreign_owner': changed[0]['fixture']['ownerId'] = 'foreign-owner'
            elif mutation == 'foreign_response': changed[1]['body']['id'] = 'foreign-todo'
            elif mutation == 'foreign_principal': changed[1]['principal']['subject'] = 'bob'
            elif mutation == 'missing_owner_predicate': changed[1]['statements'][0]['sql'] = 'SELECT * FROM "todo" WHERE "id" = ? AND "deleted_at" IS NULL'
            elif mutation == 'missing_deleted_predicate': changed[1]['statements'][0]['sql'] = 'SELECT * FROM "todo" WHERE "id" = ? AND "owner_id" = ?'
            else: changed[1].pop('principal')
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'READ-001'):
                checkpoint.observations(self.cases['READ-001'], changed)
        private = deepcopy(raw)
        private[1]['body']['owner_id'] = '00000000-0000-4000-8000-000000000001'
        private_observation = checkpoint.observations(self.cases['READ-001'], private)
        self.assertTrue(harness.assess(self.cases['READ-001'], private_observation))

    def test_stored_and_output_values_are_separate_and_require_matching_identity(self):
        raw = self.raw('CREATE-004')
        raw[1]['persistence']['storedDueAt'] = 'unexpected stored date'
        step = checkpoint.observations(self.cases['CREATE-004'], raw)[0]['steps'][0]
        self.assertEqual(step['storedDueAt'], 'unexpected stored date')
        self.assertIsNone(step['outputDueAt'])
        raw[1]['persistence']['id'] = 'foreign'
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['CREATE-004'], raw)

    def test_past_due_create_requires_frozen_error_and_zero_writes(self):
        raw = self.raw('CREATE-003')
        observations = checkpoint.observations(self.cases['CREATE-003'], raw)
        self.assertEqual(harness.assess(self.cases['CREATE-003'], observations), [])
        self.assertEqual(observations[0]['steps'][0]['clockNow'], '2026-01-15T12:00:00.000Z')
        self.assertTrue(observations[0]['steps'][0]['rollbackObserved'])
        for mutation in ('earlier_clock', 'off_clock_request', 'missing_fixture'):
            changed = deepcopy(raw)
            if mutation == 'earlier_clock': changed[0]['fixture']['clockNow'] = '2026-01-15T11:59:58.000Z'
            elif mutation == 'off_clock_request': changed[1]['requestStartedAt'] += 1000
            else: changed.pop(0)
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'CREATE-003'):
                checkpoint.observations(self.cases['CREATE-003'], changed)
        for mutation, statements in (
            ('missing_rollback', [{'sql': 'BEGIN IMMEDIATE', 'operation': 'BEGIN'},
                                  {'sql': 'SELECT 1', 'operation': 'SELECT'}]),
            ('missing_begin', [{'sql': 'ROLLBACK', 'operation': 'ROLLBACK'}]),
            ('committed', [{'sql': 'BEGIN', 'operation': 'BEGIN'},
                           {'sql': 'COMMIT', 'operation': 'COMMIT'}]),
        ):
            changed = deepcopy(raw)
            changed[1]['statements'] = statements
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'transaction rollback'):
                checkpoint.observations(self.cases['CREATE-003'], changed)
        raw[1]['statements'] = [{'sql': 'BEGIN IMMEDIATE', 'operation': 'BEGIN'},
                                {'sql': 'INSERT INTO todo VALUES (...)', 'operation': 'INSERT'},
                                {'sql': 'ROLLBACK', 'operation': 'ROLLBACK'}]
        changed = checkpoint.observations(self.cases['CREATE-003'], raw)
        self.assertTrue(harness.assess(self.cases['CREATE-003'], changed))

    def test_owner_assigned_create_requires_persisted_alice_and_public_todoview(self):
        raw = self.raw('CREATE-001')
        observations = checkpoint.observations(self.cases['CREATE-001'], raw)
        self.assertEqual(harness.assess(self.cases['CREATE-001'], observations), [])
        self.assertEqual(observations[0]['steps'][0]['storedOwner'], 'alice')
        self.assertEqual(observations[0]['steps'][0]['outputSchema'], 'TodoView')
        self.assertTrue(observations[0]['steps'][0]['ownerFieldAbsent'])
        mutations = []
        foreign_owner = deepcopy(raw); foreign_owner[-1]['persistence']['ownerId'] = 'foreign-user'
        mutations.append(('foreign stored owner', foreign_owner))
        foreign_principal = deepcopy(raw); foreign_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('foreign request principal', foreign_principal))
        owner_leak = deepcopy(raw); owner_leak[1]['body']['owner_id'] = '00000000-0000-4000-8000-000000000001'
        mutations.append(('private owner response field', owner_leak))
        mismatched_id = deepcopy(raw); mismatched_id[-1]['persistence']['id'] = 'foreign-todo'
        mutations.append(('response/persistence identity mismatch', mismatched_id))
        wrong_due = deepcopy(raw); wrong_due[-1]['persistence']['dueAt'] = '2026-01-15T12:00:00Z'
        mutations.append(('persisted due date mismatch', wrong_due))
        missing_row = deepcopy(raw); missing_row.pop()
        mutations.append(('missing persisted row', missing_row))
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing active-user fixture', missing_fixture))
        drifted_clock = deepcopy(raw); drifted_clock[0]['fixture']['clockNow'] = '2026-01-15T12:00:01.000Z'
        mutations.append(('wrong fixed clock', drifted_clock))
        off_clock = deepcopy(raw); off_clock[1]['requestStartedAt'] += 1000
        mutations.append(('request outside fixed clock', off_clock))
        committed_rejection = deepcopy(raw); committed_rejection[1]['statements'][2] = {'sql': 'ROLLBACK', 'operation': 'ROLLBACK'}
        mutations.append(('wrong transaction outcome', committed_rejection))
        missing_insert = deepcopy(raw); missing_insert[1]['statements'].pop(1)
        mutations.append(('missing todo insert', missing_insert))
        insert_before_begin = deepcopy(raw)
        insert_before_begin[1]['statements'].insert(0, insert_before_begin[1]['statements'].pop(1))
        mutations.append(('todo insert before transaction', insert_before_begin))
        insert_after_commit = deepcopy(raw)
        insert_after_commit[1]['statements'].append(insert_after_commit[1]['statements'].pop(1))
        mutations.append(('todo insert after commit', insert_after_commit))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'CREATE-001'):
                checkpoint.observations(self.cases['CREATE-001'], changed)

    def test_empty_patch_requires_the_persisted_active_owner_fixture(self):
        raw = self.raw('PATCH-003')
        observations = checkpoint.observations(self.cases['PATCH-003'], raw)
        self.assertEqual(harness.assess(self.cases['PATCH-003'], observations), [])
        self.assertTrue(observations[0]['steps'][0]['rollbackObserved'])
        raw[1]['statements'] = raw[1]['statements'][:-1]
        with self.assertRaisesRegex(ValueError, 'transaction outcome'):
            checkpoint.observations(self.cases['PATCH-003'], raw)
        raw = self.raw('PATCH-003')
        raw[0]['fixture']['ownerId'] = 'foreign-owner'
        with self.assertRaisesRegex(ValueError, 'fixture'):
            checkpoint.observations(self.cases['PATCH-003'], raw)

    def test_disabled_user_case_requires_a_valid_persisted_fixture_and_live_session_timeline(self):
        raw = self.raw('AUTH-006')
        observed = checkpoint.observations(self.cases['AUTH-006'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-006'], observed), [])
        self.assertEqual(observed[0]['steps'][0]['fixtureStatus'], 'disabled')
        self.assertEqual(observed[0]['steps'][0]['timeline'], {
            'issuedAt': 1000, 'disabledAt': 2000, 'requestAt': 3000, 'credentialExpiresAt': 4000})
        for field, value in (('issuedAt', 2500), ('disabledAt', 3500),
                             ('credentialExpiresAt', 2500), ('status', 'active')):
            changed = deepcopy(raw)
            changed[0]['fixture'][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'AUTH-006'):
                checkpoint.observations(self.cases['AUTH-006'], changed)
        equal_time = deepcopy(raw)
        equal_time[1]['requestStartedAt'] = 2000
        with self.assertRaisesRegex(ValueError, 'AUTH-006'):
            checkpoint.observations(self.cases['AUTH-006'], equal_time)
        with self.assertRaisesRegex(ValueError, 'AUTH-006'):
            checkpoint.observations(self.cases['AUTH-006'], raw[1:])

    def test_signed_bearer_case_requires_verified_active_identity_and_zero_authority_queries(self):
        raw = self.raw('AUTH-008')
        observed = checkpoint.observations(self.cases['AUTH-008'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-008'], observed), [])
        self.assertEqual(observed[0]['steps'][0]['principalAuthorityQueries'], 0)
        self.assertEqual(observed[0]['steps'][0]['credentialAuthorityQueries'], 0)
        mutations = []
        for field, value in (('signatureValid', False), ('audience', 'wrong-audience'),
                             ('userId', 'foreign-user')):
            changed = deepcopy(raw)
            changed[0]['fixture']['tokenClaims'][field] = value
            mutations.append((field, changed))
        inactive = deepcopy(raw)
        inactive[0]['fixture']['status'] = 'disabled'
        mutations.append(('inactive user', inactive))
        after_expiry = deepcopy(raw)
        after_expiry[1]['requestStartedAt'] = 302000
        mutations.append(('expired token', after_expiry))
        principal_query = deepcopy(raw)
        principal_query[1]['statements'].append({'sql': 'SELECT id FROM "user" WHERE id = ?', 'operation': 'SELECT'})
        mutations.append(('principal lookup', principal_query))
        credential_query = deepcopy(raw)
        credential_query[1]['statements'].append({'sql': 'SELECT "data" FROM "__jadpo_auth_sessions" WHERE id = ?', 'operation': 'SELECT'})
        mutations.append(('credential lookup', credential_query))
        wrong_principal = deepcopy(raw)
        wrong_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('resolved principal', wrong_principal))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-008'):
                checkpoint.observations(self.cases['AUTH-008'], changed)

    def test_untrusted_jwt_email_cannot_enter_self_scoped_user_output(self):
        raw = self.raw('AUTH-007')
        observations = checkpoint.observations(self.cases['AUTH-007'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-007'], observations), [])
        self.assertEqual(observations[0]['steps'][0]['privateFieldsAbsent'], ['email'])
        self.assertFalse(observations[0]['steps'][0]['providerEmailUsed'])
        mutations = []
        email_leak = deepcopy(raw); email_leak[1]['body']['email'] = 'attacker@example.test'
        mutations.append(('email appears in output', email_leak))
        wrong_stored_email = deepcopy(raw); wrong_stored_email[0]['fixture']['storedEmail'] = 'attacker@example.test'
        mutations.append(('stored fixture collides with JWT email', wrong_stored_email))
        wrong_issuer = deepcopy(raw); wrong_issuer[0]['fixture']['tokenClaims']['issuer'] = 'https://wrong.test'
        mutations.append(('wrong JWT issuer', wrong_issuer))
        wrong_subject = deepcopy(raw); wrong_subject[0]['fixture']['tokenClaims']['subject'] = 'bob'
        mutations.append(('wrong JWT subject', wrong_subject))
        invalid_signature = deepcopy(raw); invalid_signature[0]['fixture']['tokenClaims']['signatureValid'] = False
        mutations.append(('invalid JWT signature', invalid_signature))
        missing_jwks = deepcopy(raw); missing_jwks[0]['fixture']['providerCalls'].pop()
        mutations.append(('missing JWKS fetch', missing_jwks))
        unexpected_fetch = deepcopy(raw); unexpected_fetch[0]['fixture']['rejectedProviderCalls'] = ['https://unexpected.example/metadata']
        mutations.append(('unexpected fetch destination', unexpected_fetch))
        foreign_principal = deepcopy(raw); foreign_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('foreign resolved principal', foreign_principal))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-007'):
                checkpoint.observations(self.cases['AUTH-007'], changed)

    def test_signed_oidc_jwt_get_todos_requires_resolved_alice_and_bounded_authority_queries(self):
        raw = self.raw('AUTH-003')
        observed = checkpoint.observations(self.cases['AUTH-003'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-003'], observed), [])
        step = observed[0]['steps'][0]
        self.assertEqual((step['databaseQueries'], step['authorityQueries'], step['todoQueries']), (2, 1, 1))
        self.assertFalse(step['providerFieldsInOutput'])
        mutations = []
        wrong_subject = deepcopy(raw); wrong_subject[0]['fixture']['tokenClaims']['subject'] = 'bob'
        mutations.append(('wrong JWT subject', wrong_subject))
        invalid_signature = deepcopy(raw); invalid_signature[0]['fixture']['tokenClaims']['signatureValid'] = False
        mutations.append(('invalid JWT signature', invalid_signature))
        foreign_principal = deepcopy(raw); foreign_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('foreign resolved principal', foreign_principal))
        missing_discovery = deepcopy(raw); missing_discovery[0]['fixture']['providerCalls'].pop(0)
        mutations.append(('missing OIDC discovery', missing_discovery))
        missing_authority = deepcopy(raw); missing_authority[1]['statements'].pop(0)
        mutations.append(('missing authority query', missing_authority))
        authority_projection_only = deepcopy(raw)
        authority_projection_only[1]['statements'][0]['sql'] = 'SELECT authentication_subject FROM "user"'
        mutations.append(('authority subject appears only in projection', authority_projection_only))
        authority_or_bypass = deepcopy(raw)
        authority_or_bypass[1]['statements'][0]['sql'] = 'SELECT authentication_subject FROM "user" WHERE authentication_subject = ? OR 1 = 1'
        mutations.append(('authority subject predicate bypassed by top-level OR', authority_or_bypass))
        todo_projection_only = deepcopy(raw)
        todo_projection_only[1]['statements'][1]['sql'] = 'SELECT owner_id, deleted_at FROM "todo"'
        mutations.append(('Todo scope fields appear only in projection', todo_projection_only))
        todo_or_bypass = deepcopy(raw)
        todo_or_bypass[1]['statements'][1]['sql'] = 'SELECT owner_id, deleted_at FROM "todo" WHERE owner_id = ? OR status = \'open\' AND deleted_at IS NULL'
        mutations.append(('Todo owner predicate bypassed by top-level OR', todo_or_bypass))
        unscoped_todos = deepcopy(raw); unscoped_todos[1]['statements'][1]['sql'] = 'SELECT id FROM "todo"'
        mutations.append(('unscoped todo query', unscoped_todos))
        leaked_email = deepcopy(raw); leaked_email[1]['body']['email'] = 'attacker@example.test'
        mutations.append(('provider email in output', leaked_email))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-003'):
                checkpoint.observations(self.cases['AUTH-003'], changed)

    def test_competing_valid_session_and_jwt_fail_before_validation_or_queries(self):
        raw = self.raw('AUTH-004')
        observed = checkpoint.observations(self.cases['AUTH-004'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-004'], observed), [])
        self.assertTrue(observed[0]['steps'][0]['ambiguousBeforeValidation'])
        self.assertEqual(observed[0]['steps'][0]['databaseQueries'], 0)
        mutations = []
        wrong_code = deepcopy(raw); wrong_code[1]['body']['error']['code'] = 'invalid_credentials'
        mutations.append(('wrong ambiguity code', wrong_code))
        database_query = deepcopy(raw); database_query[1]['statements'] = [{'sql': 'SELECT id FROM "user"', 'operation': 'SELECT'}]
        mutations.append(('database query before rejection', database_query))
        provider_call = deepcopy(raw); provider_call[0]['fixture']['providerCalls'] = ['https://issuer.test/jwks']
        mutations.append(('JWT validation before ambiguity rejection', provider_call))
        same_subject = deepcopy(raw); same_subject[0]['fixture']['bobJwt']['tokenClaims']['subject'] = 'alice'
        mutations.append(('competing JWT is not Bob', same_subject))
        inactive_session = deepcopy(raw); inactive_session[0]['fixture']['aliceSession']['status'] = 'disabled'
        mutations.append(('session fixture inactive', inactive_session))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-004'):
                checkpoint.observations(self.cases['AUTH-004'], changed)

    def test_browser_session_case_requires_active_user_safe_output_and_bound_principal(self):
        raw = self.raw('AUTH-002')
        observed = checkpoint.observations(self.cases['AUTH-002'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-002'], observed), [])
        self.assertFalse(observed[0]['steps'][0]['providerFieldsInOutput'])
        mutations = []
        inactive = deepcopy(raw)
        inactive[0]['fixture']['status'] = 'disabled'
        mutations.append(('inactive user', inactive))
        expired = deepcopy(raw)
        expired[0]['fixture']['sessionExpiresAt'] = 2500
        mutations.append(('expired session', expired))
        foreign_principal = deepcopy(raw)
        foreign_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('foreign principal', foreign_principal))
        missing_session = deepcopy(raw)
        missing_session[0]['fixture']['sessionAvailable'] = False
        mutations.append(('missing session', missing_session))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-002'):
                checkpoint.observations(self.cases['AUTH-002'], changed)
        provider_email = deepcopy(raw)
        provider_email[1]['body']['email'] = 'alice@example.test'
        unsafe = checkpoint.observations(self.cases['AUTH-002'], provider_email)
        self.assertTrue(unsafe[0]['steps'][0]['providerFieldsInOutput'])
        self.assertTrue(harness.assess(self.cases['AUTH-002'], unsafe))

    def test_disabled_service_case_requires_a_valid_persisted_fixture_and_live_key_timeline(self):
        raw = self.raw('AUTH-011')
        observed = checkpoint.observations(self.cases['AUTH-011'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-011'], observed), [])
        self.assertEqual(observed[0]['steps'][0]['fixtureStatus'], 'disabled')
        self.assertEqual(observed[0]['steps'][0]['timeline'], {
            'credentialIssuedAt': 1000, 'disabledAt': 2000, 'requestAt': 3000, 'credentialExpiresAt': 4000})
        changed = deepcopy(raw)
        changed[0]['fixture']['subject'] = 'other-service'
        with self.assertRaisesRegex(ValueError, 'AUTH-011'):
            checkpoint.observations(self.cases['AUTH-011'], changed)
        equal_time = deepcopy(raw)
        equal_time[1]['requestStartedAt'] = 2000
        with self.assertRaisesRegex(ValueError, 'AUTH-011'):
            checkpoint.observations(self.cases['AUTH-011'], equal_time)
        missing_attempt = deepcopy(raw)
        missing_attempt[1]['authenticationAttempts'] = 0
        with self.assertRaisesRegex(ValueError, 'exchange attempt'):
            checkpoint.observations(self.cases['AUTH-011'], missing_attempt)
        raw = self.raw('PATCH-003')[1:]
        with self.assertRaisesRegex(ValueError, 'fixture'):
            checkpoint.observations(self.cases['PATCH-003'], raw)

    def test_service_exchange_requires_verified_claims_and_redacts_both_credentials(self):
        raw = self.raw('AUTH-009')
        observed = checkpoint.observations(self.cases['AUTH-009'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-009'], observed), [])
        step = observed[0]['steps'][0]
        self.assertEqual((step['principalKind'], step['principalService'], step['secretInOutput']),
                         ('service', 'reporting', False))
        mutations = []
        for key, value in (('signatureValid', False), ('secretInOutput', True),
                           ('outputTokenRedacted', False), ('principalKind', 'user'),
                           ('serviceId', 'foreign-service')):
            changed = deepcopy(raw)
            changed[1]['serviceExchange'][key] = value
            mutations.append((key, changed))
        for field, value in (('status', 'disabled'), ('credentialExpiresAt', 3200),
                             ('subject', 'different-service')):
            changed = deepcopy(raw)
            changed[0]['fixture'][field] = value
            mutations.append((field, changed))
        for field, value in (('requestStartedAt', 3300), ('responseCompletedAt', 5000)):
            changed = deepcopy(raw)
            changed[1][field] = value
            mutations.append((field, changed))
        missing_credential_lookup = deepcopy(raw)
        missing_credential_lookup[1]['statements'].pop(0)
        mutations.append(('credential lookup', missing_credential_lookup))
        missing_authority_lookup = deepcopy(raw)
        missing_authority_lookup[1]['statements'].pop()
        mutations.append(('active service lookup', missing_authority_lookup))
        unredacted = deepcopy(raw)
        unredacted[1]['body']['access_token'] = 'jdx1.secret-output'
        mutations.append(('unredacted access token', unredacted))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-009'):
                checkpoint.observations(self.cases['AUTH-009'], changed)

    def test_fresh_authority_disable_requires_bounded_bearer_query_and_persistence_evidence(self):
        raw = self.raw('AUTH-010')
        observed = checkpoint.observations(self.cases['AUTH-010'], raw)
        self.assertEqual(harness.assess(self.cases['AUTH-010'], observed), [])
        step = observed[0]['steps'][0]
        self.assertEqual((step['principalAuthorityQueries'], step['credentialAuthorityQueries'],
                          step['credentialAuthorityQueriesRecorded'], step['persistedStatus']),
                         (1, 0, True, 'disabled'))
        mutations = []
        for field, value in (('signatureValid', False), ('audience', 'wrong-audience'),
                             ('userId', 'foreign-user'), ('expiresAt', 2500)):
            changed = deepcopy(raw)
            changed[0]['fixture']['tokenClaims'][field] = value
            mutations.append((f'claim {field}', changed))
        inactive_fixture = deepcopy(raw)
        inactive_fixture[0]['fixture']['status'] = 'disabled'
        mutations.append(('inactive Alice fixture', inactive_fixture))
        for field, value in (('userId', 'foreign-user'), ('subject', 'bob'), ('status', 'active'), ('disabledAt', 4000)):
            changed = deepcopy(raw)
            changed[-1]['persistence'][field] = value
            mutations.append((f'persistence {field}', changed))
        wrong_path = deepcopy(raw)
        wrong_path[1]['path'] = '/users/foreign-user'
        mutations.append(('foreign disable path', wrong_path))
        wrong_principal = deepcopy(raw)
        wrong_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('foreign principal', wrong_principal))
        missing_authority = deepcopy(raw)
        missing_authority[1]['statements'].pop(0)
        mutations.append(('missing principal query', missing_authority))
        unscoped_authority = deepcopy(raw)
        unscoped_authority[1]['statements'][0]['sql'] = 'SELECT id, status FROM "user"'
        mutations.append(('unscoped principal query', unscoped_authority))
        authority_or_bypass = deepcopy(raw)
        authority_or_bypass_sql = 'SELECT id FROM "user" WHERE authentication_subject = ? OR status = \'active\''
        authority_or_bypass[1]['statements'][0]['sql'] = authority_or_bypass_sql
        self.assertRegex(authority_or_bypass_sql, r'WHERE authentication_subject = \? OR ')
        self.assertTrue(checkpoint.has_top_level_sql_keyword(authority_or_bypass_sql.split('WHERE', 1)[1], 'or'))
        mutations.append(('principal query OR bypass', authority_or_bypass))
        write_or_bypass = deepcopy(raw)
        write_or_bypass[1]['statements'][1]['sql'] = 'UPDATE "user" SET status = ? WHERE id = ? OR status = \'active\''
        mutations.append(('disable write OR bypass', write_or_bypass))
        extra_authority = deepcopy(raw)
        extra_authority[1]['statements'].append({'sql': 'SELECT id FROM "user" WHERE authentication_subject = ?', 'operation': 'SELECT'})
        mutations.append(('duplicate principal query', extra_authority))
        missing_write = deepcopy(raw)
        missing_write[1]['statements'].pop()
        mutations.append(('missing disable write', missing_write))
        wrong_status = deepcopy(raw)
        wrong_status[1]['status'] = 200
        mutations.append(('wrong success status', wrong_status))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'AUTH-010|Foreign request observation'):
                checkpoint.observations(self.cases['AUTH-010'], changed)

    def test_non_owner_patch_requires_hidden_foreign_fixture_scoped_lookup_and_zero_writes(self):
        raw = self.raw('PATCH-004')
        observed = checkpoint.observations(self.cases['PATCH-004'], raw)
        self.assertEqual(harness.assess(self.cases['PATCH-004'], observed), [])
        self.assertEqual(observed[0]['steps'][0]['ownerScopedLookup'], True)
        self.assertEqual(observed[0]['steps'][0]['persistedUnchanged'], True)
        mutations = []
        foreign_fixture = deepcopy(raw)
        foreign_fixture[0]['fixture']['ownerId'] = '00000000-0000-4000-8000-000000000001'
        mutations.append(('Alice owns fixture', foreign_fixture))
        missing_fixture = deepcopy(raw)
        missing_fixture.pop(0)
        mutations.append(('missing persisted fixture', missing_fixture))
        missing_readback = deepcopy(raw)
        missing_readback.pop()
        mutations.append(('missing persistence readback', missing_readback))
        changed_readback = deepcopy(raw)
        changed_readback[-1]['persistence']['title'] = 'Stolen'
        mutations.append(('changed foreign row', changed_readback))
        unscoped = deepcopy(raw)
        unscoped[1]['statements'][0]['sql'] = 'SELECT id, owner_id FROM "todo" WHERE id = ? AND deleted_at IS NULL'
        mutations.append(('missing owner scope', unscoped))
        or_bypass = deepcopy(raw)
        or_bypass_sql = 'SELECT id, owner_id FROM "todo" WHERE id = ? AND owner_id = ? AND deleted_at IS NULL OR status = \'open\''
        or_bypass[1]['statements'][0]['sql'] = or_bypass_sql
        self.assertRegex(or_bypass_sql, r'WHERE id = \? AND owner_id = \? AND deleted_at IS NULL OR ')
        self.assertTrue(checkpoint.has_top_level_sql_keyword(or_bypass_sql.split('WHERE', 1)[1], 'or'))
        mutations.append(('top-level owner OR bypass', or_bypass))
        wrote = deepcopy(raw)
        wrote[1]['statements'].append({'sql': 'UPDATE "todo" SET title = ? WHERE id = ?', 'operation': 'UPDATE'})
        mutations.append(('unexpected write', wrote))
        wrong_principal = deepcopy(raw)
        wrong_principal[1]['principal']['identityId'] = '00000000-0000-4000-8000-000000000002'
        mutations.append(('foreign principal', wrong_principal))
        private_output = deepcopy(raw)
        private_output[1]['body']['todo'] = 'private bob todo'
        mutations.append(('private row disclosed', private_output))
        wrong_code = deepcopy(raw)
        wrong_code[1]['body']['error']['code'] = 'not_found'
        mutations.append(('wrong concealed error', wrong_code))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'PATCH-004|Foreign request observation'):
                checkpoint.observations(self.cases['PATCH-004'], changed)

    def test_soft_delete_requires_persisted_timestamp_scoped_write_and_hidden_followup(self):
        raw = self.raw('DELETE-001')
        observed = checkpoint.observations(self.cases['DELETE-001'], raw)
        self.assertEqual(harness.assess(self.cases['DELETE-001'], observed), [])
        step = observed[0]['steps'][0]
        self.assertEqual(step['rowExists'], True)
        self.assertEqual(step['deletedAt'], '2026-01-15T12:00:00Z')
        self.assertEqual(step['subsequentGetStatus'], 404)
        mutations = []
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing active fixture', missing_fixture))
        wrong_owner = deepcopy(raw); wrong_owner[0]['fixture']['ownerId'] = 'foreign-user'
        mutations.append(('foreign fixture owner', wrong_owner))
        missing_readback = deepcopy(raw); missing_readback.pop(2)
        mutations.append(('missing persistence readback', missing_readback))
        wrong_timestamp = deepcopy(raw); wrong_timestamp[2]['persistence']['deletedAt'] = '2026-01-15T12:00:01.000Z'
        mutations.append(('wrong persisted deletion time', wrong_timestamp))
        unscoped_delete = deepcopy(raw)
        unscoped_delete[1]['statements'][1]['sql'] = 'UPDATE "todo" SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL'
        mutations.append(('missing owner scope on delete', unscoped_delete))
        delete_or_bypass = deepcopy(raw)
        delete_sql = 'UPDATE "todo" SET deleted_at = ? WHERE id = ? AND owner_id = ? AND deleted_at IS NULL OR status = \'open\''
        delete_or_bypass[1]['statements'][1]['sql'] = delete_sql
        self.assertRegex(delete_sql, r'WHERE id = \? AND owner_id = \? AND deleted_at IS NULL OR ')
        self.assertTrue(checkpoint.has_top_level_sql_keyword(delete_sql.split('WHERE', 1)[1], 'or'))
        mutations.append(('top-level OR on delete', delete_or_bypass))
        unexpected_write = deepcopy(raw)
        unexpected_write[1]['statements'].append({'sql': 'DELETE FROM "todo" WHERE id = ?', 'operation': 'DELETE'})
        mutations.append(('unexpected delete write', unexpected_write))
        visible_get = deepcopy(raw); visible_get[3]['status'] = 200
        mutations.append(('followup GET remains visible', visible_get))
        wrong_get_error = deepcopy(raw); wrong_get_error[3]['body']['error']['code'] = 'not_found'
        mutations.append(('wrong hidden GET error', wrong_get_error))
        unscoped_get = deepcopy(raw)
        unscoped_get[3]['statements'][0]['sql'] = 'SELECT id FROM "todo" WHERE id = ? AND deleted_at IS NULL'
        mutations.append(('missing owner scope on followup GET', unscoped_get))
        get_or_bypass = deepcopy(raw)
        get_sql = 'SELECT id FROM "todo" WHERE id = ? AND owner_id = ? AND deleted_at IS NULL OR status = \'open\''
        get_or_bypass[3]['statements'][0]['sql'] = get_sql
        self.assertRegex(get_sql, r'WHERE id = \? AND owner_id = \? AND deleted_at IS NULL OR ')
        self.assertTrue(checkpoint.has_top_level_sql_keyword(get_sql.split('WHERE', 1)[1], 'or'))
        mutations.append(('top-level OR on followup GET', get_or_bypass))
        wrong_principal = deepcopy(raw); wrong_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('wrong delete principal', wrong_principal))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'DELETE-001|Foreign request observation'):
                checkpoint.observations(self.cases['DELETE-001'], changed)

    def test_repeated_delete_conceals_deleted_fixture_and_performs_no_write(self):
        raw = self.raw('DELETE-002')
        observed = checkpoint.observations(self.cases['DELETE-002'], raw)
        self.assertEqual(harness.assess(self.cases['DELETE-002'], observed), [])
        self.assertEqual(observed[0]['steps'][0]['fixtureStatus'], 'deleted')
        self.assertEqual(observed[0]['steps'][0]['ownerScopedLookup'], True)
        self.assertEqual(observed[0]['steps'][0]['persistedUnchanged'], True)
        mutations = []
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing deleted fixture', missing_fixture))
        active_fixture = deepcopy(raw); active_fixture[0]['fixture']['deletedAt'] = None
        mutations.append(('fixture is active', active_fixture))
        foreign_fixture = deepcopy(raw); foreign_fixture[0]['fixture']['ownerId'] = 'foreign-user'
        mutations.append(('foreign fixture owner', foreign_fixture))
        missing_readback = deepcopy(raw); missing_readback.pop()
        mutations.append(('missing unchanged readback', missing_readback))
        changed_readback = deepcopy(raw); changed_readback[-1]['persistence']['updatedAt'] = '2026-01-15T11:59:01.000Z'
        mutations.append(('changed deleted row', changed_readback))
        visible = deepcopy(raw); visible[1]['status'] = 204
        mutations.append(('repeated delete accepted', visible))
        wrong_error = deepcopy(raw); wrong_error[1]['body']['error']['code'] = 'not_found'
        mutations.append(('wrong concealment error', wrong_error))
        unscoped = deepcopy(raw)
        unscoped[1]['statements'][0]['sql'] = 'SELECT id, owner_id FROM "todo" WHERE id = ? AND deleted_at IS NULL'
        mutations.append(('missing owner scope', unscoped))
        or_bypass = deepcopy(raw)
        sql = "SELECT id, owner_id FROM \"todo\" WHERE id = ? AND owner_id = ? AND deleted_at IS NULL OR status = 'open'"
        or_bypass[1]['statements'][0]['sql'] = sql
        mutations.append(('top-level owner OR bypass', or_bypass))
        unexpected_write = deepcopy(raw)
        unexpected_write[1]['statements'].append({'sql': 'UPDATE "todo" SET deleted_at = ? WHERE id = ?', 'operation': 'UPDATE'})
        mutations.append(('unexpected write', unexpected_write))
        wrong_principal = deepcopy(raw); wrong_principal[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('wrong principal', wrong_principal))
        disclosed_title = deepcopy(raw); disclosed_title[1]['body']['todo'] = 'previously deleted todo'
        mutations.append(('prior title disclosed', disclosed_title))
        disclosed_owner = deepcopy(raw); disclosed_owner[1]['body']['ownerId'] = '00000000-0000-4000-8000-000000000001'
        mutations.append(('owner identity disclosed', disclosed_owner))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'DELETE-002|Foreign request observation'):
                checkpoint.observations(self.cases['DELETE-002'], changed)

    def test_user001_requires_self_disable_refresh_rejection_and_exact_session_expiry(self):
        case = self.cases['USER-001']
        raw = self.raw('USER-001')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual(observed[0]['steps'], [
            {'status': 204, 'userStatus': 'disabled', 'todoRowExists': True},
            {'status': 403, 'code': 'user_disabled', 'writes': 0},
            {'failure': 'UserDisabled', 'code': 'user_disabled', 'credentialsIssued': 0},
            {'status': 401, 'code': 'invalid_credentials', 'writes': 0},
        ])
        mutations = []
        changed_time = deepcopy(raw); changed_time[1]['requestStartedAt'] += 1
        mutations.append(('disable outside fixed timeline', changed_time))
        wrong_identity = deepcopy(raw); wrong_identity[1]['principal']['identityId'] = 'foreign-user'
        mutations.append(('foreign authenticated principal', wrong_identity))
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing active user/Todo/session fixture', missing_fixture))
        stale_todo = deepcopy(raw); stale_todo[2]['persistence']['todo']['deletedAt'] = '2026-01-15T12:00:00.000Z'
        mutations.append(('self-disable changed retained Todo', stale_todo))
        missing_user_scope = deepcopy(raw)
        missing_user_scope[1]['statements'][3]['sql'] = 'SELECT "id" FROM "user" WHERE "status" = \'<literal>\' LIMIT 2'
        mutations.append(('disable lookup lost identity predicate', missing_user_scope))
        lookup_or = deepcopy(raw)
        lookup_or[1]['statements'][3]['sql'] = 'SELECT "id" FROM "user" WHERE "id" = ? AND "status" = \'<literal>\' OR 1 = 1'
        mutations.append(('disable lookup top-level OR bypass', lookup_or))
        update_or = deepcopy(raw)
        update_or[1]['statements'][4]['sql'] = 'UPDATE "user" SET "status" = ? WHERE "id" = ? AND "status" = ? OR 1 = 1'
        mutations.append(('disable update top-level OR bypass', update_or))
        no_commit = deepcopy(raw); no_commit[1]['statements'][-1]['operation'] = 'ROLLBACK'
        mutations.append(('disable transaction did not commit', no_commit))
        repeated_visible = deepcopy(raw); repeated_visible[3]['status'] = 204
        mutations.append(('repeated disable accepted', repeated_visible))
        repeated_write = deepcopy(raw)
        repeated_write[3]['statements'].append({'sql': 'UPDATE "user" SET status = ? WHERE id = ?', 'operation': 'UPDATE'})
        mutations.append(('repeated disable wrote data', repeated_write))
        refresh_succeeded = deepcopy(raw); refresh_succeeded[4]['refresh']['outcome'] = 'returned'
        mutations.append(('compiler refresh succeeded', refresh_succeeded))
        refresh_credential = deepcopy(raw); refresh_credential[4]['refresh']['credentialReturned'] = True
        mutations.append(('compiler refresh issued credential', refresh_credential))
        refresh_mapping = deepcopy(raw); refresh_mapping[4]['refresh']['publicFailureCode'] = 'principal_inactive'
        mutations.append(('wrong compiler public failure mapping', refresh_mapping))
        refresh_scope = deepcopy(raw); refresh_scope[4]['refresh']['statements'][1]['sql'] = 'SELECT id FROM "user" WHERE "authentication_subject" = ?'
        mutations.append(('refresh skipped live active-user check', refresh_scope))
        expiry_early = deepcopy(raw); expiry_early[5]['requestStartedAt'] -= 1
        mutations.append(('expiry request arrived early', expiry_early))
        expiry_accepted = deepcopy(raw); expiry_accepted[5]['status'] = 200
        mutations.append(('expired session accepted', expiry_accepted))
        changed_final = deepcopy(raw); changed_final[6]['persistence']['user']['status'] = 'active'
        mutations.append(('later step changed disabled user', changed_final))
        changed_contract = deepcopy(case); changed_contract['timeline']['maximumRevocationDelaySeconds'] = 301
        with self.assertRaisesRegex(ValueError, 'USER-001'):
            checkpoint.observations(changed_contract, raw)
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'USER-001|Foreign request observation'):
                checkpoint.observations(case, changed)

    def test_list001_requires_owner_scope_deleted_exclusion_stable_order_and_cursor(self):
        case = self.cases['LIST-001']
        raw = self.raw('LIST-001')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual(observed[0]['steps'], [{
            'status': 200, 'databaseQueries': 1, 'totalDatabaseStatements': 1,
            'authenticationAttempts': 1, 'writes': 0, 'onlyOwner': 'alice',
            'deletedAbsent': True, 'order': ['created_at desc', 'id desc'],
            'nextCursorPresent': True,
        }])
        mutations = []
        foreign_row = deepcopy(raw); foreign_row[1]['body']['items'][0]['id'] = '00000000-0000-4000-8000-000000000034'
        mutations.append(('foreign owner leaked', foreign_row))
        deleted_row = deepcopy(raw); deleted_row[1]['body']['items'][0]['id'] = '00000000-0000-4000-8000-000000000099'
        mutations.append(('soft-deleted row leaked', deleted_row))
        unstable = deepcopy(raw); unstable[1]['body']['items'].reverse()
        mutations.append(('tie ordering reversed', unstable))
        missing_cursor = deepcopy(raw); missing_cursor[1]['body']['next'] = None
        mutations.append(('missing continuation cursor', missing_cursor))
        wrong_cursor = deepcopy(raw); wrong_cursor[1]['body']['next']['id'] = '00000000-0000-4000-8000-000000000033'
        mutations.append(('cursor does not identify last item', wrong_cursor))
        no_owner = deepcopy(raw); no_owner[1]['statements'][0]['sql'] = 'SELECT "id" FROM "todo" WHERE "deleted_at" IS NULL ORDER BY "created_at" DESC, "id" DESC LIMIT ?'
        mutations.append(('SQL owner predicate absent', no_owner))
        owner_or = deepcopy(raw); owner_or[1]['statements'][0]['sql'] = 'SELECT "id" FROM "todo" WHERE "owner_id" = ? AND "deleted_at" IS NULL OR "status" = ? ORDER BY "created_at" DESC, "id" DESC LIMIT ?'
        mutations.append(('top-level owner OR bypass', owner_or))
        no_deleted_predicate = deepcopy(raw); no_deleted_predicate[1]['statements'][0]['sql'] = 'SELECT "id" FROM "todo" WHERE "owner_id" = ? ORDER BY "created_at" DESC, "id" DESC LIMIT ?'
        mutations.append(('SQL deleted filter absent', no_deleted_predicate))
        duplicate_query = deepcopy(raw); duplicate_query[1]['statements'].append(deepcopy(duplicate_query[1]['statements'][0]))
        mutations.append(('more than one Todo query', duplicate_query))
        changed_fixture = deepcopy(raw); changed_fixture[0]['fixture']['rows'][0]['ownerId'] = '00000000-0000-4000-8000-000000000002'
        mutations.append(('foreign persisted fixture row', changed_fixture))
        wrong_principal = deepcopy(raw); wrong_principal[1]['principal']['subject'] = 'bob'
        mutations.append(('wrong authenticated owner', wrong_principal))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'LIST-001|Foreign request observation'):
                checkpoint.observations(case, changed)

    def test_list002_requires_dynamic_cursor_traversal_without_gaps_or_duplicates(self):
        case = self.cases['LIST-002']
        raw = self.raw('LIST-002')
        observations = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observations), [])
        self.assertEqual(observations[0]['steps'], [{'status': 200, 'uniqueItemCount': 5, 'stableAcrossRepeat': True}])
        mutations = []
        missing_page = deepcopy(raw); missing_page.pop(3)
        mutations.append(('missing page', missing_page))
        duplicate = deepcopy(raw); duplicate[3]['body']['items'][0] = deepcopy(duplicate[1]['body']['items'][0])
        mutations.append(('duplicate across pages', duplicate))
        gap = deepcopy(raw); gap[2]['body']['items'][0] = deepcopy(gap[2]['body']['items'][1])
        mutations.append(('gap in traversal', gap))
        wrong_cursor_path = deepcopy(raw); wrong_cursor_path[2]['path'] = case['requests'][1]['path']
        mutations.append(('request did not use first response cursor', wrong_cursor_path))
        wrong_sql_cursor = deepcopy(raw); wrong_sql_cursor[2]['statements'][0]['sql'] = wrong_sql_cursor[2]['statements'][0]['sql'].replace('("created_at", "id") < (?, ?)', '"created_at" < ?')
        mutations.append(('continuation query omitted composite id key', wrong_sql_cursor))
        unstable_repeat = deepcopy(raw); unstable_repeat[5]['body']['items'][0]['title'] = 'changed on replay'
        mutations.append(('repeat changed page', unstable_repeat))
        missing_owner = deepcopy(raw); missing_owner[2]['statements'][0]['sql'] = missing_owner[2]['statements'][0]['sql'].replace('"owner_id" = ? AND ', '')
        mutations.append(('continuation dropped owner scope', missing_owner))
        missing_deleted = deepcopy(raw); missing_deleted[2]['statements'][0]['sql'] = missing_deleted[2]['statements'][0]['sql'].replace('"deleted_at" IS NULL AND ', '')
        mutations.append(('continuation dropped deleted filter', missing_deleted))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'LIST-002'):
                checkpoint.observations(case, changed)

    def test_list003_requires_composed_filters_and_owner_scope(self):
        case = self.cases['LIST-003']
        raw = self.raw('LIST-003')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual(observed[0]['steps'], [{
            'status': 200, 'databaseQueries': 1, 'totalDatabaseStatements': 1,
            'authenticationAttempts': 1, 'writes': 0, 'onlyOwner': 'alice',
            'onlyTodoStatus': 'open', 'allDueBefore': '2026-02-01T00:00:00Z',
            'undatedAbsent': True,
        }])
        mutations = []
        foreign_owner = deepcopy(raw); foreign_owner[1]['body']['items'][0]['id'] = '00000000-0000-4000-8000-000000000066'
        mutations.append(('Bob row leaked', foreign_owner))
        wrong_status = deepcopy(raw); wrong_status[1]['body']['items'][0]['status'] = 'closed'
        mutations.append(('closed row leaked', wrong_status))
        after_cutoff = deepcopy(raw); after_cutoff[1]['body']['items'][0]['due_at'] = '2026-02-02T00:00:00.000Z'
        mutations.append(('due date after cutoff leaked', after_cutoff))
        undated = deepcopy(raw); undated[1]['body']['items'][0]['due_at'] = None
        mutations.append(('undated row leaked', undated))
        missing_owner = deepcopy(raw); missing_owner[1]['statements'][0]['sql'] = missing_owner[1]['statements'][0]['sql'].replace('"owner_id" = ? AND ', '')
        mutations.append(('owner predicate absent', missing_owner))
        missing_status = deepcopy(raw); missing_status[1]['statements'][0]['sql'] = missing_status[1]['statements'][0]['sql'].replace(' AND "status" = ?', '')
        mutations.append(('status predicate absent', missing_status))
        missing_due = deepcopy(raw); missing_due[1]['statements'][0]['sql'] = missing_due[1]['statements'][0]['sql'].replace(' AND "due_at" <= ?', '')
        mutations.append(('due-before predicate absent', missing_due))
        missing_deleted = deepcopy(raw); missing_deleted[1]['statements'][0]['sql'] = missing_deleted[1]['statements'][0]['sql'].replace(' AND "deleted_at" IS NULL', '')
        mutations.append(('soft-delete predicate absent', missing_deleted))
        top_level_or = deepcopy(raw); top_level_or[1]['statements'][0]['sql'] = top_level_or[1]['statements'][0]['sql'].replace(' ORDER BY', ' OR 1 = 1 ORDER BY')
        mutations.append(('top-level OR bypass', top_level_or))
        unbounded = deepcopy(raw); unbounded[1]['statements'][0]['sql'] = unbounded[1]['statements'][0]['sql'].replace(' LIMIT ?', '')
        mutations.append(('unbounded query', unbounded))
        duplicate = deepcopy(raw); duplicate[1]['statements'].append(deepcopy(duplicate[1]['statements'][0]))
        mutations.append(('duplicate Todo query', duplicate))
        write = deepcopy(raw); write[1]['statements'].append({'sql': 'UPDATE todo SET status = ?', 'operation': 'UPDATE'})
        mutations.append(('unexpected write', write))
        changed_fixture = deepcopy(raw); changed_fixture[0]['fixture']['rows'][1]['ownerId'] = '00000000-0000-4000-8000-000000000002'
        mutations.append(('changed persisted owner fixture', changed_fixture))
        wrong_principal = deepcopy(raw); wrong_principal[1]['principal']['subject'] = 'bob'
        mutations.append(('wrong authenticated principal', wrong_principal))
        wrong_path = deepcopy(raw); wrong_path[1]['path'] = '/todos?status=closed&due_before=2026-02-01T00:00:00Z'
        mutations.append(('wrong frozen request', wrong_path))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'LIST-003'):
                checkpoint.observations(case, changed)

    def test_list004_rejects_page_size_before_any_database_statement(self):
        case = self.cases['LIST-004']
        raw = self.raw('LIST-004')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual(observed[0]['steps'], [{
            'status': 422, 'code': 'invalid_value', 'databaseQueries': 0,
            'totalDatabaseStatements': 0, 'authenticationAttempts': 1, 'writes': 0,
        }])
        mutations = []
        accepted = deepcopy(raw); accepted[0]['status'] = 200
        mutations.append(('oversized page accepted', accepted))
        wrong_code = deepcopy(raw); wrong_code[0]['body']['error']['code'] = 'invalid_request'
        mutations.append(('wrong public validation code', wrong_code))
        database_read = deepcopy(raw); database_read[0]['statements'].append({'sql': 'SELECT id FROM todo', 'operation': 'SELECT'})
        mutations.append(('database read before rejection', database_read))
        transaction = deepcopy(raw); transaction[0]['statements'].append({'sql': 'BEGIN', 'operation': 'BEGIN'})
        mutations.append(('transaction before rejection', transaction))
        wrong_page = deepcopy(raw); wrong_page[0]['path'] = '/todos?page_size=100'
        mutations.append(('wrong boundary request', wrong_page))
        missing_request_id = deepcopy(raw); del missing_request_id[0]['body']['error']['request_id']
        mutations.append(('missing request id', missing_request_id))
        unauthenticated = deepcopy(raw); unauthenticated[0]['authenticationAttempts'] = 0
        mutations.append(('authentication not observed', unauthenticated))
        missing_principal = deepcopy(raw); del missing_principal[0]['principal']
        mutations.append(('authenticated principal missing', missing_principal))
        foreign_principal = deepcopy(raw)
        foreign_principal[0]['principal'] = {
            'kind': 'user', 'subject': 'bob',
            'identityId': '00000000-0000-4000-8000-000000000002'}
        mutations.append(('authenticated principal belongs to Bob', foreign_principal))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'LIST-004'):
                checkpoint.observations(case, changed)

    def test_rel001_covers_zero_one_and_hundred_child_variants(self):
        case = self.cases['REL-001']
        raw = self.raw('REL-001')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual([scenario['variant_index'] for scenario in observed], [0, 1, 2])
        self.assertEqual([scenario['steps'][0]['databaseQueries'] for scenario in observed], [2, 2, 2])

        mutations = []
        reordered = deepcopy(raw); reordered[0:2], reordered[2:4] = reordered[2:4], reordered[0:2]
        mutations.append(('reordered fixture variants', reordered))
        missing = deepcopy(raw); del missing[1]
        mutations.append(('missing variant response', missing))
        missing_principal = deepcopy(raw); del missing_principal[1]['principal']
        mutations.append(('missing Alice principal', missing_principal))
        bob_principal = deepcopy(raw); bob_principal[3]['principal'] = {
            'kind': 'user', 'subject': 'bob', 'identityId': '00000000-0000-4000-8000-000000000002'}
        mutations.append(('Bob principal', bob_principal))
        email = deepcopy(raw); email[5]['body']['email'] = 'alice@example.test'
        mutations.append(('private email output', email))
        unexpected_parent_field = deepcopy(raw); unexpected_parent_field[1]['body']['authentication_subject'] = 'alice'
        mutations.append(('unexpected parent response field', unexpected_parent_field))
        foreign = deepcopy(raw); foreign_todo = deepcopy(foreign[0]['fixture']['rows'][-1])
        foreign[1]['body']['todos'].append({'id': foreign_todo['id'], 'title': foreign_todo['title'],
            'status': foreign_todo['status'], 'due_at': None, 'created_at': foreign_todo['createdAt'],
            'updated_at': foreign_todo['updatedAt']})
        mutations.append(('Bob todo mixed into Alice response', foreign))
        deleted = deepcopy(raw); deleted_todo = deleted[4]['fixture']['rows'][100]
        deleted[5]['body']['todos'].append({'id': deleted_todo['id'], 'title': deleted_todo['title'],
            'status': deleted_todo['status'], 'due_at': None, 'created_at': deleted_todo['createdAt'],
            'updated_at': deleted_todo['updatedAt']})
        mutations.append(('deleted Alice todo returned', deleted))
        reversed_rows = deepcopy(raw); reversed_rows[5]['body']['todos'].reverse()
        mutations.append(('child order changed', reversed_rows))
        no_limit = deepcopy(raw); no_limit[5]['statements'][1]['sql'] = no_limit[5]['statements'][1]['sql'].replace(' LIMIT ?4', '')
        mutations.append(('child SQL unbounded', no_limit))
        wrong_limit = deepcopy(raw); wrong_limit[5]['todoLimitBindings'] = [201]
        mutations.append(('child SQL bound exceeds cap and lookahead', wrong_limit))
        missing_limit_binding = deepcopy(raw); del missing_limit_binding[5]['todoLimitBindings']
        mutations.append(('child SQL bound not observed', missing_limit_binding))
        no_order = deepcopy(raw); no_order[5]['statements'][1]['sql'] = no_order[5]['statements'][1]['sql'].replace(' ORDER BY "created_at" ASC, "id" ASC', '')
        mutations.append(('child SQL order missing', no_order))
        extra_query = deepcopy(raw); extra_query[3]['statements'].append({'sql': 'SELECT id FROM todo', 'operation': 'SELECT'})
        mutations.append(('more than two database queries', extra_query))
        overfull_fixture = deepcopy(raw); overfull_fixture[4]['fixture']['rows'].append({
            'id': '00000000-0000-4000-8000-000000000999', 'ownerId': '00000000-0000-4000-8000-000000000001',
            'title': 'extra Alice todo', 'status': 'open', 'dueAt': None,
            'createdAt': '2026-01-15T11:00:00.000Z', 'updatedAt': '2026-01-15T11:00:00.000Z', 'deletedAt': None})
        mutations.append(('fixture exceeds frozen child count', overfull_fixture))
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'REL-001'):
                checkpoint.observations(case, changed)

    def test_rel002_missing_owner_insert_normalises_and_rolls_back(self):
        case = self.cases['REL-002']
        for backend in harness.BACKENDS:
            with self.subTest(backend=backend):
                raw = self.raw('REL-002', backend)
                observed = checkpoint.observations(case, raw, backend=backend)
                self.assertEqual(harness.assess(case, observed), [])
                self.assertEqual(observed[0]['steps'][0], {
                    'accepted': False, 'normalisedFailure': 'relationship_conflict',
                    'rawDatabaseDetailsPublic': False,
                })

                mutations = []
                accepted = deepcopy(raw); accepted[1]['persistenceAttempt']['accepted'] = True
                mutations.append(('orphan insert accepted', accepted))
                wrong_fault = deepcopy(raw); wrong_fault[1]['persistenceAttempt']['faultKind'] = 'driver'
                mutations.append(('wrong persistence fault kind', wrong_fault))
                wrong_failure = deepcopy(raw); wrong_failure[1]['persistenceAttempt']['normalisedFailure'] = 'todo_mutation_conflict'
                mutations.append(('wrong normalized relationship failure', wrong_failure))
                leaked_details = deepcopy(raw); leaked_details[1]['persistenceAttempt']['driverError'] = 'FOREIGN KEY constraint failed'
                mutations.append(('raw database error leaked', leaked_details))
                missing_rollback = deepcopy(raw); missing_rollback[1]['persistenceAttempt']['statements'].pop()
                mutations.append(('failed insert did not roll back', missing_rollback))
                wrong_insert = deepcopy(raw); wrong_insert[1]['persistenceAttempt']['statements'][1]['sql'] = 'INSERT INTO "todo" ("id", "title") VALUES (?, ?)'
                mutations.append(('attempt was not the generated owner-bearing Todo insert', wrong_insert))
                orphan_retained = deepcopy(raw); orphan_retained[2]['readback']['orphanRows'] = [{'id': '00000000-0000-4000-8000-000000000099', 'owner_id': '00000000-0000-4000-8000-000000009999'}]
                mutations.append(('orphan row persisted', orphan_retained))
                changed_control = deepcopy(raw); changed_control[2]['readback']['aliceControlTodos'][0]['title'] = 'changed'
                mutations.append(('Alice control changed', changed_control))
                owner_exists = deepcopy(raw); owner_exists[0]['fixture']['missingOwnerRowsBefore'] = [{'id': '00000000-0000-4000-8000-000000009999'}]
                mutations.append(('missing owner was present', owner_exists))
                wrong_backend = deepcopy(raw)
                wrong_backend[1]['persistenceAttempt']['backend'] = 'postgres' if backend == 'sqlite' else 'sqlite'
                mutations.append(('backend label contradicts output backend', wrong_backend))
                for name, changed in mutations:
                    with self.subTest(backend=backend, mutation=name), self.assertRaisesRegex(ValueError, 'REL-002'):
                        checkpoint.observations(case, changed, backend=backend)

    def test_rel004_restricts_hard_delete_and_retains_disabled_user_and_todo(self):
        case = self.cases['REL-004']
        for backend in harness.BACKENDS:
            with self.subTest(backend=backend):
                raw = self.raw('REL-004', backend)
                observed = checkpoint.observations(case, raw, backend=backend)
                self.assertEqual(harness.assess(case, observed), [])
                self.assertEqual(observed[0]['steps'][0], {
                    'accepted': False, 'normalisedFailure': 'relationship_conflict',
                    'todoRowExists': True, 'rawDatabaseDetailsPublic': False,
                })

                mutations = []
                accepted = deepcopy(raw); accepted[1]['persistenceAttempt']['accepted'] = True
                mutations.append(('hard delete accepted', accepted))
                wrong_fault = deepcopy(raw); wrong_fault[1]['persistenceAttempt']['faultKind'] = 'driver'
                mutations.append(('wrong persistence fault kind', wrong_fault))
                wrong_constraint = deepcopy(raw); wrong_constraint[1]['persistenceAttempt']['relationshipConstraintMatched'] = False
                mutations.append(('expected owner constraint not matched', wrong_constraint))
                wrong_failure = deepcopy(raw); wrong_failure[1]['persistenceAttempt']['normalisedFailure'] = 'todo_mutation_conflict'
                mutations.append(('wrong normalized relationship failure', wrong_failure))
                leaked_details = deepcopy(raw); leaked_details[1]['persistenceAttempt']['errno'] = 1811
                mutations.append(('raw database code leaked', leaked_details))
                missing_rollback = deepcopy(raw); missing_rollback[1]['persistenceAttempt']['statements'].pop()
                mutations.append(('failed delete did not roll back', missing_rollback))
                wrong_delete = deepcopy(raw); wrong_delete[1]['persistenceAttempt']['statements'][1]['sql'] = 'DELETE FROM "todo" WHERE "owner_id" = ?1'
                mutations.append(('attempt was not the frozen user delete', wrong_delete))
                referencing_service = deepcopy(raw); referencing_service[0]['fixture']['referencingServicesBefore'] = [{'id': 'service'}]
                mutations.append(('fixture has a competing referencing service', referencing_service))
                lost_todo = deepcopy(raw); lost_todo[2]['readback']['retainedTodos'] = []
                mutations.append(('retained Todo missing after rollback', lost_todo))
                changed_user = deepcopy(raw); changed_user[2]['readback']['user']['status'] = 'active'
                mutations.append(('disabled Alice changed after rollback', changed_user))
                wrong_backend = deepcopy(raw)
                wrong_backend[1]['persistenceAttempt']['backend'] = 'postgres' if backend == 'sqlite' else 'sqlite'
                mutations.append(('backend label contradicts output backend', wrong_backend))
                for name, changed in mutations:
                    with self.subTest(backend=backend, mutation=name), self.assertRaisesRegex(ValueError, 'REL-004'):
                        checkpoint.observations(case, changed, backend=backend)

    def test_patch001_preserves_omitted_due_date_and_reminder(self):
        case = self.cases['PATCH-001']
        raw = self.raw('PATCH-001')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual(observed[0]['steps'][0]['storedDueAt'], '2026-02-01T00:00:00Z')
        self.assertTrue(observed[0]['steps'][0]['dueDatePreserved'])
        self.assertTrue(observed[0]['steps'][0]['reminderStatePreserved'])

        mutations = []
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing persisted fixture', missing_fixture))
        reordered = deepcopy(raw); reordered[0], reordered[2] = reordered[2], reordered[0]
        mutations.append(('reordered fixture and persistence', reordered))
        foreign_fixture = deepcopy(raw); foreign_fixture[0]['fixture']['ownerId'] = '00000000-0000-4000-8000-000000000002'
        mutations.append(('Bob-owned fixture', foreign_fixture))
        bob_principal = deepcopy(raw); bob_principal[1]['principal']['subject'] = 'bob'
        mutations.append(('Bob principal', bob_principal))
        due_cleared = deepcopy(raw); due_cleared[2]['persistence']['dueAt'] = None
        mutations.append(('omitted due date cleared in storage', due_cleared))
        reminder_cleared = deepcopy(raw); reminder_cleared[2]['persistence']['reminderSentAt'] = None
        mutations.append(('omitted due date reset reminder', reminder_cleared))
        wrong_title = deepcopy(raw); wrong_title[2]['persistence']['title'] = 'alice todo t1'
        mutations.append(('title not persisted', wrong_title))
        missing_persistence = deepcopy(raw); missing_persistence.pop(2)
        mutations.append(('missing persisted readback', missing_persistence))
        wrong_body_due = deepcopy(raw); wrong_body_due[1]['body']['due_at'] = None
        mutations.append(('response due date changed', wrong_body_due))
        private_output = deepcopy(raw); private_output[1]['body']['owner_id'] = '00000000-0000-4000-8000-000000000001'
        mutations.append(('private response field', private_output))
        due_written = deepcopy(raw); due_written[1]['statements'][3]['sql'] = due_written[1]['statements'][3]['sql'].replace('ELSE "due_at" END', 'ELSE NULL END')
        mutations.append(('SQL does not preserve omitted due field', due_written))
        mismatched_presence = deepcopy(raw); mismatched_presence[1]['statements'][3]['sql'] = mismatched_presence[1]['statements'][3]['sql'].replace('"reminder_sent_at" = CASE WHEN ?5', '"reminder_sent_at" = CASE WHEN ?6')
        mutations.append(('due and reminder use different presence flags', mismatched_presence))
        missing_owner = deepcopy(raw); missing_owner[1]['statements'][3]['sql'] = missing_owner[1]['statements'][3]['sql'].replace(' AND (("todo"."owner_id" = ?9))', '')
        mutations.append(('update owner predicate absent', missing_owner))
        missing_deleted = deepcopy(raw); missing_deleted[1]['statements'][3]['sql'] = missing_deleted[1]['statements'][3]['sql'].replace(' AND ("deleted_at" IS NULL)', '')
        mutations.append(('update deleted predicate absent', missing_deleted))
        top_level_or = deepcopy(raw); top_level_or[1]['statements'][3]['sql'] += ' OR 1 = 1'
        mutations.append(('update top-level OR bypass', top_level_or))
        extra_write = deepcopy(raw); extra_write[1]['statements'].append({'sql': 'UPDATE todo SET status = ?', 'operation': 'UPDATE'})
        mutations.append(('extra database write', extra_write))
        wrong_path = deepcopy(raw); wrong_path[1]['path'] = '/todos/00000000-0000-4000-8000-000000000099'
        mutations.append(('wrong request path', wrong_path))
        wrong_status = deepcopy(raw); wrong_status[1]['status'] = 204
        mutations.append(('wrong response status', wrong_status))
        begin_after_updates = deepcopy(raw)
        statements = begin_after_updates[1]['statements']
        statements[:] = statements[1:5] + [statements[0], statements[5]]
        mutations.append(('BEGIN after todo reads and updates', begin_after_updates))
        commit_before_todo_operations = deepcopy(raw)
        statements = commit_before_todo_operations[1]['statements']
        statements[:] = [statements[0], statements[5], *statements[1:5]]
        mutations.append(('COMMIT before todo reads and updates', commit_before_todo_operations))
        self.assertEqual(len(mutations), 20)
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'PATCH-001|Foreign request observation'):
                checkpoint.observations(case, changed)

    def test_patch002_clears_explicit_none_due_date(self):
        case = self.cases['PATCH-002']
        raw = self.raw('PATCH-002')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertIsNone(observed[0]['steps'][0]['storedDueAt'])
        self.assertTrue(observed[0]['steps'][0]['dueDateCleared'])
        self.assertTrue(observed[0]['steps'][0]['reminderStateReset'])

        mutations = []
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing persisted fixture', missing_fixture))
        reordered = deepcopy(raw); reordered[0], reordered[2] = reordered[2], reordered[0]
        mutations.append(('reordered fixture and persistence', reordered))
        foreign_fixture = deepcopy(raw); foreign_fixture[0]['fixture']['ownerId'] = '00000000-0000-4000-8000-000000000002'
        mutations.append(('Bob-owned fixture', foreign_fixture))
        bob_principal = deepcopy(raw); bob_principal[1]['principal']['subject'] = 'bob'
        mutations.append(('Bob principal', bob_principal))
        due_retained = deepcopy(raw); due_retained[2]['persistence']['dueAt'] = '2026-02-01T00:00:00.000Z'
        mutations.append(('explicit none retained due date', due_retained))
        reminder_retained = deepcopy(raw); reminder_retained[2]['persistence']['reminderSentAt'] = '2026-01-20T00:00:00.000Z'
        mutations.append(('due clear failed to reset reminder', reminder_retained))
        changed_title = deepcopy(raw); changed_title[2]['persistence']['title'] = 'Changed'
        mutations.append(('unrequested title changed', changed_title))
        missing_persistence = deepcopy(raw); missing_persistence.pop(2)
        mutations.append(('missing persisted readback', missing_persistence))
        missing_due_key = deepcopy(raw); del missing_due_key[2]['persistence']['dueAt']
        mutations.append(('missing persisted dueAt key', missing_due_key))
        missing_reminder_key = deepcopy(raw); del missing_reminder_key[2]['persistence']['reminderSentAt']
        mutations.append(('missing persisted reminderSentAt key', missing_reminder_key))
        missing_deleted_key = deepcopy(raw); del missing_deleted_key[2]['persistence']['deletedAt']
        mutations.append(('missing persisted deletedAt key', missing_deleted_key))
        wrong_body_due = deepcopy(raw); wrong_body_due[1]['body']['due_at'] = '2026-02-01T00:00:00Z'
        mutations.append(('response due date retained', wrong_body_due))
        private_output = deepcopy(raw); private_output[1]['body']['owner_id'] = '00000000-0000-4000-8000-000000000001'
        mutations.append(('private response field', private_output))
        due_not_cleared = deepcopy(raw); due_not_cleared[1]['statements'][3]['sql'] = due_not_cleared[1]['statements'][3]['sql'].replace('ELSE "due_at" END', 'ELSE NULL END')
        mutations.append(('SQL does not clear due field', due_not_cleared))
        title_not_preserved = deepcopy(raw); title_not_preserved[1]['statements'][3]['sql'] = title_not_preserved[1]['statements'][3]['sql'].replace('ELSE "title" END', 'ELSE NULL END')
        mutations.append(('SQL does not preserve omitted title', title_not_preserved))
        status_not_preserved = deepcopy(raw); status_not_preserved[1]['statements'][3]['sql'] = status_not_preserved[1]['statements'][3]['sql'].replace('ELSE "status" END', 'ELSE NULL END')
        mutations.append(('SQL does not preserve omitted status', status_not_preserved))
        mismatched_presence = deepcopy(raw); mismatched_presence[1]['statements'][3]['sql'] = mismatched_presence[1]['statements'][3]['sql'].replace('"reminder_sent_at" = CASE WHEN ?5', '"reminder_sent_at" = CASE WHEN ?6')
        mutations.append(('due and reminder use different presence flags', mismatched_presence))
        missing_owner = deepcopy(raw); missing_owner[1]['statements'][3]['sql'] = missing_owner[1]['statements'][3]['sql'].replace(' AND (("todo"."owner_id" = ?9))', '')
        mutations.append(('update owner predicate absent', missing_owner))
        missing_deleted = deepcopy(raw); missing_deleted[1]['statements'][3]['sql'] = missing_deleted[1]['statements'][3]['sql'].replace(' AND ("deleted_at" IS NULL)', '')
        mutations.append(('update deleted predicate absent', missing_deleted))
        top_level_or = deepcopy(raw); top_level_or[1]['statements'][3]['sql'] += ' OR 1 = 1'
        mutations.append(('update top-level OR bypass', top_level_or))
        extra_write = deepcopy(raw); extra_write[1]['statements'].append({'sql': 'UPDATE todo SET status = ?', 'operation': 'UPDATE'})
        mutations.append(('extra database write', extra_write))
        wrong_path = deepcopy(raw); wrong_path[1]['path'] = '/todos/00000000-0000-4000-8000-000000000099'
        mutations.append(('wrong request path', wrong_path))
        wrong_status = deepcopy(raw); wrong_status[1]['status'] = 204
        mutations.append(('wrong response status', wrong_status))
        begin_after_updates = deepcopy(raw)
        statements = begin_after_updates[1]['statements']
        statements[:] = statements[1:5] + [statements[0], statements[5]]
        mutations.append(('BEGIN after Todo reads and updates', begin_after_updates))
        commit_before_todo_operations = deepcopy(raw)
        statements = commit_before_todo_operations[1]['statements']
        statements[:] = [statements[0], statements[5], *statements[1:5]]
        mutations.append(('COMMIT before Todo reads and updates', commit_before_todo_operations))
        self.assertEqual(len(mutations), 25)
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'PATCH-002|Foreign request observation'):
                checkpoint.observations(case, changed)

    def test_patch005_due_date_change_resets_reminder(self):
        case = self.cases['PATCH-005']
        raw = self.raw('PATCH-005')
        observed = checkpoint.observations(case, raw)
        self.assertEqual(harness.assess(case, observed), [])
        self.assertEqual(observed[0]['steps'][0]['storedDueAt'], '2026-02-15T09:00:00Z')
        self.assertIsNone(observed[0]['steps'][0]['storedReminderSentAt'])
        self.assertTrue(observed[0]['steps'][0]['reminderStateReset'])

        mutations = []
        missing_fixture = deepcopy(raw); missing_fixture.pop(0)
        mutations.append(('missing persisted fixture', missing_fixture))
        reordered = deepcopy(raw); reordered[0], reordered[2] = reordered[2], reordered[0]
        mutations.append(('reordered fixture and persistence', reordered))
        foreign_fixture = deepcopy(raw); foreign_fixture[0]['fixture']['ownerId'] = '00000000-0000-4000-8000-000000000002'
        mutations.append(('Bob-owned fixture', foreign_fixture))
        bob_principal = deepcopy(raw); bob_principal[1]['principal']['subject'] = 'bob'
        mutations.append(('Bob principal', bob_principal))
        due_not_changed = deepcopy(raw); due_not_changed[2]['persistence']['dueAt'] = '2026-02-01T00:00:00.000Z'
        mutations.append(('requested due date was not stored', due_not_changed))
        reminder_retained = deepcopy(raw); reminder_retained[2]['persistence']['reminderSentAt'] = '2026-01-20T00:00:00.000Z'
        mutations.append(('prior reminder was not reset', reminder_retained))
        changed_title = deepcopy(raw); changed_title[2]['persistence']['title'] = 'Changed'
        mutations.append(('unrequested title changed', changed_title))
        missing_persistence = deepcopy(raw); missing_persistence.pop(2)
        mutations.append(('missing persisted readback', missing_persistence))
        unexpected_persisted_key = deepcopy(raw); unexpected_persisted_key[2]['persistence']['private'] = 'unexpected'
        mutations.append(('unexpected persisted field', unexpected_persisted_key))
        for field in ('dueAt', 'reminderSentAt', 'deletedAt'):
            missing = deepcopy(raw); del missing[2]['persistence'][field]
            mutations.append((f'missing persisted {field} key', missing))
        wrong_body_due = deepcopy(raw); wrong_body_due[1]['body']['due_at'] = '2026-02-01T00:00:00Z'
        mutations.append(('response returned old due date', wrong_body_due))
        private_output = deepcopy(raw); private_output[1]['body']['owner_id'] = '00000000-0000-4000-8000-000000000001'
        mutations.append(('private response field', private_output))
        due_not_conditional = deepcopy(raw); due_not_conditional[1]['statements'][3]['sql'] = due_not_conditional[1]['statements'][3]['sql'].replace('ELSE "due_at" END', 'ELSE NULL END')
        mutations.append(('due SQL does not preserve omitted field', due_not_conditional))
        title_not_preserved = deepcopy(raw); title_not_preserved[1]['statements'][3]['sql'] = title_not_preserved[1]['statements'][3]['sql'].replace('ELSE "title" END', 'ELSE NULL END')
        mutations.append(('SQL does not preserve omitted title', title_not_preserved))
        status_not_preserved = deepcopy(raw); status_not_preserved[1]['statements'][3]['sql'] = status_not_preserved[1]['statements'][3]['sql'].replace('ELSE "status" END', 'ELSE NULL END')
        mutations.append(('SQL does not preserve omitted status', status_not_preserved))
        mismatched_presence = deepcopy(raw); mismatched_presence[1]['statements'][3]['sql'] = mismatched_presence[1]['statements'][3]['sql'].replace('"reminder_sent_at" = CASE WHEN ?5', '"reminder_sent_at" = CASE WHEN ?6')
        mutations.append(('due and reminder use different presence flags', mismatched_presence))
        missing_owner = deepcopy(raw); missing_owner[1]['statements'][3]['sql'] = missing_owner[1]['statements'][3]['sql'].replace(' AND (("todo"."owner_id" = ?9))', '')
        mutations.append(('update owner predicate absent', missing_owner))
        missing_deleted = deepcopy(raw); missing_deleted[1]['statements'][3]['sql'] = missing_deleted[1]['statements'][3]['sql'].replace(' AND ("deleted_at" IS NULL)', '')
        mutations.append(('update deleted predicate absent', missing_deleted))
        top_level_or = deepcopy(raw); top_level_or[1]['statements'][3]['sql'] += ' OR 1 = 1'
        mutations.append(('update top-level OR bypass', top_level_or))
        extra_write = deepcopy(raw); extra_write[1]['statements'].append({'sql': 'UPDATE todo SET status = ?', 'operation': 'UPDATE'})
        mutations.append(('extra database write', extra_write))
        wrong_path = deepcopy(raw); wrong_path[1]['path'] = '/todos/00000000-0000-4000-8000-000000000099'
        mutations.append(('wrong request path', wrong_path))
        wrong_status = deepcopy(raw); wrong_status[1]['status'] = 204
        mutations.append(('wrong response status', wrong_status))
        wrong_request_due = deepcopy(raw); wrong_request_due[1]['requestJson']['due_at'] = '2026-02-16T09:00:00Z'
        mutations.append(('request changed to a different due date', wrong_request_due))
        shifted_clock = deepcopy(raw)
        shifted_clock[1]['requestStartedAt'] += 1000
        shifted_clock[1]['responseCompletedAt'] += 1000
        mutations.append(('request and response use a shifted clock', shifted_clock))
        updated_after_response = deepcopy(raw)
        updated_after_response[2]['persistence']['updatedAt'] = '2030-01-01T00:00:00.000Z'
        updated_after_response[1]['body']['updated_at'] = '2030-01-01T00:00:00.000Z'
        mutations.append(('persisted updatedAt advances after the response', updated_after_response))
        past_fixture_due = deepcopy(raw); past_fixture_due[1]['requestStartedAt'] = 1769990400000
        mutations.append(('fixture due date is no longer in the future', past_fixture_due))
        begin_after_updates = deepcopy(raw)
        statements = begin_after_updates[1]['statements']
        statements[:] = statements[1:5] + [statements[0], statements[5]]
        mutations.append(('BEGIN after Todo reads and updates', begin_after_updates))
        commit_before_todo_operations = deepcopy(raw)
        statements = commit_before_todo_operations[1]['statements']
        statements[:] = [statements[0], statements[5], *statements[1:5]]
        mutations.append(('COMMIT before Todo reads and updates', commit_before_todo_operations))
        self.assertEqual(len(mutations), 30)
        for name, changed in mutations:
            with self.subTest(mutation=name), self.assertRaisesRegex(ValueError, 'PATCH-005|Foreign request observation|Changed request'):
                checkpoint.observations(case, changed)

    def test_scenario_expansion_is_not_silently_partial(self):
        for field, value in (('fixtureVariants', ['first', 'second']), ('followUps', [{'expect': {'status': 200}}])):
            case = deepcopy(self.cases['AUTH-001']); case[field] = value
            with self.assertRaises(ValueError): checkpoint.observations(case, self.raw('AUTH-001'))
        with self.assertRaises(ValueError): checkpoint.observations(self.cases['USER-001'], [])

    def test_actual_adapter_and_instrumentation_are_bound_source_inputs(self):
        paths = harness.provenance(ROOT)['files']
        for name in ('golden_checkpoint.py', 'golden_http_checkpoint.ts', 'golden_checkpoint_fetch.ts', 'golden_sql_observer.ts'):
            self.assertIn('tools/'+name, paths)
        self.assertIn('tests/runtime/golden-auth-http-jwt.test.ts', paths)
        self.assertIn('tests/runtime/golden-checkpoint-fetch.test.ts', paths)
        self.assertIn('tests/validation/test_golden_checkpoint.py', paths)

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
            compatibility = evidence/'application/build/compatibility/public-failure-codes.json'
            compatibility.parent.mkdir(parents=True)
            compatibility.write_text(json.dumps({'schema_version': 1, 'current': [
                {'failure': 'UserDisabled', 'code': 'user_disabled'}]}))
            dependencies = evidence/'dependencies'; jose = dependencies/'node_modules/jose'; jose.mkdir(parents=True)
            (jose/'package.json').write_text('synthetic dependency')
            for generated, retained in (('persistence.ts', 'original-persistence.ts'), ('first-party-authentication.ts', 'original-authentication.ts')):
                (target/generated).write_text('synthetic instrumented')
                (evidence/retained).write_text('synthetic original')
            outputs = self.outputs()
            for output in outputs: (evidence/f'{output["backend"]}-raw.json').write_text(json.dumps(output))
            report = harness.run_cases(self.root); checkpoint.apply_checkpoint(report, outputs, self.root)
            original = checkpoint.hashes(evidence/'application/build')
            original['target/persistence.ts'] = harness.digest(evidence/'original-persistence.ts')
            original['target/first-party-authentication.ts'] = harness.digest(evidence/'original-authentication.ts')
            report['checkpoint'] = {'supported_ids': list(checkpoint.SUPPORTED), 'generated_original': original,
                                    'generated_instrumented': checkpoint.hashes(evidence/'application/build'),
                                    'raw_files': {f'{backend}-raw.json': harness.digest(evidence/f'{backend}-raw.json') for backend in harness.BACKENDS},
                                    'dependency_files': checkpoint.hashes(jose)}
            checkpoint.validate_checkpoint(report, evidence, dependencies, self.root)
            for path in (target/'persistence.ts', evidence/'original-authentication.ts', compatibility,
                         evidence/'sqlite-raw.json', jose/'package.json'):
                previous = path.read_bytes(); path.write_bytes(previous+b'\n')
                with self.subTest(path=path.name), self.assertRaises(ValueError): checkpoint.validate_checkpoint(report, evidence, dependencies, self.root)
                path.write_bytes(previous)
            changed = deepcopy(report)
            entry = next(entry for entry in changed['results'] if entry['id'] == 'AUTH-001' and entry['backend'] == 'sqlite')
            entry['observations'][0]['steps'][0]['writes'] = 99
            with self.assertRaisesRegex(ValueError, 'differ from raw'):
                checkpoint.validate_checkpoint(changed, evidence, dependencies, self.root)

    def test_synthetic_fixture_still_rejects_passes_without_compiler_binding(self):
        self.compiler.unlink()
        report = harness.run_cases(self.root)
        self.assertIsNone(report['provenance']['compiler_sha256'])
        self.assertTrue(all(entry['status'] == 'not_run' for entry in report['results']))
        with self.assertRaisesRegex(ValueError, 'Passing label lacks complete matching observations'):
            checkpoint.apply_checkpoint(report, self.outputs(), self.root)


if __name__ == '__main__': unittest.main()
