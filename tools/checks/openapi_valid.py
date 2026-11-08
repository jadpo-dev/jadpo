#!/usr/bin/env python3
"""Every OpenAPI document Jadpo writes is valid OpenAPI 3.1
(docs/leaving-jadpo.md; area 7, "OpenAPI").

Copies the golden todo (benchmarks/golden-todo/) and each teaching example
into a folder of their own, runs `jadpo check` in each, which writes
`build/openapi/<api>.json`, and checks every document it wrote, with the
golden copy `crates/jadpo-cli/tests/goldens/todos.openapi.json`, against the
OpenAPI specification's own JSON Schema. The validator is pinned in
tools/fixtures/openapi/package-lock.json and installed there by
`npm ci --ignore-scripts` the first time.

  JADPO_BIN=target/debug/jadpo python3 tools/checks/openapi_valid.py
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'tools/fixtures/openapi'
VALIDATOR = '@seriousme/openapi-schema-validator'


def installed() -> bool:
    """Whether the pinned validator is in tools/fixtures/openapi/."""
    lock = json.loads((TOOLS / 'package-lock.json').read_text())
    pinned = lock['packages'][f'node_modules/{VALIDATOR}']['version']
    found = TOOLS / 'node_modules' / VALIDATOR / 'package.json'
    return found.is_file() and json.loads(found.read_text()).get('version') == pinned


def install() -> None:
    if installed():
        return
    subprocess.run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'],
                   cwd=TOOLS, check=True, stdout=subprocess.DEVNULL)
    if not installed():
        sys.exit(f'npm ci left no {VALIDATOR} in tools/fixtures/openapi/')


def projects() -> list[Path]:
    """The golden todo and every teaching example."""
    examples = sorted(path.parent for path in (ROOT / 'examples').glob('*/app.jadpo'))
    return [ROOT / 'benchmarks/golden-todo', *examples]


def documents(jadpo: str, work: Path) -> list[Path]:
    """Each project checked in a copy of its own, and the documents it wrote."""
    found = []
    for project in projects():
        copy = work / project.name
        shutil.copytree(project, copy, ignore=shutil.ignore_patterns('typescript', 'build', '.jadpo'))
        checked = subprocess.run([jadpo, 'check'], cwd=copy, capture_output=True, text=True)
        if checked.returncode:
            sys.exit(f'`jadpo check` failed in {project.relative_to(ROOT)}:\n{checked.stdout}{checked.stderr}')
        found += sorted(path for path in (copy / 'build/openapi').glob('*.json')
                        if '.failures.' not in path.name)
    return found


def main() -> int:
    jadpo = os.environ.get('JADPO_BIN', str(ROOT / 'target/debug/jadpo'))
    install()
    with tempfile.TemporaryDirectory(prefix='openapi-') as temporary:
        found = [ROOT / 'crates/jadpo-cli/tests/goldens/todos.openapi.json', *documents(jadpo, Path(temporary))]
        if len(found) < 2:
            sys.exit('no project wrote an OpenAPI document')
        checked = subprocess.run(['node', str(TOOLS / 'validate.mjs'), *map(str, found)],
                                 cwd=TOOLS, capture_output=True, text=True)
        if checked.returncode:
            print(checked.stdout + checked.stderr)
            return 1
    print(f'passed: {len(found)} OpenAPI documents are valid OpenAPI 3.1')
    return 0


if __name__ == '__main__':
    sys.exit(main())
