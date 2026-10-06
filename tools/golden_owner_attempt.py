"""Exact compiler-only protected-attempt observations; no runtime authority."""
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

import golden_cases as harness

ACTION = '''
    action checkpoint_reassign_owner(todo_id: Todo.id, owner_id: User.id)
        fails TodoNotFound, TodoMutationConflict -> Todo
    {
        var changed = attempt update required Todo {
            where: id == todo_id
            set: { owner_id: Todo.owner_id(owner_id) }
            missing: TodoNotFound { todo_id: todo_id }
            conflict: TodoMutationConflict
        }
        return changed
    }
'''
REFERENCE = 'tests/assurance/service-reference-mail-v0.1.json'


def source_hashes(project, runtime_files=()):
    return {name: harness.digest(project/name) for name in sorted(harness.source_paths(project, project, 'build')) if name not in runtime_files}


def attempted_source(authored):
    end = authored.rfind('}')
    if end < 0 or authored[end:].strip() != '}':
        raise ValueError('Changed checked Todo source instrumentation boundary')
    return authored[:end] + ACTION + authored[end:]


def request_for(case):
    operation = case['operation']['attemptOwnerChange']
    if set(operation) != {'todo', 'from', 'to'} or any(type(value) is not str for value in operation.values()):
        raise ValueError('Changed original owner-change operation')
    return {'entity': 'Todo', 'field': 'owner_id', 'callable': 'Todo.checkpoint_reassign_owner',
            'row': {'parameter': 'todo_id', 'nominal_type': 'Todo.id', 'alias': operation['todo']},
            'from': {'nominal_type': 'User.id', 'alias': operation['from']},
            'to': {'parameter': 'owner_id', 'nominal_type': 'User.id', 'alias': operation['to']}}


def canonical_digest(value):
    return 'sha256:' + hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False).encode()).hexdigest()


def observations(case, raw):
    if len(raw) != 1 or set(raw[0]) != {'protectedAttempt'}:
        raise ValueError('Missing/foreign exact compiler protected-attempt capture')
    capture = raw[0]['protectedAttempt']
    if set(capture) != {'classification', 'specification', 'request', 'artifact', 'compiler_sha256', 'baselineCheck', 'ordinaryAttemptCheck', 'nativeExecutions'}:
        raise ValueError('Foreign protected-attempt observation fields')
    expected_specification = {'fixture': case['fixture'], 'operation': case['operation']}
    if capture.get('classification') != 'compiler_only_protected_attempt' or not harness.same_json(capture.get('specification'), expected_specification) or not harness.same_json(capture.get('request'), request_for(case)):
        raise ValueError('Foreign protected-attempt supplied intent or fixture')
    artifact = capture.get('artifact', {})
    c = artifact.get('canonical', {})
    if set(artifact) != {'canonical', 'kind', 'schema_version', 'subject_digest'} or artifact.get('kind') != 'protected_change_attempt' or type(artifact.get('schema_version')) is not int or artifact['schema_version'] != 1 or artifact.get('subject_digest') != canonical_digest(c):
        raise ValueError('Changed compiler protected-attempt subject/digest')
    request = c.get('request', {})
    attempt = c.get('attempt', {})
    field = c.get('protected_field', {})
    if (c.get('kind') != 'protected_change_attempt' or c.get('canonicalization') != 'jadpo.protected-change-attempt.v1' or
            c.get('status') != 'local_inspection_non_releasable' or c.get('release_authority') is not False or c.get('release_equivalent_success') is not False or
            request.get('classification') != 'supplied_unverified' or request.get('live_identity_validation') != 'unavailable' or
            not harness.same_json(request.get('value'), capture['request']) or request.get('digest') != canonical_digest(capture['request']) or
            attempt.get('checked_after', 'missing') is not None or attempt.get('semantic_graph', 'missing') is not None or
            attempt.get('status') != 'frontend_rejected_unexecuted' or
            attempt.get('runtime_authority') != 'unavailable' or attempt.get('feasibility') != 'unavailable'):
        raise ValueError('Changed non-releasable compiler refusal/intent classification')
    if (field.get('entity') != capture['request']['entity'] or field.get('field') != capture['request']['field'] or
            field.get('immutable') is not True or field.get('status') != 'compiler_checked_baseline_contract' or
            field.get('declared_nominal_type') != capture['request']['to']['nominal_type'] or
            attempt.get('callable') != capture['request']['callable'] or
            not harness.same_json(attempt.get('refusal_diagnostic', {}).get('primary'), attempt.get('assignment', {}).get('field_location')) or
            attempt.get('refusal_diagnostic', {}).get('code') != 'TYPE_FIELD_UPDATE_FORBIDDEN' or
            not any(harness.same_json(row, attempt['refusal_diagnostic']) for row in attempt.get('diagnostics', []))):
        raise ValueError('Foreign checked protected field or genuine refusal span')
    if c.get('policy_rule') != field['entity'] + '.fields.' + field['field'] or type(c.get('disposition')) is not str:
        raise ValueError('Foreign compiler-derived field policy')
    commands = capture.get('nativeExecutions', [])
    if [row.get('name') for row in commands] != ['checked-baseline', 'ordinary-refused-attempt', 'protected-attempt', 'fresh-export-validation'] or not harness.same_json([row.get('exit_code') for row in commands], [0, 1, 0, 0]):
        raise ValueError('Missing real checked/refused/native renderer observation')
    if capture.get('baselineCheck', {}).get('status') != 'passed' or capture.get('ordinaryAttemptCheck', {}).get('status') != 'failed':
        raise ValueError('Changed checked baseline versus rejected attempt distinction')
    return [{'variant_index': 0, 'steps': [{'releaseEquivalentSuccess': c['release_equivalent_success'],
             'disposition': c['disposition'], 'policyRule': c['policy_rule'],
             'evidenceClassification': capture['classification'], 'liveIdentityValidation': request['live_identity_validation'],
             'releaseAuthority': c['release_authority']}]}]


def collect(root, evidence, project, case, environment):
    compiler = root/'jadpo/target/debug/jadpo'
    attempt = evidence/'owner-attempt-source'
    shutil.copytree(project, attempt, ignore=shutil.ignore_patterns('build'))
    source = attempt/'entities/todo.jadpo'
    source.write_text(attempted_source(source.read_text()))
    request = request_for(case)
    request_path = evidence/'owner-attempt-request.json'
    request_path.write_text(json.dumps(request, sort_keys=True)+'\n')
    records = []

    def execute(name, arguments, expected):
        run = subprocess.run([str(compiler), *map(str, arguments)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)
        (evidence/(name+'.stdout')).write_bytes(run.stdout)
        (evidence/(name+'.stderr')).write_bytes(run.stderr)
        if run.returncode != expected:
            raise ValueError('Native protected-attempt process failed: '+name+'; raw exception text withheld')
        records.append({'name': name, 'exit_code': run.returncode, 'stdout_sha256': hashlib.sha256(run.stdout).hexdigest(), 'stderr_sha256': hashlib.sha256(run.stderr).hexdigest()})
        return run.stdout

    checked = json.loads(execute('checked-baseline', ['check', project, '--diagnostic-format=json'], 0))
    refused = json.loads(execute('ordinary-refused-attempt', ['check', attempt, '--diagnostic-format=json'], 1))
    command = ['protected-attempt', attempt, '--before', project, '--request', request_path]
    exported = execute('protected-attempt', command, 0)
    export = evidence/'owner-attempt-artifact.json'
    export.write_bytes(exported)
    if execute('fresh-export-validation', [*command, '--validate-export', export], 0) != exported:
        raise ValueError('Native fresh renderer changed exact subject')
    capture = {'classification': 'compiler_only_protected_attempt', 'specification': {'fixture': deepcopy(case['fixture']), 'operation': deepcopy(case['operation'])},
               'request': request, 'artifact': json.loads(exported), 'compiler_sha256': harness.digest(compiler),
               'baselineCheck': checked, 'ordinaryAttemptCheck': refused, 'nativeExecutions': records}
    observations(case, [{'protectedAttempt': capture}])
    (evidence/'owner-attempt-probe.json').write_text(json.dumps(capture, indent=2, allow_nan=False)+'\n')
    return binding(evidence)


def binding(evidence):
    return {'sources': source_hashes(evidence/'owner-attempt-source'),
            'files': {name: harness.digest(evidence/name) for name in ('owner-attempt-request.json', 'owner-attempt-artifact.json', 'owner-attempt-probe.json',
                       'checked-baseline.stdout', 'checked-baseline.stderr', 'ordinary-refused-attempt.stdout', 'ordinary-refused-attempt.stderr',
                       'protected-attempt.stdout', 'protected-attempt.stderr', 'fresh-export-validation.stdout', 'fresh-export-validation.stderr')}}


def validate(root, evidence, case, recorded):
    if binding(evidence) != recorded:
        raise ValueError('Changed native protected-attempt inputs/observations')
    project = evidence/'application'
    expected = source_hashes(root/'examples/golden-todo-migration')
    expected[REFERENCE] = harness.digest(root/REFERENCE)
    # Only the declared SQLite runtime fixture resources are excluded. The
    # diagnosed attempt retains an exact inventory with no runtime exclusion.
    if source_hashes(project, ('checkpoint.sqlite', 'checkpoint.sqlite-journal', 'checkpoint.sqlite-wal', 'checkpoint.sqlite-shm')) != expected:
        raise ValueError('Protected-attempt baseline differs from current migrated source')
    expected_attempt = dict(expected)
    expected_attempt['entities/todo.jadpo'] = hashlib.sha256(attempted_source((project/'entities/todo.jadpo').read_text()).encode()).hexdigest()
    if source_hashes(evidence/'owner-attempt-source') != expected_attempt:
        raise ValueError('Protected-attempt source differs from exact diagnosed operation instrumentation')
    capture = json.loads((evidence/'owner-attempt-probe.json').read_text())
    observations(case, [{'protectedAttempt': capture}])
    if capture['compiler_sha256'] != harness.digest(root/'jadpo/target/debug/jadpo') or not harness.same_json(json.loads((evidence/'owner-attempt-request.json').read_text()), request_for(case)):
        raise ValueError('Changed protected-attempt compiler/request binding')
    for row in capture['nativeExecutions']:
        if row['stdout_sha256'] != harness.digest(evidence/(row['name']+'.stdout')) or row['stderr_sha256'] != harness.digest(evidence/(row['name']+'.stderr')):
            raise ValueError('Changed native protected-attempt command observation binding')
    if not harness.same_json(capture['baselineCheck'], json.loads((evidence/'checked-baseline.stdout').read_text())) or not harness.same_json(capture['ordinaryAttemptCheck'], json.loads((evidence/'ordinary-refused-attempt.stdout').read_text())):
        raise ValueError('Changed actual checked/refused compiler diagnostics')
    artifact_bytes = (evidence/'owner-attempt-artifact.json').read_bytes()
    if any((evidence/name).read_bytes() != artifact_bytes for name in ('protected-attempt.stdout', 'fresh-export-validation.stdout')):
        raise ValueError('Changed native protected-attempt emitted/export bytes')
    if not harness.same_json(json.loads(artifact_bytes), capture['artifact']):
        raise ValueError('Native protected-attempt raw subject differs from export')
    environment = {key: value for key, value in os.environ.items() if key not in ('DATABASE_URL', 'SQLITE_PATH', 'JADPO_BIN', 'JADPO_DEBUG_TARGET_STACKS')}
    run = subprocess.run([str(root/'jadpo/target/debug/jadpo'), 'protected-attempt', str(evidence/'owner-attempt-source'), '--before', str(project), '--request', str(evidence/'owner-attempt-request.json'), '--expected-before', capture['artifact']['canonical']['before']['source_digest'], '--expected-before-state', capture['artifact']['canonical']['before']['state_digest'], '--validate-export', str(evidence/'owner-attempt-artifact.json')], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)
    if run.returncode != 0 or run.stdout != artifact_bytes:
        raise ValueError('Fresh native compiler refused rebound protected-attempt subject')
    return capture
