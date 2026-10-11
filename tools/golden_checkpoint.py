#!/usr/bin/env python3
"""Supporting migrated HTTP checkpoint; does not activate the final golden gate."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile

import golden_cases as harness

ROOT = Path(__file__).resolve().parents[1]
SUPPORTED = ('AUTH-001', 'AUTH-002', 'AUTH-003', 'AUTH-004', 'AUTH-005', 'AUTH-006', 'AUTH-007', 'AUTH-008', 'AUTH-009', 'AUTH-010', 'AUTH-011', 'AUTH-012', 'PUBLIC-001', 'CREATE-001', 'CREATE-002',
             'CREATE-003', 'CREATE-004', 'READ-001', 'READ-002', 'READ-003', 'LIST-001', 'LIST-002', 'PATCH-003', 'PATCH-004', 'DELETE-001', 'DELETE-002', 'USER-001')

TODO_VIEW_FIELDS = frozenset({'id', 'title', 'status', 'due_at', 'created_at', 'updated_at'})
TODO_PRIVATE_FIELDS = ('owner_id', 'deleted_at', 'reminder_sent_at')


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
            yield {**environment, 'DATABASE_URL': f'postgres://golden_checkpoint@127.0.0.1:{port}/golden_checkpoint'}
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


def epoch_millis(value):
    if type(value) is int:
        return value
    if isinstance(value, str):
        try:
            parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
            if parsed.tzinfo is None:
                return None
            return int(parsed.timestamp() * 1000)
        except ValueError:
            return None
    return None


PROVIDER_FIELDS = {'access_token', 'birthdate', 'email', 'email_verified', 'family_name',
                   'gender', 'given_name', 'id_token', 'locale', 'name', 'phone_number',
                   'picture', 'preferred_username', 'profile', 'provider', 'provider_claims',
                   'provider_email', 'provider_id', 'provider_subject', 'raw_claims',
                   'refresh_token', 'website', 'zoneinfo'}


def contains_provider_fields(value):
    if isinstance(value, dict):
        return any(key.lower().replace('-', '_') in PROVIDER_FIELDS or contains_provider_fields(item)
                   for key, item in value.items())
    if isinstance(value, list):
        return any(contains_provider_fields(item) for item in value)
    return False


def has_top_level_sql_keyword(sql, keyword):
    """Find a boolean keyword outside nested expressions, strings and identifiers."""
    depth = 0
    quote = None
    index = 0
    lowered = sql.lower()
    while index < len(sql):
        char = sql[index]
        if quote:
            if char == quote:
                if index + 1 < len(sql) and sql[index + 1] == quote:
                    index += 2
                    continue
                quote = None
            elif char == '\\' and quote == "'" and index + 1 < len(sql):
                index += 2
                continue
            index += 1
            continue
        if char in ('"', "'", '`'):
            quote = char
            index += 1
            continue
        if char == '(':
            depth += 1
            index += 1
            continue
        if char == ')':
            depth -= 1
            index += 1
            continue
        end = index + len(keyword)
        if (depth == 0 and lowered[index:end] == keyword
                and (index == 0 or not (lowered[index - 1].isalnum() or lowered[index - 1] == '_'))
                and (end == len(lowered) or not (lowered[end].isalnum() or lowered[end] == '_'))):
            return True
        index += 1
    return False


def contains_key(value, key):
    if isinstance(value, dict):
        return key in value or any(contains_key(item, key) for item in value.values())
    if isinstance(value, list):
        return any(contains_key(item, key) for item in value)
    return False


def list_001_observation(raw, reply):
    """Validate LIST-001 against persisted fixtures, response rows and SQL."""
    alice = '00000000-0000-4000-8000-000000000001'
    bob = '00000000-0000-4000-8000-000000000002'
    tie_time = '2026-01-15T11:00:00.000Z'
    older_time = '2026-01-15T10:00:00.000Z'
    expected_rows = [
        {'id': '00000000-0000-4000-8000-000000000031', 'ownerId': alice, 'title': 'alice tie lower id', 'status': 'open', 'createdAt': tie_time, 'deletedAt': None},
        {'id': '00000000-0000-4000-8000-000000000032', 'ownerId': alice, 'title': 'alice tie higher id', 'status': 'open', 'createdAt': tie_time, 'deletedAt': None},
        {'id': '00000000-0000-4000-8000-000000000033', 'ownerId': alice, 'title': 'alice older active', 'status': 'open', 'createdAt': older_time, 'deletedAt': None},
        {'id': '00000000-0000-4000-8000-000000000034', 'ownerId': bob, 'title': 'bob newer active', 'status': 'open', 'createdAt': tie_time, 'deletedAt': None},
        {'id': '00000000-0000-4000-8000-000000000099', 'ownerId': alice, 'title': 'alice newer deleted', 'status': 'open', 'createdAt': tie_time, 'deletedAt': tie_time},
    ]
    fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
    if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply) or fixtures[0][1] != {'rows': expected_rows}:
        raise ValueError('LIST-001 missing, reordered or changed persisted fixture rows')

    principal = reply.get('principal')
    statements = reply.get('statements', [])
    todo_selects = [row for row in statements if row['operation'] == 'SELECT'
                    and re.search(r'\bfrom\s+"?todo"?(?:\s|$)', row['sql'], re.IGNORECASE)]
    sql = todo_selects[0]['sql'] if len(todo_selects) == 1 else ''
    where = re.search(r'\bwhere\b(.*?)(?:\border\s+by\b|\blimit\b|\bfetch\b|$)',
                      sql, re.IGNORECASE | re.DOTALL)
    predicates = where.group(1) if where else ''
    owner_scoped = bool(re.search(r'"?owner_id"?\s*=', predicates, re.IGNORECASE))
    excludes_deleted = bool(re.search(r'"?deleted_at"?\s+is\s+null', predicates, re.IGNORECASE))
    order_by = re.search(r'\border\s+by\s+"?created_at"?\s+desc\s*,\s*"?id"?\s+desc\b',
                         sql, re.IGNORECASE)
    has_limit = bool(re.search(r'\blimit\s+(?:\?|\$\d+)', sql, re.IGNORECASE))
    if (reply.get('status') != 200 or reply.get('authenticationAttempts') != 1
            or not isinstance(principal, dict) or principal.get('kind') != 'user'
            or principal.get('subject') != 'alice' or principal.get('identityId') != alice
            or len(todo_selects) != 1 or not owner_scoped or not excludes_deleted
            or has_top_level_sql_keyword(predicates, 'or') or not order_by or not has_limit):
        raise ValueError('LIST-001 did not retain one bounded owner/deletion-scoped stable list query')

    body = reply.get('body')
    if not isinstance(body, dict) or set(body) != {'items', 'next'} or not isinstance(body['items'], list):
        raise ValueError('LIST-001 response is not a TodoPage')
    active_alice = [row for row in expected_rows if row['ownerId'] == alice and row['deletedAt'] is None]
    ordered = sorted(active_alice, key=lambda row: (epoch_millis(row['createdAt']), row['id']), reverse=True)
    expected_page = ordered[:2]
    expected_items = [{'id': row['id'], 'title': row['title'], 'status': row['status'], 'due_at': None,
                       'created_at': row['createdAt'], 'updated_at': row['createdAt']} for row in expected_page]
    if body['items'] != expected_items:
        raise ValueError('LIST-001 page rows violate owner scope, soft-delete exclusion or stable two-key ordering')
    for item in body['items']:
        if not isinstance(item, dict) or frozenset(item) != TODO_VIEW_FIELDS:
            raise ValueError('LIST-001 exposed fields outside TodoView')
    cursor = body['next']
    if (not isinstance(cursor, dict) or set(cursor) != {'created_at', 'id'}
            or cursor.get('id') != expected_page[-1]['id']
            or epoch_millis(cursor.get('created_at')) != epoch_millis(expected_page[-1]['createdAt'])
            or len(ordered) <= len(body['items'])):
        raise ValueError('LIST-001 missing or incorrect continuation cursor')
    writes = sum(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in statements)
    if writes:
        raise ValueError('LIST-001 list request unexpectedly wrote data')
    return {'status': reply['status'], 'databaseQueries': len(todo_selects),
            'totalDatabaseStatements': len(statements), 'authenticationAttempts': reply['authenticationAttempts'],
            'writes': writes, 'onlyOwner': 'alice', 'deletedAbsent': True,
            'order': ['created_at desc', 'id desc'], 'nextCursorPresent': True}


def list_002_observation(case, raw, replies):
    """Validate two complete cursor traversals against five persisted rows."""
    alice = '00000000-0000-4000-8000-000000000001'
    ids = [f'00000000-0000-4000-8000-0000000000{number}' for number in range(41, 46)]
    fixture_rows = [
        {'id': ids[index], 'ownerId': alice, 'title': f'cursor row {index + 1}', 'status': 'open',
         'createdAt': f'2026-01-15T{10 + index:02d}:00:00.000Z', 'deletedAt': None}
        for index in range(5)
    ]
    frozen_requests = [
        {'method': 'GET', 'path': '/todos?page_size=2', 'auth': 'alice-session'},
        {'method': 'GET', 'path': '/todos?page_size=2&after={first.next}', 'auth': 'alice-session'},
        {'method': 'GET', 'path': '/todos?page_size=2&after={second.next}', 'auth': 'alice-session'},
    ]
    if (case.get('id') != 'LIST-002' or case.get('requests') != frozen_requests
            or case.get('expect') != {'status': 200, 'uniqueItemCount': 5, 'stableAcrossRepeat': True}):
        raise ValueError('Changed LIST-002 frozen request sequence or expectations')
    if (len(raw) != 7 or raw[0] != {'fixture': {'rows': fixture_rows}}
            or len(replies) != 6 or any(item is not reply for item, reply in zip(raw[1:], replies))):
        raise ValueError('LIST-002 missing, reordered or changed persisted fixture/repeated request traces')

    ordered = sorted(fixture_rows, key=lambda row: (epoch_millis(row['createdAt']), row['id']), reverse=True)
    observed_pages = []
    expected_paths = []
    from urllib.parse import quote

    for traversal in range(2):
        cursor = None
        traversal_pages = []
        for page_index in range(3):
            template = frozen_requests[page_index]
            if page_index == 0:
                expected_path = template['path']
            else:
                if not isinstance(cursor, dict) or set(cursor) != {'created_at', 'id'}:
                    raise ValueError('LIST-002 traversal ended before the next frozen cursor request')
                canonical_cursor = {'created_at': cursor['created_at'], 'id': cursor['id']}
                marker = '{first.next}' if page_index == 1 else '{second.next}'
                expected_path = template['path'].replace(marker, quote(json.dumps(canonical_cursor, separators=(',', ':')), safe=''))
            expected_paths.append(expected_path)
            reply = replies[traversal * 3 + page_index]
            if (reply.get('method') != template['method'] or reply.get('path') != expected_path
                    or reply.get('auth') != template['auth'] or reply.get('requestJson') is not None):
                raise ValueError('LIST-002 cursor request was not dynamically encoded from the prior response')
            if (reply.get('status') != 200 or reply.get('authenticationAttempts') != 1
                    or reply.get('principal') != {'kind': 'user', 'subject': 'alice', 'identityId': alice}):
                raise ValueError('LIST-002 page did not resolve the authenticated Alice principal')

            statements = reply.get('statements')
            todo_selects = [row for row in statements if row['operation'] == 'SELECT'
                            and re.search(r'\bfrom\s+"?todo"?(?:\s|$)', row['sql'], re.IGNORECASE)]
            sql = todo_selects[0]['sql'] if len(todo_selects) == 1 else ''
            where = re.search(r'\bwhere\b(.*?)(?:\border\s+by\b|\blimit\b|\bfetch\b|$)',
                              sql, re.IGNORECASE | re.DOTALL)
            predicates = where.group(1) if where else ''
            owner_scoped = bool(re.search(r'"?owner_id"?\s*=', predicates, re.IGNORECASE))
            excludes_deleted = bool(re.search(r'"?deleted_at"?\s+is\s+null', predicates, re.IGNORECASE))
            stable_order = bool(re.search(r'\border\s+by\s+"?created_at"?\s+desc\s*,\s*"?id"?\s+desc\b', sql, re.IGNORECASE))
            bounded = bool(re.search(r'\blimit\s+(?:\?|\$\d+)', sql, re.IGNORECASE))
            if (len(statements) != 1 or len(todo_selects) != 1 or not owner_scoped or not excludes_deleted
                    or has_top_level_sql_keyword(predicates, 'or') or not stable_order or not bounded
                    or re.search(r'\boffset\b', sql, re.IGNORECASE)):
                raise ValueError('LIST-002 page lacks one bounded, owner/deletion-scoped stable query')
            if page_index > 0 and not re.search(r'\(\s*\(\s*"?created_at"?\s*,\s*"?id"?\s*\)\s*<\s*\(', predicates, re.IGNORECASE):
                raise ValueError('LIST-002 continuation query lacks the composite keyset predicate')
            if any(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in statements):
                raise ValueError('LIST-002 list request unexpectedly wrote data')

            body = reply.get('body')
            if not isinstance(body, dict) or set(body) != {'items', 'next'} or not isinstance(body['items'], list):
                raise ValueError('LIST-002 response is not a TodoPage')
            expected_page = ordered[page_index * 2:(page_index + 1) * 2]
            expected_items = [{'id': row['id'], 'title': row['title'], 'status': row['status'],
                               'due_at': None, 'created_at': row['createdAt'], 'updated_at': row['createdAt']}
                              for row in expected_page]
            if body['items'] != expected_items or any(not isinstance(item, dict) or frozenset(item) != TODO_VIEW_FIELDS
                                                       for item in body['items']):
                raise ValueError('LIST-002 page has a gap, duplicate, unstable order, foreign row or non-TodoView field')
            expected_cursor = ({'created_at': expected_page[-1]['createdAt'], 'id': expected_page[-1]['id']}
                               if page_index < 2 else None)
            if body['next'] != expected_cursor:
                raise ValueError('LIST-002 continuation cursor does not identify the last returned row')
            cursor = body['next']
            traversal_pages.append(body)
        observed_pages.append(traversal_pages)

    first_ids = [item['id'] for page in observed_pages[0] for item in page['items']]
    second_ids = [item['id'] for page in observed_pages[1] for item in page['items']]
    if first_ids != [row['id'] for row in ordered] or len(set(first_ids)) != 5:
        raise ValueError('LIST-002 traversal contains a gap or duplicate')
    if first_ids != second_ids or observed_pages[0] != observed_pages[1] or expected_paths[:3] != expected_paths[3:]:
        raise ValueError('LIST-002 repeated traversal is not stable')
    return {'status': 200, 'uniqueItemCount': len(set(first_ids)), 'stableAcrossRepeat': True}


def observations(case: dict, raw: list) -> list:
    """Derive only fields supported by retained real response/statement evidence."""
    if (case['id'] not in SUPPORTED or case.get('fixtureVariants')
            or (case.get('followUps') and case['id'] != 'USER-001')):
        raise ValueError('Unsupported case/scenario')
    expected_requests = case.get('requests', [case.get('request')])
    if case['id'] == 'DELETE-001':
        expected_requests = [case['request'], {'method': 'GET', 'path': case['request']['path'],
                                                'auth': case['request'].get('auth')}]
    if case['id'] == 'USER-001':
        follow_ups = case.get('followUps', [])
        if (len(follow_ups) != 3
                or follow_ups[0].get('at') != '2026-01-15T12:00:01Z'
                or follow_ups[1].get('at') != '2026-01-15T12:00:02Z'
                or follow_ups[1].get('operation') != 'compiler-owned authentication refresh'
                or follow_ups[2].get('at') != '2026-01-15T12:04:00Z'
                or case.get('timeline') != {'issuedAt': '2026-01-15T11:59:00Z',
                    'disabledAt': '2026-01-15T12:00:00Z',
                    'credentialExpiresAt': '2026-01-15T12:04:00Z',
                    'maximumRevocationDelaySeconds': 300}):
            raise ValueError('Changed USER-001 frozen refresh/expiry timeline')
        expected_requests = [case['request'], *[step['request'] for step in follow_ups if 'request' in step]]
    replies = [item for item in raw if 'method' in item]
    if case['id'] == 'LIST-002':
        step = list_002_observation(case, raw, replies)
        return [{'variant_index': 0, 'steps': [step]}]
    if len(replies) != len(expected_requests):
        raise ValueError('Missing request observation')
    for request, reply in zip(expected_requests, replies):
        path = (request['path']
                .replace('/todos/t1', '/todos/00000000-0000-4000-8000-000000000021')
                .replace('/todos/t2', '/todos/00000000-0000-4000-8000-000000000022'))
        if case['id'] == 'AUTH-007':
            path = path.replace('/users/alice', '/users/00000000-0000-4000-8000-000000000001')
        if case['id'] == 'AUTH-010':
            path = path.replace('/users/alice', '/users/00000000-0000-4000-8000-000000000001')
        if case['id'] == 'USER-001':
            path = path.replace('/users/alice', '/users/00000000-0000-4000-8000-000000000001')
        if request['method'] != reply['method'] or path != reply['path']:
            raise ValueError('Foreign request observation')
        if not harness.same_json(request.get('auth'), reply.get('auth')) or not harness.same_json(request.get('json'), reply.get('requestJson')):
            raise ValueError('Changed request credential selection/body')
        if type(reply.get('authenticationAttempts')) is not int or reply['authenticationAttempts'] < 0:
            raise ValueError('Missing authentication instrumentation')
        if not isinstance(reply.get('statements'), list) or any(not isinstance(row.get('sql'), str) or not isinstance(row.get('operation'), str) for row in reply['statements']):
            raise ValueError('Missing physical statement instrumentation')
    read_projection = None
    delete_projection = None
    if case['id'] == 'LIST-001':
        step = list_001_observation(raw, replies[0])
    elif case['id'] == 'READ-002':
        step = {'sameStatus': replies[0]['status'] if replies[0]['status'] == replies[1]['status'] else None,
                'sameCode': replies[0]['body'].get('error', {}).get('code') if replies[0]['body'].get('error', {}).get('code') == replies[1]['body'].get('error', {}).get('code') else None,
                'samePublicShape': public_shape(replies[0]['body']) == public_shape(replies[1]['body'])}
    else:
        reply = replies[0]
        if case['id'] == 'PATCH-003':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if (len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply)
                    or fixtures[0][1] != {'id': '00000000-0000-4000-8000-000000000021',
                                          'ownerId': '00000000-0000-4000-8000-000000000001',
                                          'status': 'open', 'deletedAt': None}):
                raise ValueError('Missing, foreign or inactive PATCH-003 todo fixture')
            if not any(row['operation'] == 'ROLLBACK' for row in reply['statements']):
                raise ValueError('Missing PATCH-003 rejected-transaction outcome observation')
        if case['id'] == 'PATCH-004':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            persisted = [(index, item['persistence']) for index, item in enumerate(raw) if 'persistence' in item]
            expected_fixture = {'id': '00000000-0000-4000-8000-000000000022',
                                'ownerId': '00000000-0000-4000-8000-000000000002',
                                'title': 'private bob todo', 'status': 'open', 'deletedAt': None}
            if (len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply)
                    or fixtures[0][1] != expected_fixture
                    or len(persisted) != 1 or persisted[0][0] <= raw.index(reply)
                    or persisted[0][1] != expected_fixture):
                raise ValueError('Missing or changed PATCH-004 persisted foreign-owner fixture')
            principal = reply.get('principal')
            todo_selects = [row for row in reply['statements']
                            if row['operation'] == 'SELECT'
                            and re.search(r'\bfrom\s+"?todo"?', row['sql'], re.IGNORECASE)]
            where = (re.search(r'\bwhere\b(.*)', todo_selects[0]['sql'], re.IGNORECASE | re.DOTALL)
                     if len(todo_selects) == 1 else None)
            predicates = where.group(1) if where else ''
            sql_is_scoped = (bool(re.search(r'"?id"?\s*=', predicates, re.IGNORECASE))
                             and bool(re.search(r'"?owner_id"?\s*=', predicates, re.IGNORECASE))
                             and bool(re.search(r'"?deleted_at"?\s+is\s+null', predicates, re.IGNORECASE))
                             and not has_top_level_sql_keyword(predicates, 'or'))
            writes = [row for row in reply['statements']
                      if row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE')]
            body = reply.get('body')
            if (reply.get('status') != 404 or not isinstance(body, dict)
                    or body.get('error', {}).get('code') != 'todo_not_found'
                    or reply.get('authenticationAttempts') != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice'
                    or principal.get('identityId') != '00000000-0000-4000-8000-000000000001'
                    or len(writes) != 0 or not sql_is_scoped
                    or 'private bob todo' in json.dumps(body)
                    or '00000000-0000-4000-8000-000000000002' in json.dumps(body)):
                raise ValueError('Invalid PATCH-004 foreign-owner concealment, scoped lookup or zero-write evidence')
        if case['id'] == 'DELETE-001':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            persisted = [(index, item['persistence']) for index, item in enumerate(raw) if 'persistence' in item]
            hidden_get = replies[1]
            hidden_index = raw.index(hidden_get)
            clock_now = epoch_millis((fixtures[0][1] if len(fixtures) == 1 else {}).get('clockNow'))
            fixture = fixtures[0][1] if len(fixtures) == 1 else {}
            row = persisted[0][1] if len(persisted) == 1 else {}
            deleted_at = epoch_millis(row.get('deletedAt'))
            updated_at = epoch_millis(row.get('updatedAt'))
            principal = reply.get('principal')
            hidden_principal = hidden_get.get('principal')
            writes = [statement for statement in reply['statements']
                      if statement['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE')]
            todo_updates = [statement for statement in writes
                            if statement['operation'] == 'UPDATE'
                            and re.search(r'\bupdate\s+"?todo"?', statement['sql'], re.IGNORECASE)]
            update_where = (re.search(r'\bwhere\b(.*)', todo_updates[0]['sql'], re.IGNORECASE | re.DOTALL)
                            if len(todo_updates) == 1 else None)
            update_predicates = update_where.group(1) if update_where else ''
            update_is_scoped = (bool(re.search(r'"?id"?\s*=', update_predicates, re.IGNORECASE))
                                and bool(re.search(r'"?owner_id"?\s*=', update_predicates, re.IGNORECASE))
                                and bool(re.search(r'"?deleted_at"?\s+is\s+null', update_predicates, re.IGNORECASE))
                                and not has_top_level_sql_keyword(update_predicates, 'or'))
            hidden_selects = [statement for statement in hidden_get['statements']
                              if statement['operation'] == 'SELECT'
                              and re.search(r'\bfrom\s+"?todo"?', statement['sql'], re.IGNORECASE)]
            hidden_where = (re.search(r'\bwhere\b(.*)', hidden_selects[0]['sql'], re.IGNORECASE | re.DOTALL)
                            if len(hidden_selects) == 1 else None)
            hidden_predicates = hidden_where.group(1) if hidden_where else ''
            hidden_is_scoped = (bool(re.search(r'"?id"?\s*=', hidden_predicates, re.IGNORECASE))
                                and bool(re.search(r'"?owner_id"?\s*=', hidden_predicates, re.IGNORECASE))
                                and bool(re.search(r'"?deleted_at"?\s+is\s+null', hidden_predicates, re.IGNORECASE))
                                and not has_top_level_sql_keyword(hidden_predicates, 'or'))
            expected_fixture = {'id': '00000000-0000-4000-8000-000000000021',
                                'ownerId': '00000000-0000-4000-8000-000000000001',
                                'title': 'active fixture todo', 'status': 'open',
                                'createdAt': '2026-01-15T12:00:00.000Z',
                                'updatedAt': '2026-01-15T12:00:00.000Z', 'deletedAt': None,
                                'clockNow': '2026-01-15T12:00:00.000Z'}
            expected_row = {**{key: value for key, value in expected_fixture.items() if key != 'clockNow'},
                            'deletedAt': '2026-01-15T12:00:00.000Z'}
            delete_principal_ok = (isinstance(principal, dict) and principal.get('kind') == 'user'
                                   and principal.get('subject') == 'alice'
                                   and principal.get('identityId') == expected_fixture['ownerId'])
            get_principal_ok = (isinstance(hidden_principal, dict) and hidden_principal.get('kind') == 'user'
                                and hidden_principal.get('subject') == 'alice'
                                and hidden_principal.get('identityId') == expected_fixture['ownerId'])
            delete_outcomes = [statement['operation'] for statement in reply['statements']
                               if statement['operation'] in ('BEGIN', 'COMMIT', 'ROLLBACK')]
            body = hidden_get.get('body')
            if (len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply)
                    or fixture != expected_fixture or len(persisted) != 1
                    or not raw.index(reply) < persisted[0][0] < hidden_index
                    or row != expected_row or clock_now is None or deleted_at != clock_now or updated_at != clock_now
                    or reply.get('requestStartedAt') != clock_now or reply.get('responseCompletedAt') != clock_now
                    or reply.get('status') != 204 or reply.get('body') is not None
                    or reply.get('authenticationAttempts') != 1 or not delete_principal_ok
                    or len(writes) != 1 or len(todo_updates) != 1 or not update_is_scoped
                    or delete_outcomes != ['BEGIN', 'COMMIT']
                    or hidden_get.get('status') != 404 or not isinstance(body, dict)
                    or body.get('error', {}).get('code') != 'todo_not_found'
                    or hidden_get.get('authenticationAttempts') != 1 or not get_principal_ok
                    or hidden_get.get('statements') is None
                    or any(item['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE')
                           for item in hidden_get.get('statements', []))
                    or not hidden_is_scoped):
                raise ValueError('Invalid DELETE-001 soft-delete persistence, owner scope or subsequent hidden GET evidence')
            delete_projection = {'rowExists': True,
                                 'deletedAt': datetime.fromtimestamp(clock_now / 1000, timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
                                 'subsequentGetStatus': hidden_get['status'],
                                 'ownerScopedDelete': True,
                                 'subsequentGetOwnerScoped': True}
        if case['id'] == 'DELETE-002':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            persisted = [(index, item['persistence']) for index, item in enumerate(raw) if 'persistence' in item]
            fixture = fixtures[0][1] if len(fixtures) == 1 else {}
            row = persisted[0][1] if len(persisted) == 1 else {}
            expected_row = {'id': '00000000-0000-4000-8000-000000000021',
                            'ownerId': '00000000-0000-4000-8000-000000000001',
                            'title': 'previously deleted todo', 'status': 'open',
                            'createdAt': '2026-01-15T11:00:00.000Z',
                            'updatedAt': '2026-01-15T11:59:00.000Z',
                            'deletedAt': '2026-01-15T11:59:00.000Z'}
            principal = reply.get('principal')
            todo_selects = [statement for statement in reply['statements']
                            if statement['operation'] == 'SELECT'
                            and re.search(r'\bfrom\s+"?todo"?', statement['sql'], re.IGNORECASE)]
            where = (re.search(r'\bwhere\b(.*)', todo_selects[0]['sql'], re.IGNORECASE | re.DOTALL)
                     if len(todo_selects) == 1 else None)
            predicates = where.group(1) if where else ''
            sql_is_scoped = (bool(re.search(r'"?id"?\s*=', predicates, re.IGNORECASE))
                             and bool(re.search(r'"?owner_id"?\s*=', predicates, re.IGNORECASE))
                             and bool(re.search(r'"?deleted_at"?\s+is\s+null', predicates, re.IGNORECASE))
                             and not has_top_level_sql_keyword(predicates, 'or'))
            writes = [statement for statement in reply['statements']
                      if statement['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE')]
            body = reply.get('body')
            body_text = json.dumps(body)
            if (len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply) or fixture != expected_row
                    or len(persisted) != 1 or persisted[0][0] <= raw.index(reply) or row != expected_row
                    or reply.get('status') != 404 or not isinstance(body, dict)
                    or body.get('error', {}).get('code') != 'todo_not_found'
                    or reply.get('authenticationAttempts') != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice'
                    or principal.get('identityId') != expected_row['ownerId']
                    or writes or not sql_is_scoped
                    or expected_row['title'] in body_text or expected_row['ownerId'] in body_text):
                raise ValueError('Invalid DELETE-002 deleted fixture, repeated-delete concealment or zero-write evidence')
            delete_projection = {'fixtureStatus': 'deleted', 'ownerScopedLookup': True,
                                 'persistedUnchanged': True}
        if case['id'] == 'USER-001':
            expected_issue = '2026-01-15T11:59:00.000Z'
            expected_disable = '2026-01-15T12:00:00.000Z'
            expected_expiry = '2026-01-15T12:04:00.000Z'
            expected_user_id = '00000000-0000-4000-8000-000000000001'
            expected_todo_id = '00000000-0000-4000-8000-000000000021'
            expected_todo = {'id': expected_todo_id, 'ownerId': expected_user_id,
                             'title': 'retained user todo', 'status': 'open', 'dueAt': None,
                             'createdAt': expected_issue, 'updatedAt': expected_issue, 'deletedAt': None}
            expected_fixture = {
                'user': {'id': expected_user_id, 'subject': 'alice', 'status': 'active', 'disabledAt': None},
                'todo': expected_todo,
                'session': {'issuedAt': 1768478340000, 'expiresAt': 1768478640000,
                            'available': True, 'csrfAvailable': True},
            }
            raw_shape = ['method' if isinstance(item, dict) and 'method' in item
                         else next(iter(item)) if isinstance(item, dict) and len(item) == 1 else None
                         for item in raw]
            if (len(raw) != 7 or raw_shape != ['fixture', 'method', 'persistence', 'method', 'refresh', 'method', 'persistence']
                    or raw[0].get('fixture') != expected_fixture):
                raise ValueError('Missing, reordered or foreign USER-001 fixture and persistence observations')
            first, repeated, expired = replies
            after_disable = raw[2]['persistence']
            final_readback = raw[6]['persistence']
            refresh = raw[4]['refresh']
            expected_persistence = {'stage': 'after_disable',
                'user': {'id': expected_user_id, 'subject': 'alice', 'status': 'disabled', 'disabledAt': expected_disable},
                'todo': expected_todo}
            final_persistence = {**expected_persistence, 'stage': 'final'}
            if after_disable != expected_persistence or final_readback != final_persistence:
                raise ValueError('USER-001 user/Todo state or exact disable timestamp changed')
            disabled_at = epoch_millis(expected_disable)
            issued_at = epoch_millis(expected_issue)
            expires_at = epoch_millis(expected_expiry)
            if (first.get('status') != 204 or first.get('body') is not None
                    or first.get('requestStartedAt') != disabled_at or first.get('responseCompletedAt') != disabled_at
                    or first.get('authenticationAttempts') != 1
                    or first.get('path') != f'/users/{expected_user_id}' or first.get('auth') != 'alice-session'
                    or first.get('principal') != {'kind': 'user', 'subject': 'alice', 'identityId': expected_user_id}):
                raise ValueError('Invalid USER-001 authenticated self-disable response or timeline')
            statements = first.get('statements', [])
            user_selects = [row for row in statements if row['operation'] == 'SELECT' and 'from "user"' in row['sql'].lower()]
            user_updates = [row for row in statements if row['operation'] == 'UPDATE' and re.search(r'\bupdate\s+"?user"?(?:\s|$)', row['sql'], re.IGNORECASE)]
            writes = [row for row in statements if row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE')]
            active_id_lookups = []
            for row in user_selects:
                where = re.search(r'\bwhere\b(.*)', row['sql'], re.IGNORECASE | re.DOTALL)
                predicates = where.group(1) if where else ''
                if (re.search(r'\b"?id"?\s*=', predicates, re.IGNORECASE)
                        and re.search(r'\b"?status"?\s*=', predicates, re.IGNORECASE)
                        and not has_top_level_sql_keyword(predicates, 'or')):
                    active_id_lookups.append(row)
            update_where = (re.search(r'\bwhere\b(.*)', user_updates[0]['sql'], re.IGNORECASE | re.DOTALL)
                            if len(user_updates) == 1 else None)
            update_predicates = update_where.group(1) if update_where else ''
            update_scoped = (bool(re.search(r'\b"?id"?\s*=', update_predicates, re.IGNORECASE))
                             and bool(re.search(r'\b"?status"?\s*=', update_predicates, re.IGNORECASE))
                             and not has_top_level_sql_keyword(update_predicates, 'or'))
            transaction = [row['operation'] for row in statements
                           if row['operation'] in ('BEGIN', 'COMMIT', 'ROLLBACK')]
            begin_indexes = [index for index, row in enumerate(statements) if row['operation'] == 'BEGIN']
            update_indexes = [index for index, row in enumerate(statements) if row in user_updates]
            commit_indexes = [index for index, row in enumerate(statements) if row['operation'] == 'COMMIT']
            if (len(active_id_lookups) != 1 or len(user_updates) != 1 or len(writes) != 1
                    or len(begin_indexes) != 1 or len(commit_indexes) != 1 or transaction != ['BEGIN', 'COMMIT']
                    or not update_scoped or not begin_indexes[0] < update_indexes[0] < commit_indexes[0]):
                raise ValueError('USER-001 disable lacks a committed ID/status-scoped user transition')
            if (repeated.get('status') != 403 or repeated.get('body', {}).get('error', {}).get('code') != 'user_disabled'
                    or repeated.get('requestStartedAt') != disabled_at + 1000
                    or repeated.get('responseCompletedAt') != disabled_at + 1000
                    or repeated.get('authenticationAttempts') != 1
                    or any(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in repeated['statements'])):
                raise ValueError('USER-001 repeated disable was not concealed as a zero-write disabled-user rejection')
            refresh_selects = [row for row in refresh.get('statements', [])
                               if row['operation'] == 'SELECT' and 'from "user"' in row['sql'].lower()]
            live_user_checks = []
            for row in refresh_selects:
                where = re.search(r'\bwhere\b(.*)', row['sql'], re.IGNORECASE | re.DOTALL)
                predicates = where.group(1) if where else ''
                projection = re.search(r'\bselect\b(.*?)\bfrom\b', row['sql'], re.IGNORECASE | re.DOTALL)
                if (re.search(r'\b"?authentication_subject"?\s*=', predicates, re.IGNORECASE)
                        and projection is not None
                        and re.search(r'\bstatus\b', projection.group(1), re.IGNORECASE)
                        and not has_top_level_sql_keyword(predicates, 'or')):
                    live_user_checks.append(row)
            if (refresh.get('at') != disabled_at + 2000 or refresh.get('outcome') != 'rejected'
                    or refresh.get('declaredFailure') != 'UserDisabled'
                    or refresh.get('internalCode') != 'principal_inactive'
                    or refresh.get('credentialReturned') is not False
                    or refresh.get('publicFailureCode') != 'user_disabled'
                    or len(live_user_checks) != 1
                    or any(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in refresh.get('statements', []))):
                raise ValueError('USER-001 compiler-owned refresh did not reject the disabled principal without issuing credentials')
            if (expired.get('status') != 401 or expired.get('body', {}).get('error', {}).get('code') != 'invalid_credentials'
                    or expired.get('requestStartedAt') != expires_at or expired.get('responseCompletedAt') != expires_at
                    or expired.get('authenticationAttempts') != 1
                    or any(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in expired['statements'])):
                raise ValueError('USER-001 credential was not rejected at exact expiry without writes')
            return [{'variant_index': 0, 'steps': [
                {'status': first['status'], 'userStatus': after_disable['user']['status'], 'todoRowExists': True},
                {'status': repeated['status'], 'code': repeated['body']['error']['code'], 'writes': 0},
                {'failure': refresh['declaredFailure'], 'code': refresh['publicFailureCode'],
                 'credentialsIssued': 0 if not refresh['credentialReturned'] and refresh['outcome'] == 'rejected' else 1},
                {'status': expired['status'], 'code': expired['body']['error']['code'], 'writes': 0},
            ]}]
        if case['id'] == 'READ-001':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing READ-001 persisted active owner fixture')
            fixture = fixtures[0][1]
            body = reply.get('body')
            principal = reply.get('principal')
            statements = reply.get('statements', [])
            selects = [row for row in statements if row['operation'] == 'SELECT']
            where = selects[0]['sql'].lower().partition(' where ')[2] if len(selects) == 1 else ''
            expected_fixture = {'id': '00000000-0000-4000-8000-000000000021',
                                'ownerId': '00000000-0000-4000-8000-000000000001',
                                'title': 'active fixture todo', 'status': 'open', 'deletedAt': None}
            if (fixture != expected_fixture or reply['authenticationAttempts'] != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice' or principal.get('identityId') != fixture['ownerId']
                    or not isinstance(body, dict) or body.get('id') != fixture['id']
                    or body.get('title') != fixture['title'] or body.get('status') != fixture['status']
                    or len(statements) != 1 or len(selects) != 1 or 'from "todo"' not in selects[0]['sql'].lower()
                    or 'owner_id' not in where or 'deleted_at' not in where or 'is null' not in where):
                raise ValueError('Invalid READ-001 owned todo fixture, principal or response identity')
            response_fields = frozenset(body)
            read_projection = {'outputSchema': 'TodoView' if response_fields == TODO_VIEW_FIELDS else None,
                               'privateFieldsAbsent': [field for field in TODO_PRIVATE_FIELDS if not contains_key(body, field)],
                               'ownerScopedSelect': True, 'activeRowFilter': True,
                               'ownedTodoMatchesFixture': True}
        if case['id'] == 'AUTH-002':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-002 active-user browser-session fixture')
            fixture = fixtures[0][1]
            issued = epoch_millis(fixture.get('sessionIssuedAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            expires = epoch_millis(fixture.get('sessionExpiresAt'))
            principal = reply.get('principal')
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'active'
                    or fixture.get('sessionAvailable') is not True or fixture.get('csrfAvailable') is not True
                    or not fixture.get('userId') or issued is None or requested is None or completed is None
                    or expires is None or not issued <= requested <= completed < expires
                    or expires - issued > 300_000 or reply['authenticationAttempts'] != 1
                    or reply['status'] != 200 or not isinstance(principal, dict)
                    or principal.get('kind') != 'user' or principal.get('subject') != 'alice'
                    or principal.get('identityId') != fixture['userId']):
                raise ValueError('Invalid AUTH-002 active user, browser session or resolved principal evidence')
        if case['id'] == 'AUTH-003':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-003 active Alice and JWT fixture')
            fixture = fixtures[0][1]
            claims = fixture.get('tokenClaims')
            principal = reply.get('principal')
            issued = epoch_millis((claims or {}).get('issuedAt'))
            expires = epoch_millis((claims or {}).get('expiresAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            statements = reply.get('statements', [])
            selects = [row for row in statements if row['operation'] == 'SELECT']
            authority = [row for row in selects if 'from "user"' in row['sql'].lower()
                         and 'authentication_subject' in row['sql'].lower()]
            todos = [row for row in selects if 'from "todo"' in row['sql'].lower()]
            authority_where = (re.search(r'\bwhere\b(.*)', authority[0]['sql'], re.IGNORECASE | re.DOTALL)
                               if len(authority) == 1 else None)
            todo_where = (re.search(r'\bwhere\b(.*)', todos[0]['sql'], re.IGNORECASE | re.DOTALL)
                          if len(todos) == 1 else None)
            authority_predicates = re.sub(r'\s+', ' ', authority_where.group(1).lower()).strip() if authority_where else ''
            todo_predicates = re.sub(r'\s+', ' ', todo_where.group(1).lower()).strip() if todo_where else ''
            authority_subject_scoped = bool(re.match(r'^\s*"?authentication_subject"?\s*=', authority_predicates))
            todo_owner_scoped = bool(re.match(r'^\s*"?owner_id"?\s*=', todo_predicates))
            todo_active_only = bool(re.search(r'\band\s+"?deleted_at"?\s+is\s+null\b', todo_predicates))
            authority_has_unscoped_or = bool(authority_where and has_top_level_sql_keyword(authority_where.group(1), 'or'))
            todo_has_unscoped_or = bool(todo_where and has_top_level_sql_keyword(todo_where.group(1), 'or'))
            body = reply.get('body')
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'active'
                    or not fixture.get('userId') or not isinstance(claims, dict)
                    or claims.get('signatureValid') is not True
                    or claims.get('issuer') != 'https://issuer.test' or claims.get('audience') != 'todo'
                    or claims.get('subject') != 'alice' or claims.get('email') != 'attacker@example.test'
                    or issued is None or expires is None or requested is None or completed is None
                    or not issued <= requested <= completed < expires or expires - issued > 120_000
                    or fixture.get('providerCalls') != ['https://issuer.test/.well-known/openid-configuration', 'https://issuer.test/jwks']
                    or fixture.get('rejectedProviderCalls') != []
                    or reply.get('auth') != 'alice-jwt' or reply.get('path') != '/todos'
                    or reply['status'] != 200 or reply['authenticationAttempts'] != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice' or principal.get('identityId') != fixture['userId']
                    or len(statements) != 2 or len(selects) != 2 or len(authority) != 1 or len(todos) != 1
                    or not authority_subject_scoped or authority_has_unscoped_or
                    or not todo_owner_scoped or not todo_active_only or todo_has_unscoped_or
                    or not isinstance(body, dict) or contains_provider_fields(body)
                    or any(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in statements)):
                raise ValueError('Invalid AUTH-003 signed Alice bearer, owner-scoped read or authority-query evidence')
            read_projection = {'principalKind': 'user', 'principalUser': 'alice',
                               'providerFieldsInOutput': contains_provider_fields(body),
                               'databaseQueries': len(statements), 'authorityQueries': len(authority),
                               'todoQueries': len(todos)}
        if case['id'] == 'AUTH-004':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-004 competing Alice session and Bob JWT fixture')
            fixture = fixtures[0][1]
            session_fixture = fixture.get('aliceSession')
            bob_fixture = fixture.get('bobJwt')
            claims = (bob_fixture or {}).get('tokenClaims')
            issued = epoch_millis((claims or {}).get('issuedAt'))
            expires = epoch_millis((claims or {}).get('expiresAt'))
            session_issued = epoch_millis((session_fixture or {}).get('issuedAt'))
            session_expires = epoch_millis((session_fixture or {}).get('expiresAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            statements = reply.get('statements', [])
            principal = reply.get('principal')
            response_code = reply.get('body', {}).get('error', {}).get('code')
            if (not isinstance(session_fixture, dict) or session_fixture.get('subject') != 'alice'
                    or session_fixture.get('userId') != '00000000-0000-4000-8000-000000000001'
                    or session_fixture.get('status') != 'active' or session_fixture.get('sessionAvailable') is not True
                    or not isinstance(bob_fixture, dict) or bob_fixture.get('subject') != 'bob'
                    or bob_fixture.get('userId') != '00000000-0000-4000-8000-000000000002'
                    or bob_fixture.get('status') != 'active' or not isinstance(claims, dict)
                    or claims.get('signatureValid') is not True or claims.get('issuer') != 'https://issuer.test'
                    or claims.get('audience') != 'todo' or claims.get('subject') != 'bob'
                    or issued is None or expires is None or session_issued is None or session_expires is None
                    or requested is None or completed is None
                    or not session_issued <= requested <= completed < session_expires
                    or not issued <= requested <= completed < expires or expires - issued > 120_000
                    or fixture.get('providerCalls') != [] or fixture.get('rejectedProviderCalls') != []
                    or reply.get('auth') != ['alice-session', 'bob-jwt'] or reply.get('path') != '/todos'
                    or reply['status'] != 401 or response_code != 'ambiguous_credentials'
                    or reply['authenticationAttempts'] != 1 or statements != [] or principal is not None):
                raise ValueError('Invalid AUTH-004 pre-validation ambiguity, competing identities or zero-query evidence')
            read_projection = {'status': reply['status'], 'code': response_code,
                               'databaseQueries': len(statements),
                               'authenticationAttempts': reply['authenticationAttempts'],
                               'ambiguousBeforeValidation': True}
        if case['id'] == 'AUTH-007':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-007 persisted active-user and JWT fixture')
            fixture = fixtures[0][1]
            claims = fixture.get('tokenClaims')
            principal = reply.get('principal')
            issued = epoch_millis((claims or {}).get('issuedAt'))
            expires = epoch_millis((claims or {}).get('expiresAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            body = reply.get('body')
            private_fields_absent = ['email'] if not contains_key(body, 'email') else []
            body_text = json.dumps(body, sort_keys=True)
            provider_calls = fixture.get('providerCalls')
            rejected_provider_calls = fixture.get('rejectedProviderCalls')
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'active'
                    or not fixture.get('userId') or fixture.get('storedEmail') != 'alice@example.test'
                    or not isinstance(claims, dict) or claims.get('signatureValid') is not True
                    or claims.get('issuer') != 'https://issuer.test' or claims.get('audience') != 'todo'
                    or claims.get('subject') != 'alice' or claims.get('email') != 'attacker@example.test'
                    or issued is None or expires is None or requested is None or completed is None
                    or not issued <= requested <= completed < expires or expires - issued > 120_000
                    or provider_calls != ['https://issuer.test/.well-known/openid-configuration', 'https://issuer.test/jwks']
                    or rejected_provider_calls != []
                    or reply.get('auth') != 'alice-jwt' or reply.get('path') != f"/users/{fixture['userId']}/todos"
                    or reply['status'] != 200 or reply['authenticationAttempts'] != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice' or principal.get('identityId') != fixture['userId']
                    or not isinstance(body, dict) or frozenset(body) != {'user_id', 'todos'}
                    or body.get('user_id') != fixture['userId'] or not isinstance(body.get('todos'), list)
                    or private_fields_absent != ['email'] or 'attacker@example.test' in body_text
                    or any(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in reply['statements'])):
                raise ValueError('Invalid AUTH-007 signed-email fixture, self-scoped response or provider-email isolation')
        if case['id'] == 'AUTH-008':
            if reply['authenticationAttempts'] != 1:
                raise ValueError('Missing AUTH-008 authentication attempt observation')
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-008 active-user bearer fixture')
            fixture = fixtures[0][1]
            claims = fixture.get('tokenClaims')
            principal = reply.get('principal')
            issued = epoch_millis(fixture.get('credentialIssuedAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            expires = epoch_millis(fixture.get('credentialExpiresAt'))
            select_sql = [row['sql'].lower() for row in reply['statements'] if row['operation'] == 'SELECT']
            principal_queries = sum(any(f'from "{table}"' in sql for table in ('user', 'service')) for sql in select_sql)
            credential_tables = ('__jadpo_auth_sessions', '__jadpo_auth_service_credentials', 'service_credential')
            credential_queries = sum(any(f'from "{table}"' in sql for table in credential_tables)
                                     and not any(f'from "{table}"' in sql for table in ('user', 'service'))
                                     for sql in select_sql)
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'active'
                    or fixture.get('cacheAdapter') != 'unavailable; no cache adapter is configured'
                    or not fixture.get('userId') or not fixture.get('credentialId')
                    or issued is None or requested is None or completed is None or expires is None
                    or not issued <= requested <= completed < expires or expires - issued > 300_000
                    or not isinstance(claims, dict) or claims.get('signatureValid') is not True
                    or claims.get('version') != 1 or claims.get('audience') != 'todo-api'
                    or claims.get('strategy') != 'api_bearer' or claims.get('credentialId') != fixture['credentialId']
                    or claims.get('subject') != 'alice' or claims.get('userId') != fixture['userId']
                    or claims.get('issuedAt') != issued or claims.get('expiresAt') != expires
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice' or principal.get('identityId') != fixture['userId']
                    or principal_queries != 0 or credential_queries != 0):
                raise ValueError('Invalid AUTH-008 signed bearer, active-user identity, or authority-query evidence')
            reply['derivedAuthorityQueries'] = {'principalAuthorityQueries': principal_queries,
                                                'credentialAuthorityQueries': credential_queries}
        if case['id'] == 'AUTH-009':
            if reply['authenticationAttempts'] != 1:
                raise ValueError('Missing AUTH-009 service credential-exchange attempt observation')
            selects = [row['sql'].lower() for row in reply['statements'] if row['operation'] == 'SELECT']
            if (not any('from "__jadpo_auth_service_credentials"' in sql for sql in selects)
                    or not any('from "service_credential"' in sql for sql in selects)
                    or not any('from "service"' in sql and 'name' in sql for sql in selects)):
                raise ValueError('Missing AUTH-009 persisted credential and active-service lookup observations')
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-009 persisted active-service fixture')
            fixture = fixtures[0][1]
            issued = epoch_millis(fixture.get('credentialIssuedAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            credential_expires = epoch_millis(fixture.get('credentialExpiresAt'))
            exchange = reply.get('serviceExchange')
            if (fixture.get('subject') != 'reporting' or fixture.get('status') != 'active'
                    or fixture.get('disabledAt') is not None or not fixture.get('serviceId')
                    or not fixture.get('credentialId') or issued is None or requested is None
                    or completed is None or credential_expires is None
                    or not issued < requested <= completed < credential_expires
                    or not isinstance(exchange, dict) or exchange.get('signatureValid') is not True
                    or exchange.get('version') != 1 or exchange.get('audience') != 'todo-service-token'
                    or exchange.get('strategy') != 'api_bearer' or exchange.get('credentialId') != fixture['credentialId']
                    or exchange.get('subject') != fixture['subject'] or exchange.get('serviceId') != fixture['serviceId']
                    or exchange.get('principalKind') != 'service' or exchange.get('authenticationStrength') != 'api_key'
                    or exchange.get('secretInOutput') is not False or exchange.get('outputTokenRedacted') is not True
                    or reply['body'].get('access_token') != '<redacted>' or reply['body'].get('token_type') != 'Bearer'):
                raise ValueError('Invalid AUTH-009 persisted fixture, exchanged token or redaction evidence')
            token_issued = epoch_millis(exchange.get('issuedAt'))
            token_expires = epoch_millis(exchange.get('expiresAt'))
            response_expires = epoch_millis(reply['body'].get('expires_at'))
            if (token_issued is None or token_expires is None or response_expires is None
                    or not issued <= requested <= token_issued <= completed < token_expires <= credential_expires
                    or response_expires != token_expires):
                raise ValueError('Invalid AUTH-009 signed service-token lifetime')
        if case['id'] == 'AUTH-010':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            persisted = [(index, item['persistence']) for index, item in enumerate(raw) if 'persistence' in item]
            if (len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply)
                    or len(persisted) != 1 or persisted[0][0] <= raw.index(reply)):
                raise ValueError('Missing AUTH-010 active Alice credential or disabled-user readback')
            fixture = fixtures[0][1]
            row = persisted[0][1]
            claims = fixture.get('tokenClaims')
            issued = epoch_millis(fixture.get('credentialIssuedAt'))
            expires = epoch_millis(fixture.get('credentialExpiresAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            disabled = epoch_millis(row.get('disabledAt'))
            principal = reply.get('principal')
            statements = reply.get('statements', [])
            selects = [statement for statement in statements if statement['operation'] == 'SELECT']
            # Count identity resolution, not the separate domain-row SELECT
            # used by User.disable_user after the principal is authenticated.
            principal_selects = []
            for statement in selects:
                sql = statement['sql'].lower()
                where = re.search(r'\bwhere\b(.*)', sql, re.IGNORECASE | re.DOTALL)
                if ('from "user"' in sql and where
                        and re.search(r'"?authentication_subject"?\s*=', where.group(1))):
                    principal_selects.append(statement)
            principal_queries = len(principal_selects)
            authority_sql = principal_selects[0]['sql'].lower() if len(principal_selects) == 1 else ''
            authority_where = re.search(r'\bwhere\b(.*)', authority_sql, re.IGNORECASE | re.DOTALL)
            authority_predicates = authority_where.group(1) if authority_where else ''
            authority_identity_scoped = bool(re.search(
                r'"?authentication_subject"?\s*=', authority_predicates))
            authority_has_unscoped_or = bool(authority_where and
                                             has_top_level_sql_keyword(authority_predicates, 'or'))
            credential_tables = ('__jadpo_auth_sessions', '__jadpo_auth_service_credentials', 'service_credential')
            credential_queries = sum(any(f'from "{table}"' in statement['sql'].lower()
                                         for table in credential_tables)
                                     and not any(f'from "{table}"' in statement['sql'].lower()
                                                 for table in ('user', 'service')) for statement in selects)
            writes = [statement for statement in statements
                      if statement['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE')]
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'active'
                    or not fixture.get('userId') or not fixture.get('credentialId')
                    or not isinstance(claims, dict) or claims.get('signatureValid') is not True
                    or claims.get('version') != 1 or claims.get('audience') != 'todo-api'
                    or claims.get('strategy') != 'api_bearer' or claims.get('credentialId') != fixture['credentialId']
                    or claims.get('subject') != 'alice' or claims.get('userId') != fixture['userId']
                    or issued is None or expires is None or requested is None or completed is None or disabled is None
                    or claims.get('issuedAt') != issued or claims.get('expiresAt') != expires
                    or not issued <= requested <= completed < expires or expires - issued > 300_000
                    or reply.get('method') != 'DELETE' or reply.get('path') != f"/users/{fixture['userId']}"
                    or reply.get('auth') != 'alice-bounded-bearer' or reply.get('status') != 204
                    or reply.get('body') is not None or reply.get('authenticationAttempts') != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice' or principal.get('identityId') != fixture['userId']
                    or principal_queries != 1 or 'from "user"' not in authority_sql
                    or not authority_identity_scoped or authority_has_unscoped_or
                    or len(writes) != 1 or writes[0]['operation'] != 'UPDATE'
                    or 'update "user"' not in writes[0]['sql'].lower()
                    or not re.search(r'\bwhere\b.*"?(?:id|authentication_subject)"?\s*=', writes[0]['sql'].lower(), re.DOTALL)
                    or has_top_level_sql_keyword(writes[0]['sql'].lower().split('where', 1)[-1], 'or')
                    or row.get('userId') != fixture['userId'] or row.get('subject') != 'alice'
                    or row.get('status') != 'disabled' or not requested <= disabled <= completed):
                raise ValueError('Invalid AUTH-010 bounded bearer, fresh principal lookup, disablement or persistence evidence')
            reply['derivedAuthorityQueries'] = {'principalAuthorityQueries': principal_queries,
                                                'credentialAuthorityQueries': credential_queries}
        if case['id'] == 'AUTH-006':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-006 persisted disabled-user fixture')
            fixture = fixtures[0][1]
            issued = epoch_millis(fixture.get('issuedAt'))
            disabled = epoch_millis(fixture.get('disabledAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            expires = epoch_millis(fixture.get('credentialExpiresAt'))
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'disabled'
                    or issued is None or disabled is None or requested is None or expires is None
                    or not issued < disabled < requested < expires):
                raise ValueError('Invalid AUTH-006 persisted user state or credential timeline')
        if case['id'] == 'AUTH-011':
            if reply['authenticationAttempts'] != 1:
                raise ValueError('Missing AUTH-011 service credential-exchange attempt observation')
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing AUTH-011 persisted disabled-service fixture')
            fixture = fixtures[0][1]
            issued = epoch_millis(fixture.get('credentialIssuedAt'))
            disabled = epoch_millis(fixture.get('disabledAt'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            expires = epoch_millis(fixture.get('credentialExpiresAt'))
            if (fixture.get('subject') != 'reporting' or fixture.get('status') != 'disabled'
                    or issued is None or disabled is None or requested is None or expires is None
                    or not issued < disabled < requested < expires):
                raise ValueError('Invalid AUTH-011 persisted service state or credential timeline')
        step = {'status': reply['status'], 'databaseQueries': len(reply['statements']),
                'authenticationAttempts': reply['authenticationAttempts'],
                'writes': sum(row['operation'] in ('INSERT', 'UPDATE', 'DELETE', 'REPLACE') for row in reply['statements'])}
        if read_projection is not None:
            step.update(read_projection)
        if delete_projection is not None:
            step.update(delete_projection)
        if case['id'] == 'CREATE-001':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            persisted = [(index, item['persistence']) for index, item in enumerate(raw) if 'persistence' in item]
            if (len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply)
                    or len(persisted) != 1 or persisted[0][0] <= raw.index(reply)):
                raise ValueError('Missing CREATE-001 active-user fixture or persisted create observation')
            fixture = fixtures[0][1]
            row = persisted[0][1]
            body = reply.get('body')
            request_json = reply.get('requestJson')
            clock_now = epoch_millis(fixture.get('clockNow'))
            due_at = epoch_millis((request_json or {}).get('due_at'))
            stored_due_at = epoch_millis(row.get('dueAt'))
            issued_created_at = epoch_millis(row.get('createdAt'))
            issued_updated_at = epoch_millis(row.get('updatedAt'))
            principal = reply.get('principal')
            transaction_outcomes = [item['operation'] for item in reply['statements']
                                    if item['operation'] in ('BEGIN', 'COMMIT', 'ROLLBACK')]
            begin_indexes = [index for index, item in enumerate(reply['statements'])
                             if item['operation'] == 'BEGIN']
            todo_insert_indexes = [index for index, item in enumerate(reply['statements'])
                                   if item['operation'] == 'INSERT'
                                   and 'into "todo"' in item['sql'].lower()]
            commit_indexes = [index for index, item in enumerate(reply['statements'])
                              if item['operation'] == 'COMMIT']
            if (fixture.get('subject') != 'alice' or fixture.get('status') != 'active'
                    or not fixture.get('userId') or fixture.get('sessionAvailable') is not True
                    or fixture.get('csrfAvailable') is not True
                    or fixture.get('clockNow') != '2026-01-15T12:00:00.000Z'
                    or clock_now is None or due_at is None or stored_due_at != due_at
                    or not clock_now < due_at or issued_created_at != clock_now
                    or issued_updated_at != clock_now
                    or reply['requestStartedAt'] != clock_now or reply['responseCompletedAt'] != clock_now
                    or reply['status'] != 201 or reply['authenticationAttempts'] != 1
                    or not isinstance(principal, dict) or principal.get('kind') != 'user'
                    or principal.get('subject') != 'alice' or principal.get('identityId') != fixture['userId']
                    or not isinstance(body, dict) or frozenset(body) != TODO_VIEW_FIELDS
                    or body.get('id') != row.get('id') or body.get('title') != request_json.get('title')
                    or body.get('status') != row.get('status') or body.get('status') != 'open'
                    or epoch_millis(body.get('due_at')) != due_at
                    or epoch_millis(body.get('created_at')) != clock_now
                    or epoch_millis(body.get('updated_at')) != clock_now
                    or row.get('ownerId') != fixture['userId'] or row.get('deletedAt') is not None
                    or row.get('title') != request_json.get('title') or row.get('status') != 'open'
                    or transaction_outcomes != ['BEGIN', 'COMMIT']
                    or len(begin_indexes) != 1 or len(todo_insert_indexes) != 1 or len(commit_indexes) != 1
                    or not begin_indexes[0] < todo_insert_indexes[0] < commit_indexes[0]
                    or sum(item['operation'] == 'INSERT' for item in reply['statements']) != 1
                    or step['writes'] != 1):
                raise ValueError('Invalid CREATE-001 owner-bound persisted create or public TodoView evidence')
            step.update({'storedOwner': 'alice', 'outputSchema': 'TodoView',
                         'ownerFieldAbsent': True, 'fixedClock': fixture['clockNow'],
                         'storedDueAt': row['dueAt'], 'transactionCommitted': True})
        if case['id'] == 'CREATE-003':
            fixtures = [(index, item['fixture']) for index, item in enumerate(raw) if 'fixture' in item]
            if len(fixtures) != 1 or fixtures[0][0] >= raw.index(reply):
                raise ValueError('Missing CREATE-003 injected-clock fixture')
            fixture = fixtures[0][1]
            clock_now = epoch_millis(fixture.get('clockNow'))
            due_at = epoch_millis((reply.get('requestJson') or {}).get('due_at'))
            requested = epoch_millis(reply.get('requestStartedAt'))
            completed = epoch_millis(reply.get('responseCompletedAt'))
            if (fixture.get('clockNow') != '2026-01-15T12:00:00.000Z'
                    or clock_now is None or due_at is None or requested != clock_now
                    or completed != clock_now or not due_at < clock_now):
                raise ValueError('Invalid CREATE-003 fixed-clock relation or request timeline')
            transaction_outcomes = [row['operation'] for row in reply['statements']
                                    if row['operation'] in ('BEGIN', 'COMMIT', 'ROLLBACK')]
            if transaction_outcomes != ['BEGIN', 'ROLLBACK']:
                raise ValueError('Missing CREATE-003 transaction rollback outcome')
            step['clockNow'] = fixture['clockNow']
            step['dueAt'] = reply['requestJson']['due_at']
            step['requestClockAligned'] = True
            step['rollbackObserved'] = True
        if case['id'] == 'PATCH-003':
            step['rollbackObserved'] = True
        if case['id'] == 'PATCH-004':
            step.update({'principalKind': 'user', 'principalUser': 'alice', 'fixtureOwner': 'bob',
                         'ownerScopedLookup': True, 'writes': 0, 'persistedUnchanged': True})
        if case['id'] == 'AUTH-008':
            step['principalKind'] = reply['principal']['kind']
            step['principalUser'] = reply['principal']['subject']
            step['principalAuthorityQueries'] = reply['derivedAuthorityQueries']['principalAuthorityQueries']
            step['credentialAuthorityQueries'] = reply['derivedAuthorityQueries']['credentialAuthorityQueries']
        if case['id'] == 'AUTH-002':
            step['principalKind'] = reply['principal']['kind']
            step['principalUser'] = reply['principal']['subject']
            step['providerFieldsInOutput'] = contains_provider_fields(reply.get('body'))
        if case['id'] == 'AUTH-007':
            step['privateFieldsAbsent'] = private_fields_absent
            step['providerEmailUsed'] = False
        if case['id'] == 'AUTH-006':
            fixture = next(item['fixture'] for item in raw if 'fixture' in item)
            step['fixtureStatus'] = fixture['status']
            step['timeline'] = {'issuedAt': fixture['issuedAt'], 'disabledAt': fixture['disabledAt'],
                                'requestAt': reply['requestStartedAt'],
                                'credentialExpiresAt': fixture['credentialExpiresAt']}
        if case['id'] == 'AUTH-009':
            fixture = next(item['fixture'] for item in raw if 'fixture' in item)
            step['principalKind'] = 'service'
            step['principalService'] = fixture['subject']
            step['secretInOutput'] = reply['serviceExchange']['secretInOutput']
        if case['id'] == 'AUTH-010':
            step['principalKind'] = reply['principal']['kind']
            step['principalUser'] = reply['principal']['subject']
            step['principalAuthorityQueries'] = reply['derivedAuthorityQueries']['principalAuthorityQueries']
            step['credentialAuthorityQueries'] = reply['derivedAuthorityQueries']['credentialAuthorityQueries']
            step['credentialAuthorityQueriesRecorded'] = True
            fixture = next(item['fixture'] for item in raw if 'fixture' in item)
            persisted = next(item['persistence'] for item in raw if 'persistence' in item)
            step['fixtureStatus'] = fixture['status']
            step['persistedStatus'] = persisted['status']
        if case['id'] == 'AUTH-011':
            fixture = next(item['fixture'] for item in raw if 'fixture' in item)
            step['fixtureStatus'] = fixture['status']
            step['timeline'] = {'credentialIssuedAt': fixture['credentialIssuedAt'], 'disabledAt': fixture['disabledAt'],
                                'requestAt': reply['requestStartedAt'],
                                'credentialExpiresAt': fixture['credentialExpiresAt']}
        body = reply.get('body')
        code = body.get('error', {}).get('code') if isinstance(body, dict) else None
        if code is not None:
            step['code'] = code
        if case['id'] in ('AUTH-012', 'PUBLIC-001'):
            step['body'] = reply.get('body')
        if case['id'] == 'CREATE-004':
            persisted = [item.get('persistence') for item in raw if 'persistence' in item]
            if len(persisted) != 1 or persisted[0]['id'] != (reply.get('body') or {}).get('id'):
                raise ValueError('Missing persisted observation')
            step['storedDueAt'] = persisted[0]['storedDueAt']
            step['outputDueAt'] = reply['body']['due_at']
    return [{'variant_index': 0, 'steps': [step]}]


def apply_checkpoint(report: dict, outputs: list[dict], root: Path):
    contract = harness.read_contract(root)
    cases = {case['id']: case for case in contract['cases']}
    observed = {}
    if len(outputs) != 2 or {output['backend'] for output in outputs} != set(harness.BACKENDS):
        raise ValueError('Missing/duplicate backend')
    for output in outputs:
        ids = [item['id'] for item in output['results']]
        if len(ids) != len(set(ids)) or set(ids) != set(SUPPORTED):
            raise ValueError('Missing/duplicate/foreign supported result')
        for item in output['results']:
            observed[(output['backend'], item['id'])] = None if item.get('adapterFailure') is True else observations(cases[item['id']], item['raw'])
    for entry in report['results']:
        key = (entry['backend'], entry['id'])
        if key not in observed:
            entry['reason'] = 'Unsupported by the reviewed migrated HTTP checkpoint; full application/gate prerequisites remain'
            continue
        entry.pop('reason', None)
        if observed[key] is None:
            entry['status'] = 'failed'
            entry.pop('observations', None)
            entry['failures'] = ['Checkpoint adapter failure; no raw exception text retained']
            continue
        entry['observations'] = observed[key]
        entry['failures'] = harness.assess(cases[entry['id']], observed[key])
        entry['status'] = 'failed' if entry['failures'] else 'passed'
    report['status'] = 'failed' if any(entry['status'] == 'failed' for entry in report['results']) else 'not_run'
    harness.validate_report(report, root)


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
                           ('first-party-authentication.ts', 'original-authentication.ts')):
        original['target/'+name] = harness.digest(evidence/retained)
    if original != checkpoint['generated_original']:
        raise ValueError('Changed retained generated original artifacts')
    if hashes(dependencies/'node_modules/jose') != checkpoint['dependency_files']:
        raise ValueError('Changed actual pinned JWT dependency files')
    outputs = []
    for backend in harness.BACKENDS:
        path = evidence/f'{backend}-raw.json'
        if harness.digest(path) != checkpoint['raw_files'][path.name]:
            raise ValueError('Changed raw observations')
        outputs.append(json.loads(path.read_text()))
    compatibility_path = evidence/'application/build/compatibility/public-failure-codes.json'
    if not compatibility_path.is_file():
        raise ValueError('Missing retained compiler public-failure mapping')
    compatibility = json.loads(compatibility_path.read_text())
    user_disabled_codes = [row.get('code') for row in compatibility.get('current', [])
                           if row.get('failure') == 'UserDisabled']
    if len(user_disabled_codes) != 1 or not isinstance(user_disabled_codes[0], str):
        raise ValueError('Missing unique compiler UserDisabled public-failure mapping')
    for output in outputs:
        user_result = next((item for item in output['results'] if item['id'] == 'USER-001'), None)
        refresh_rows = [item.get('refresh') for item in (user_result or {}).get('raw', [])
                        if isinstance(item, dict) and 'refresh' in item]
        if len(refresh_rows) != 1 or refresh_rows[0].get('publicFailureCode') != user_disabled_codes[0]:
            raise ValueError('USER-001 refresh mapping differs from retained compiler artifact')
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
    source_before = harness.provenance(ROOT)['files']
    safe_run(['cargo', 'build', '--locked', '-p', 'jadpo-cli'], cwd=ROOT/'jadpo', env=environment)
    binding = harness.provenance(ROOT)
    if binding['files'] != source_before:
        raise ValueError('Source changed during compiler build')
    project = output/'application'
    shutil.copytree(ROOT/'examples/golden-todo-migration', project, ignore=shutil.ignore_patterns('build'))
    safe_run([str(ROOT/'jadpo/target/debug/jadpo'), 'build', str(project)], env=environment)
    generated = hashes(project/'build')
    target = project/'build/target'
    original = (target/'persistence.ts').read_text()
    declaration = 'import { SQL } from "bun";'
    if original.count(declaration) != 1:
        raise ValueError('Generated SQL instrumentation import changed')
    (output/'original-persistence.ts').write_text(original)
    (target/'persistence.ts').write_text(original.replace(declaration,
        'import { SQL } from ' + json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';'))
    authentication = (target/'first-party-authentication.ts').read_text()
    generated_authentication = authentication
    entry = 'async authenticate(request: Request, freshAuthority: boolean, operationNow: number, mutates = false): Promise<AuthPrincipal> {'
    if authentication.count(entry) != 1:
        raise ValueError('Generated authentication instrumentation entry changed')
    authenticate_start = authentication.index(entry)
    refresh_entry = '    async refresh(strategyName: string, credential: string, operationNow: number) {'
    if authentication.count(refresh_entry) != 1:
        raise ValueError('Generated authentication method boundary changed')
    authenticate_end = authentication.index(refresh_entry, authenticate_start)
    authenticate_method = authentication[authenticate_start:authenticate_end]
    external_return = 'if (externallyVerified) return principal;'
    final_return = '\n      return principal;\n    },\n'
    if authenticate_method.count(external_return) != 1 or authenticate_method.count(final_return) != 1:
        raise ValueError('Generated successful-principal return boundaries changed')
    authenticate_method = authenticate_method.replace(
        external_return, 'if (externallyVerified) { recordAuthenticatedPrincipal(principal); return principal; }')
    authenticate_method = authenticate_method.replace(
        final_return, '\n      recordAuthenticatedPrincipal(principal);\n      return principal;\n    },\n')
    authentication = authentication[:authenticate_start] + authenticate_method + authentication[authenticate_end:]
    exchange_entry = 'async exchangeServiceCredential(strategyName: string, credential: string, operationNow: number) {'
    if authentication.count(exchange_entry) != 1:
        raise ValueError('Generated service exchange instrumentation entry changed')
    (output/'original-authentication.ts').write_text(generated_authentication)
    (target/'first-party-authentication.ts').write_text('import { recordAuthenticationAttempt, recordAuthenticatedPrincipal } from ' +
        json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';\n' +
        authentication.replace(entry, entry + '\n      recordAuthenticationAttempt();').replace(
            exchange_entry, exchange_entry + '\n      recordAuthenticationAttempt();'))
    (target/'node_modules').symlink_to(dependencies/'node_modules', target_is_directory=True)
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
    report = harness.run_cases(ROOT)
    apply_checkpoint(report, outputs, ROOT)
    report['checkpoint'] = {'scope': f'{len(SUPPORTED)}-ID migrated application checkpoint; original frozen source is not executing',
                            'supported_ids': list(SUPPORTED), 'fresh_build_command': ['cargo', 'build', '--locked', '-p', 'jadpo-cli'],
                            'generated_original': generated, 'generated_instrumented': hashes(project/'build', ('node_modules',)),
                            'raw_files': {f'{backend}-raw.json': harness.digest(output/f'{backend}-raw.json') for backend in harness.BACKENDS},
                            'dependency_files': hashes(dependencies/'node_modules/jose'),
                            'remaining': 'Independent checkpoint review; all unobserved IDs and RM-108/402/403/full golden prerequisite gates remain'}
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
