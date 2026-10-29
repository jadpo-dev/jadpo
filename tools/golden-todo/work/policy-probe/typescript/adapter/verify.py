#!/usr/bin/env python3
"""Private gate entry: locked install, static checks and real Node/Bun conformance."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

import run as harness
from support import dependency_lock

PROJECT = Path(__file__).resolve().parent.parent
ROOT = PROJECT.parents[2]


def run(command, **kwargs):
    return subprocess.run(command, check=True, stdout=sys.stderr, **kwargs)


def install_dependencies(project):
    run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=project)


def check_versions():
    for runtime, prefix, declaration in [('node', 'v', '.node-version'), ('bun', '', '.bun-version')]:
        expected = prefix + (PROJECT / declaration).read_text().strip()
        if harness.runtime_version(runtime) != expected:
            raise RuntimeError(f'{runtime} must match {declaration}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--contract-source', type=Path, default=ROOT)
    args = parser.parse_args()
    source = args.contract_source.resolve()
    with dependency_lock(PROJECT):
        try:
            # Missing own prerequisites are a red result with all 88 obligations,
            # before installation or any database/provider work.
            harness.load_registry(source)
            check_versions()
            install_dependencies(PROJECT)
            run([sys.executable, str(PROJECT / 'adapter/verification_test.py')], cwd=PROJECT)
            for name in ['check', 'lint', 'format:check']:
                run(['npm', 'run', name], cwd=PROJECT)
            result = harness.verify_contract(source, ['node', 'bun'], unit=True)
        except Exception as error:
            result = harness.unavailable_report(source, ['node', 'bun'], harness.CASE_IDS, str(error))
    print(json.dumps(result, indent=2))
    return bool(result['counts']['failed'])


if __name__ == '__main__':
    raise SystemExit(main())
