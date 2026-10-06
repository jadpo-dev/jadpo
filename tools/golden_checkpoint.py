#!/usr/bin/env python3
"""Supporting migrated HTTP checkpoint; does not activate the final golden gate."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
import json
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
             'LIST-003', 'REL-001', 'DELETE-002')
ALIASES = {'alice': '00000000-0000-4000-8000-000000000001',
           'bob': '00000000-0000-4000-8000-000000000002',
           't1': '00000000-0000-4000-8000-000000000021',
           't2': '00000000-0000-4000-8000-000000000022'}


def request_path(path: str) -> str:
    for alias, value in ALIASES.items():
        path = path.replace('/todos/'+alias, '/todos/'+value)
        path = path.replace('/users/'+alias+'/todos', '/users/'+value+'/todos')
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


def todo_statements(reply: dict):
    return [row for row in reply['statements'] if row['operation'] == 'SELECT' and re.search(r'FROM\s+"todo"', row['sql'], re.I)]


def observed_order(statement: dict):
    match = re.search(r'ORDER BY\s+"created_at"\s+(ASC|DESC),\s*"id"\s+(ASC|DESC)\s+LIMIT', statement['sql'], re.I)
    return ['created_at '+match[1].lower(), 'id '+match[2].lower()] if match else []


def observations(case: dict, raw: list, clock: str | None = None) -> list:
    """Derive only fields supported by retained real response/statement evidence."""
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
            todo_id = ALIASES['t2' if case['id'] == 'PATCH-004' else 't1']
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
                           ('app.ts', 'original-app.ts')):
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
    plan_path = evidence/'application/build/persistence/entities.json'
    expected_plans = [plan for plan in json.loads(plan_path.read_text())['query_plans'] if plan.get('entity') == 'Todo' and plan.get('strategy') == 'indexed_keyset_page']
    for output in outputs:
        for item in output['results']:
            journals = [scenario['raw'] for scenario in item['scenarios']] if item['id'] == 'REL-001' else [item['raw']]
            for raw in journals:
                for event in raw:
                    if 'queryMetadata' in event and not harness.same_json(event['queryMetadata'], expected_plans):
                        raise ValueError('Raw query metadata differs from generated artifact')
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
    entry = 'async authenticate(request: Request, freshAuthority: boolean, operationNow: number, mutates = false): Promise<AuthPrincipal> {'
    if authentication.count(entry) != 1:
        raise ValueError('Generated authentication instrumentation entry changed')
    (output/'original-authentication.ts').write_text(authentication)
    (target/'first-party-authentication.ts').write_text('import { recordAuthenticationAttempt } from ' +
        json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';\n' +
        authentication.replace(entry, entry + '\n      recordAuthenticationAttempt();'))
    application = (target/'app.ts').read_text()
    if application.count('function validate_TodoView(') != 1 or application.count('function validate_UserWithTodos(') != 1:
        raise ValueError('Generated output validation instrumentation changed')
    (output/'original-app.ts').write_text(application)
    (target/'app.ts').write_text(application + '\nexport { validate_TodoView, validate_UserWithTodos };\n')
    (target/'node_modules').symlink_to(dependencies/'node_modules', target_is_directory=True)
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
    report = harness.run_cases(ROOT)
    apply_checkpoint(report, outputs, ROOT)
    report['checkpoint'] = {'scope': 'Migrated application checkpoint; original frozen source is not executing',
                            'supported_ids': list(SUPPORTED), 'fresh_build_command': ['cargo', 'build', '--locked', '-p', 'jadpo-cli'],
                            'generated_original': generated, 'generated_instrumented': instrumented_binding,
                            'raw_files': {f'{backend}-raw.json': harness.digest(output/f'{backend}-raw.json') for backend in harness.BACKENDS},
                            'dependency_files': dependency_binding,
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
