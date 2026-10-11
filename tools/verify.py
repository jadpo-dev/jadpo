#!/usr/bin/env python3
"""Run supported-language checks and report the separate golden application gate."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'tests/validation/manifest.json'
COMPILER = ROOT / 'jadpo/target/debug/jadpo'


def inventory(manifest):
    actual = {str(p.relative_to(ROOT)) for p in (ROOT / 'tests/runtime').rglob('*.test.ts')}
    registered = manifest['runtime_suites'] + list(manifest['postgres_suites'])
    if len(registered) != len(set(registered)) or actual != set(registered):
        raise ValueError(f'Runtime suite inventory differs: missing={sorted(actual-set(registered))}, stale/duplicate={sorted(set(registered)-actual)}')
    projects = {str(p.relative_to(ROOT)) for p in (ROOT / 'examples').iterdir() if p.is_dir() and any('build' not in q.relative_to(p).parts for q in p.rglob('*.jadpo'))}
    if projects != set(manifest['examples']):
        raise ValueError('Every example project must be explicitly classified in the validation manifest')
    for name, entry in manifest['examples'].items():
        if entry['status'] not in ('runnable', 'design_contract', 'legacy') or not entry['reason']:
            raise ValueError(f'Invalid example classification: {name}')
    acceptance = json.loads((ROOT / 'examples/golden-todo/acceptance.json').read_text())
    obligations = json.loads((ROOT / 'tests/validation/golden-obligations.json').read_text())
    if {c['id'] for c in acceptance['cases']} != set(obligations['cases']):
        raise ValueError('Every golden acceptance case must have an explicit evidence/gap entry')
    for case_id, evidence in obligations['cases'].items():
        if evidence.get('status') not in ('not_executed', 'contract_conflict', 'needs_clarification') or not evidence.get('gap'):
            raise ValueError(f'{case_id}: unexecuted obligation requires a gap, not a passing-evidence label')
    digest = hashlib.sha256((ROOT / 'examples/golden-todo/acceptance.json').read_bytes()).hexdigest()
    if obligations['acceptance_sha256'] != digest:
        raise ValueError('Golden acceptance changed: review the evidence/gap map before updating its digest')
    return obligations


def execute(command, log, environment, timeout=900):
    """Timeout the process group so a failed suite cannot leave a server running."""
    with log.open('w') as output:
        process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            return process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise


def verification_environment():
    """Never let caller database/output overrides reach a validation subprocess."""
    environment = os.environ.copy()
    for name in ['DATABASE_URL', 'SQLITE_PATH', 'JADPO_AUTH_TEST_DATABASE_URL', 'JADPO_SERVICE_AUTH_DATABASE_URL', 'JADPO_JWT_AUTH_DATABASE_URL', 'JADPO_JWT_DEPENDENCY_DIR', 'JADPO_VALIDATION_PERSISTENCE_DATABASE_URL', 'JADPO_VALIDATION_AUTH_POLICY_DATABASE_URL', 'JADPO_LIFECYCLE_DATABASE_URL', 'JADPO_READINESS_TEST_DATABASE_URL', 'JADPO_READINESS_AUTH_DATABASE_URL', 'JADPO_READINESS_RECOVERY_DATABASE_URL', 'JADPO_READINESS_CONTROL_DATABASE_URL', 'JADPO_DELIVERY_HOOK_DATABASE_URL', 'JADPO_DELIVERY_HOOK_COMPONENT_OUTPUT', 'JADPO_DEBUG_TARGET_STACKS', 'JADPO_BIN']:
        environment.pop(name, None)
    environment['JADPO_BIN'] = str(COMPILER)
    environment['NO_COLOR'] = '1'
    environment['PYTHONDONTWRITEBYTECODE'] = '1'
    environment.pop('JADPO_DELIVERY_HOOK_COMPONENT_VARIANT', None)
    return environment


def collect_golden_case_report(output, environment):
    """Retain the reviewed protocol inventory separately from supported passes.

    Pending adapters deliberately return one. A crashed command or contradictory
    exit/status is an infrastructure failure, never an accepted pending result.
    The protocol command validates its source-bound report before writing it.
    """
    path = output / 'golden-cases.json'
    if path.exists():
        raise ValueError('Golden case report already exists; fresh evidence is required')
    command = [sys.executable, 'tools/golden_cases.py', '--output', str(path)]
    checkpoint_directory = output / 'golden-http-checkpoint'
    if checkpoint_directory.is_dir():
        command += ['--checkpoint-dir', str(checkpoint_directory),
                    '--jwt-dependencies', str(ROOT/'build/validation/jwt-dependencies')]
    code = execute(command,
                   output / 'golden-cases.log', environment)
    cases = json.loads(path.read_text())
    if (cases.get('status') not in ('not_run', 'failed', 'observations_passed')
            or cases.get('release_equivalent') is not False
            or code != (0 if cases['status'] == 'observations_passed' else 1)):
        raise ValueError('Golden case command failed or contradicted its supporting report')
    spec = importlib.util.spec_from_file_location('jadpo_verify_golden_cases', ROOT/'tools/golden_cases.py')
    if spec is None or spec.loader is None:
        raise ValueError('Golden report validator is unavailable')
    cases_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cases_module)
    cases_module.validate_report(cases, ROOT)
    counts = {status: sum(entry['status'] == status for entry in cases['results'])
              for status in ('passed', 'failed', 'not_run')}
    source_bound = isinstance(cases.get('driver_evidence'), dict)
    if cases['status'] == 'observations_passed' and (len(cases['results']) != 88 or not source_bound):
        raise ValueError('Passing Golden results lack complete source-bound application evidence')
    return {'report': path.name, 'status': cases['status'],
            'case_backend_entries': len(cases['results']),
            'counts': counts, 'source_bound': source_bound}


def golden_behavioral_gate_passes(golden):
    """The opt-in gate means all frozen behavioural entries executed and passed."""
    protocol = golden.get('case_protocol', {})
    counts = protocol.get('counts', {})
    return (golden.get('status') == 'compiled'
            and protocol.get('status') == 'observations_passed'
            and protocol.get('case_backend_entries') == 88
            and protocol.get('source_bound') is True
            and counts == {'passed': 88, 'failed': 0, 'not_run': 0})


def collect_golden_source_report(output, environment, obligations):
    """Check the executable successor and retain the frozen pressure-source diagnostics separately."""
    checks = {}
    for name, project, report_name in (
            ('accepted_successor', 'examples/golden-todo-migration', 'golden-diagnostics.json'),
            ('historical_pressure_source', 'examples/golden-todo', 'golden-historical-pressure-diagnostics.json')):
        result = subprocess.run([str(COMPILER), 'check', project, '--diagnostic-format=json'],
                                 cwd=ROOT, env=environment, capture_output=True, text=True, timeout=60)
        try:
            diagnostic_report = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            raise ValueError(f'{project} compiler check did not produce a diagnostic report') from error
        (output/report_name).write_text(result.stdout)
        checks[name] = {
            'source': project,
            'status': 'compiled' if result.returncode == 0 and diagnostic_report.get('status') == 'passed' else 'compile_failed',
            'diagnostic_count': len(diagnostic_report.get('diagnostics', [])),
            'report': report_name,
        }
    accepted = checks['accepted_successor']
    return {
        'status': accepted['status'],
        'source': accepted['source'],
        'diagnostic_count': accepted['diagnostic_count'],
        'historical_pressure_source': checks['historical_pressure_source'],
        'behavioural_evidence': 'not_run',
        'obligations': obligations['cases'],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=['full', 'quick'], default='full', help='quick explicitly omits live PostgreSQL')
    parser.add_argument('--require-golden', action='store_true', help='also fail until the full golden app has executable behavioural evidence')
    args = parser.parse_args()
    manifest = json.loads(MANIFEST.read_text())
    obligations = inventory(manifest)
    required = ['cargo', 'rustc', 'bun', 'node', 'ruby', 'bash']
    if args.profile == 'full':
        required += ['initdb', 'pg_ctl', 'createdb', 'postgres']
    missing = [tool for tool in required if not shutil.which(tool)]
    if missing:
        parser.error('Required tools missing from PATH: ' + ', '.join(missing))
    # New results directory per run: failure cannot leave a previous green report.
    output = ROOT / 'build/validation' / (time.strftime('%Y%m%dT%H%M%S') + f'-{os.getpid()}')
    output.mkdir(parents=True)
    environment = verification_environment()
    report = {'schema_version': 1, 'profile': args.profile, 'status': 'running', 'steps': [],
              'release_equivalent': False, 'golden': {'status': 'not_run'},
              'toolchain': {}, 'examples': manifest['examples'], 'remaining_gates': manifest['remaining_gates']}
    for tool in required:
        if tool not in ('initdb', 'pg_ctl', 'createdb'):
            result = subprocess.run([tool, '--version'], capture_output=True, text=True, env=environment)
            report['toolchain'][tool] = (result.stdout or result.stderr).splitlines()[0]
    def save():
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    def step(name, command):
        entry = {'name': name, 'command': command, 'status': 'running', 'log': f'{len(report["steps"]):02d}-{name}.log'}
        report['steps'].append(entry)
        save()
        print(f'RUN  {name}', flush=True)
        started = time.monotonic()
        try:
            code = execute(command, output / entry['log'], environment)
        except BaseException:
            entry['status'] = 'interrupted_or_timed_out'
            save()
            raise
        entry.update(status='passed' if code == 0 else 'failed', exit_code=code,
                     seconds=round(time.monotonic()-started, 3))
        save()
        if code != 0:
            print((output / entry['log']).read_text(errors='replace')[-6000:], file=sys.stderr)
            raise RuntimeError(f'{name} failed (exit {code})')
        print(f'PASS {name}', flush=True)
    save()
    try:
        step('verification-contract', [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests/validation', '-p', 'test_*.py'])
        step('rust-workspace', ['cargo', 'test', '--locked', '--manifest-path', 'jadpo/Cargo.toml', '--workspace'])
        step('compiler-build', ['cargo', 'build', '--locked', '--manifest-path', 'jadpo/Cargo.toml', '-p', 'jadpo-cli'])
        step('compile-fixtures', ['ruby', 'tests/compile/verify.rb'])
        step('language-docs', [sys.executable, 'tools/language_docs.py', '--check'])
        step('editor', ['node', '--test', *[str(p.relative_to(ROOT)) for p in sorted((ROOT/'editors/vscode/test').glob('*.test.js'))]])
        step('candidate-contract', [sys.executable, 'tools/verify-p10r.py'])
        for name, entry in manifest['examples'].items():
            if entry['status'] == 'runnable':
                step('build-' + Path(name).name, [str(COMPILER), 'build', name])
        for name in manifest['build_fixtures']:
            step('build-' + Path(name).stem, [str(COMPILER), 'build', name])
        step('authored-tests', [str(COMPILER), 'test', 'examples/test-fixtures'])
        step('jwt-explicit-dependency-install', [sys.executable, 'tools/install-jwt-dependency.py'])
        for name in manifest['runtime_suites']:
            step(Path(name).name.removesuffix('.test.ts'), ['bun', '--no-install', f'--env-file={os.devnull}', 'test', name])
        for mode in [*manifest['postgres_suites'].values(), *manifest['additional_postgres_modes']]:
            if args.profile == 'full':
                step('postgres-' + mode, ['bash', 'tests/runtime/postgres.sh', mode])
            else:
                report['steps'].append({'name': 'postgres-' + mode, 'status': 'skipped', 'reason': 'Explicit quick profile'})
        # Supporting migrated HTTP evidence remains separate from the frozen source gate.
        checkpoint_summary = None
        if args.profile == 'full':
            checkpoint_directory = output / 'golden-http-checkpoint'
            step('golden-http-checkpoint', [sys.executable, 'tools/golden_checkpoint.py',
                                          '--output', str(checkpoint_directory)])
            checkpoint_report = json.loads((checkpoint_directory / 'golden-checkpoint.json').read_text())
            checkpoint_summary = {
                'report': 'golden-http-checkpoint/golden-checkpoint.json',
                'scope': checkpoint_report['checkpoint']['scope'],
                'status': checkpoint_report['status'],
                'release_equivalent': False,
                'counts': {status: sum(entry['status'] == status for entry in checkpoint_report['results'])
                           for status in ('passed', 'failed', 'not_run')},
            }
        else:
            report['steps'].append({'name': 'golden-http-checkpoint', 'status': 'skipped',
                                    'reason': 'Explicit quick profile omits this two-backend checkpoint'})
        # The frozen pressure source remains historical; the accepted migration
        # is the app the supporting checkpoint actually builds and executes.
        report['golden'] = collect_golden_source_report(output, environment, obligations)
        report['golden']['case_protocol'] = collect_golden_case_report(output, environment)
        if checkpoint_summary is not None:
            report['golden']['http_checkpoint'] = checkpoint_summary
        if args.require_golden and not golden_behavioral_gate_passes(report['golden']):
            raise RuntimeError('Golden behavioural acceptance is incomplete; see golden-diagnostics.json and the source-bound case report')
        report['status'] = 'supported_checks_passed_with_open_gates'
        print('OPEN full golden application, behavioural acceptance and other recorded release gates', flush=True)
    except (Exception, KeyboardInterrupt) as error:
        report['status'] = 'failed'
        report['failure'] = str(error) or 'Interrupted'
        print(f'FAIL {report["failure"]}', file=sys.stderr)
    finally:
        save()
        print(f'Report: {output / "report.json"}', flush=True)
    return 1 if report['status'] == 'failed' else 0


if __name__ == '__main__':
    raise SystemExit(main())
