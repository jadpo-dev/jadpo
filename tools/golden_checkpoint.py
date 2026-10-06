#!/usr/bin/env python3
"""Supporting eight-ID HTTP checkpoint; does not activate the final golden gate."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile

import golden_cases as harness

ROOT = Path(__file__).resolve().parents[1]
SUPPORTED = ('AUTH-001', 'AUTH-005', 'AUTH-012', 'PUBLIC-001', 'CREATE-002',
             'CREATE-004', 'READ-002', 'READ-003')


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


def observations(case: dict, raw: list) -> list:
    """Derive only fields supported by retained real response/statement evidence."""
    if case['id'] not in SUPPORTED or case.get('fixtureVariants') or case.get('followUps'):
        raise ValueError('Unsupported case/scenario')
    expected_requests = case.get('requests', [case.get('request')])
    replies = [item for item in raw if 'method' in item]
    if len(replies) != len(expected_requests):
        raise ValueError('Missing request observation')
    for request, reply in zip(expected_requests, replies):
        path = request['path'].replace('/todos/t2', '/todos/00000000-0000-4000-8000-000000000022')
        if request['method'] != reply['method'] or path != reply['path']:
            raise ValueError('Foreign request observation')
        if not harness.same_json(request.get('auth'), reply.get('auth')) or not harness.same_json(request.get('json'), reply.get('requestJson')):
            raise ValueError('Changed request credential selection/body')
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
        code = reply['body'].get('error', {}).get('code')
        if code is not None:
            step['code'] = code
        if case['id'] in ('AUTH-012', 'PUBLIC-001'):
            step['body'] = reply['body']
        if case['id'] == 'CREATE-004':
            persisted = [item.get('persistence') for item in raw if 'persistence' in item]
            if len(persisted) != 1 or persisted[0]['id'] != reply['body'].get('id'):
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
            entry['reason'] = 'Unsupported by the reviewed eight-ID migrated HTTP checkpoint; full application/gate prerequisites remain'
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
    entry = 'async authenticate(request: Request, freshAuthority: boolean, operationNow: number, mutates = false): Promise<AuthPrincipal> {'
    if authentication.count(entry) != 1:
        raise ValueError('Generated authentication instrumentation entry changed')
    (output/'original-authentication.ts').write_text(authentication)
    (target/'first-party-authentication.ts').write_text('import { recordAuthenticationAttempt } from ' +
        json.dumps(str(ROOT/'tools/golden_sql_observer.ts')) + ';\n' +
        authentication.replace(entry, entry + '\n      recordAuthenticationAttempt();'))
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
    report['checkpoint'] = {'scope': 'Eight-ID migrated application checkpoint; original frozen source is not executing',
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
