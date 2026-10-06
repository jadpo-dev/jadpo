#!/usr/bin/env python3
"""Prepare/run source-bound golden cases; no application adapters exist yet.

Driver results are observations, never supplied pass labels. This protocol is
supporting infrastructure, not evidence that an adapter observed real behaviour.
The full golden verifier remains separately gated until adapters are reviewed.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
ACCEPTANCE = 'examples/golden-todo/acceptance.json'
OBLIGATIONS = 'tests/validation/golden-obligations.json'
BACKENDS = ('sqlite', 'postgres')
KINDS = {'http', 'job', 'persistence', 'policy', 'preflight', 'runtime'}
SCHEMA_VERSION = 1


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_contract(root: Path) -> dict:
    contract = json.loads((root / ACCEPTANCE).read_text())
    cases = contract['cases']
    ids = [case['id'] for case in cases]
    if not cases or len(ids) != len(set(ids)):
        raise ValueError('Acceptance IDs must be unique and nonempty')
    obligations = json.loads((root / OBLIGATIONS).read_text())
    if set(ids) != set(obligations['cases']):
        raise ValueError('Acceptance IDs differ from the obligation map')
    if obligations['acceptance_sha256'] != digest(root / ACCEPTANCE):
        raise ValueError('Acceptance digest differs from the obligation map')
    for case in cases:
        if case['kind'] not in KINDS or not isinstance(case.get('expect'), dict) or not case['expect']:
            raise ValueError(f'{case["id"]}: missing expectations or unsupported kind')
        if 'fixtureVariants' in case and (not case['fixtureVariants'] or
                len(case['fixtureVariants']) != len(set(case['fixtureVariants']))):
            raise ValueError(f'{case["id"]}: fixture variants must be unique and nonempty')
    return contract


def provenance(root: Path) -> dict:
    """Bind every authored project input and compiler implementation, not Git HEAD.

Uncommitted edits and adding/removing files therefore invalidate an old result.
Generated output is excluded; application adapters must retain its own digest/raw
observations once implemented. A missing built compiler is explicitly unknown.
"""
    paths = {ACCEPTANCE, OBLIGATIONS, 'tools/golden_cases.py'}
    # Supporting checkpoint adapters/instrumentation are source inputs too.
    # They are deliberately not registered as the complete golden drivers.
    for name in ('tools/golden_checkpoint.py', 'tools/golden_http_checkpoint.ts',
                 'tools/golden_sql_observer.ts'):
        if (root / name).is_file():
            paths.add(name)
    for directory in ('examples/golden-todo', 'examples/golden-todo-migration'):
        paths.update(source_paths(root, root / directory, 'build'))
    paths.update(source_paths(root, root / 'jadpo', 'target', ('.pyc', '.pyo')))
    compiler = root / 'jadpo/target/debug/jadpo'
    return {'files': {name: digest(root / name) for name in sorted(paths)},
            'compiler_sha256': digest(compiler) if compiler.is_file() else None}


def source_paths(root: Path, directory: Path, excluded: str, suffixes=()):
    # Prune generated trees before traversal. Filtering rglob's results still
    # walks every compiler cache file on each provenance check.
    for current, directories, files in os.walk(directory):
        directories[:] = [name for name in directories if name != excluded]
        for name in files:
            path = Path(current) / name
            if path.is_file() and name != excluded and path.suffix not in suffixes:
                yield str(path.relative_to(root))


def scenarios(case: dict) -> list[dict]:
    """Retain the frozen whole case, including multi-request semantics, per variant.

Follow-ups remain sequenced inside that same scenario. The driver is responsible
for fixture reset/isolation and injected clock transitions; this layer demands
all observations and never infers them from a related unit suite.
"""
    variants = case.get('fixtureVariants', [None])
    return [{'variant_index': i, 'fixture_variant': variant,
             'expectations': [case['expect'], *[step['expect'] for step in case.get('followUps', [])]]}
            for i, variant in enumerate(variants)]


def same_json(left, right) -> bool:
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(same_json(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(same_json(a, b) for a, b in zip(left, right))
    return left == right


def compare(expected: dict, observed: dict) -> list[str]:
    failures = []
    if not isinstance(observed, dict):
        return ['Observation must be an object']
    for key, value in expected.items():
        observation_key = key[:-6] if key.endswith('AtMost') else key
        if observation_key not in observed:
            failures.append(f'Missing observation: {observation_key}')
            continue
        actual = observed[observation_key]
        if key.endswith('AtMost'):
            # The frozen AtMost obligations count physical database queries.
            # Fractional observations cannot establish a count budget.
            matches = (type(actual) is int and actual >= 0 and actual <= value)
        else:
            # Python considers True == 1; JSON contracts do not.
            matches = same_json(actual, value)
        if not matches:
            failures.append(f'Expectation mismatch: {key}')
    return failures


def assess(case: dict, observations: list) -> list[str]:
    required = scenarios(case)
    if not isinstance(observations, list) or len(observations) != len(required):
        return ['Missing or extra fixture-variant observations']
    failures = []
    for plan, observation in zip(required, observations):
        if not isinstance(observation, dict) or type(observation.get('variant_index')) is not int or observation['variant_index'] != plan['variant_index']:
            failures.append('Foreign, duplicate or reordered fixture variant')
            continue
        phases = observation.get('steps')
        if not isinstance(phases, list) or len(phases) != len(plan['expectations']):
            failures.append(f'Variant {plan["variant_index"]}: missing or extra follow-up observations')
            continue
        for index, (expected, actual) in enumerate(zip(plan['expectations'], phases)):
            failures.extend(f'Variant {plan["variant_index"]}, step {index}: {message}'
                            for message in compare(expected, actual))
    return failures


def run_cases(root: Path, drivers: dict[tuple[str, str], Callable] | None = None) -> dict:
    """Execute registered adapters, otherwise emit explicit pending entries.

Drivers keyed by (backend, kind) receive a copy of the entire frozen case and
must return ordered variant observations with ordered primary/follow-up steps.
This repository deliberately registers no adapters until application composition
and reviewed observation instrumentation are available. No JSON pass import API
is offered. Driver failures cannot delete a case or abort later inventory entries.
"""
    contract = read_contract(root)
    binding = provenance(root)
    drivers = drivers or {}
    results = []
    for backend in BACKENDS:
        for case in contract['cases']:
            entry = {'id': case['id'], 'backend': backend, 'kind': case['kind'],
                     'required_scenarios': scenarios(case), 'status': 'not_run'}
            driver = drivers.get((backend, case['kind']))
            if driver is None:
                entry['reason'] = 'Source-bound application adapter is not implemented'
            elif binding['compiler_sha256'] is None:
                entry['reason'] = 'Built compiler is missing; execution cannot be source-bound'
            else:
                try:
                    observations = driver(deepcopy(case), backend)
                    # Reject non-JSON results before retaining any observations.
                    json.dumps(observations, allow_nan=False)
                    entry['observations'] = observations
                    entry['failures'] = assess(case, observations)
                    entry['status'] = 'failed' if entry['failures'] else 'passed'
                except Exception as error:
                    # Adapter exceptions might include credentials/provider text.
                    entry['status'] = 'failed'
                    entry['failures'] = [f'Adapter exception ({type(error).__name__}); no raw exception text retained']
            results.append(entry)
    if provenance(root) != binding:
        raise ValueError('Source or compiler changed during the harness run')
    return {'schema_version': SCHEMA_VERSION, 'contract_version': contract.get('contractVersion'),
            'provenance': binding, 'results': results, 'release_equivalent': False,
            'status': 'observations_passed' if all(r['status'] == 'passed' for r in results)
                      else 'failed' if any(r['status'] == 'failed' for r in results) else 'not_run',
            'remaining_gate': 'Reviewed application adapters, artifact/audit agreement and full require-golden integration'}


def validate_report(report: dict, root: Path) -> None:
    """Reject stale/missing/foreign/tampered supporting results; not attestation."""
    contract = read_contract(root)
    if (report.get('schema_version') != SCHEMA_VERSION or report.get('provenance') != provenance(root)
            or report.get('contract_version') != contract.get('contractVersion')):
        raise ValueError('Stale or foreign harness provenance/schema')
    expected = {(backend, case['id']): case for backend in BACKENDS for case in contract['cases']}
    results = report.get('results', [])
    keys = [(entry.get('backend'), entry.get('id')) for entry in results]
    if len(keys) != len(set(keys)) or set(keys) != set(expected):
        raise ValueError('Missing, duplicate or foreign case/backend results')
    for key, entry in zip(keys, results):
        case = expected[key]
        if entry.get('kind') != case['kind'] or entry.get('required_scenarios') != scenarios(case):
            raise ValueError('Changed case expectations or fixture scenarios')
        status = entry.get('status')
        if status == 'passed':
            if report['provenance']['compiler_sha256'] is None or assess(case, entry.get('observations')) or entry.get('failures') != []:
                raise ValueError('Passing label lacks complete matching observations')
        elif status == 'failed':
            if not isinstance(entry.get('failures'), list) or not entry['failures']:
                raise ValueError('Failed result requires retained failure disposition')
        elif status == 'not_run':
            if not entry.get('reason') or 'observations' in entry:
                raise ValueError('Unexecuted result requires reason and cannot claim observations')
        else:
            raise ValueError('Unknown case status')
    aggregate = 'observations_passed' if all(r['status'] == 'passed' for r in results) else 'failed' if any(r['status'] == 'failed' for r in results) else 'not_run'
    if report.get('status') != aggregate or report.get('release_equivalent') is not False:
        raise ValueError('Invalid aggregate status or release claim')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New report path; existing evidence is never overwritten')
    args = parser.parse_args()
    report = run_cases(ROOT)
    validate_report(report, ROOT)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as output:
        json.dump(report, output, indent=2, allow_nan=False)
        output.write('\n')
    print(f'{report["status"]}: {len(report["results"])} case/backend entries; report {args.output}')
    # Pending adapters are not a passing acceptance command.
    return 0 if report['status'] == 'observations_passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
