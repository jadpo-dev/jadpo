#!/usr/bin/env python3
"""Check the teaching examples in examples/ (docs/compiler-runtime.md §2.7).

Each project in examples/ (a folder holding app.jadpo; golden-todo/ holds
only the golden app's contract) is copied to a temporary folder, so the
manifest `jadpo check` writes never lands in the repository, and must:

- check with no diagnostics at all, errors or warnings (`jadpo check --json`);
- be canonically formatted (`jadpo fmt --check`);
- have tests, all passing under `jadpo test --json --seed 1`;
- have its own README.md, and appear in examples/README.md's reading order.

`--export-corpus` writes each project and its `jadpo test --json` document
to build/spec-corpus/examples/<project>/, beside the language pages'
projects, for the differential run. Which built features the examples
cover is checked by `cargo test -p jadpo-check --test examples`.

  python3 tools/examples.py                    # check
  python3 tools/examples.py --export-corpus    # check and export
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

from language_docs import CORPUS, ROOT, TEST_SEED, jadpo

EXAMPLES = ROOT / 'examples'
EXPORT = CORPUS / 'examples'


def projects() -> list[Path]:
    return sorted(p for p in EXAMPLES.iterdir() if (p / 'app.jadpo').is_file())


def source_files(project: Path) -> list[Path]:
    return sorted(p for p in project.rglob('*') if p.is_file() and 'build' not in p.relative_to(project).parts)


def check(project: Path) -> tuple[list[str], str | None]:
    """The problems with one project, and its `jadpo test --json` document."""
    name = project.name
    problems = []
    if not (project / 'README.md').is_file():
        problems.append(f'{name}: add a README.md saying what it teaches, how to run it and what to try next')
    with tempfile.TemporaryDirectory() as tmp:
        copy = Path(tmp) / name
        for path in source_files(project):
            target = copy / path.relative_to(project)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
        code, out = jadpo(['check', '--json', '.'], cwd=copy)
        try:
            diagnostics = json.loads(out).get('diagnostics', [])
        except ValueError:
            diagnostics = None
        if diagnostics is None:
            problems.append(f'{name}: jadpo check --json gave no report')
        elif code or diagnostics:
            rules = sorted({d.get('rule', '?') for d in diagnostics})
            problems.append(f'{name}: must check with no diagnostics, but jadpo check says {", ".join(rules) or f"exit {code}"}')
        code, out = jadpo(['fmt', '--check', '.'], cwd=copy)
        if code:
            problems.append(f'{name}: not canonically formatted (run jadpo fmt in it)')
        code, document = jadpo(['test', '.', '--json', '--seed', TEST_SEED], cwd=copy)
        try:
            tests = (json.loads(document).get('data') or {}).get('tests') or []
        except ValueError:
            tests = None
        if tests is None:
            problems.append(f'{name}: jadpo test --json gave no document')
        elif not tests:
            problems.append(f'{name}: has no tests; every example has its own')
        else:
            failed = [t.get('name', '?') for t in tests if t.get('outcome') != 'passed']
            if code or failed:
                problems.append(f'{name}: its tests must pass, but these don\'t: {"; ".join(failed) or f"exit {code}"}')
    return problems, document if not problems else None


def reading_order_problems(found: list[Path]) -> list[str]:
    index = EXAMPLES / 'README.md'
    if not index.is_file():
        return ['examples/README.md is missing: it lists the examples in reading order']
    text = index.read_text()
    return [f'examples/README.md: add {p.name}/ to the reading order' for p in found if f'{p.name}/' not in text]


def export(project: Path, document: str) -> None:
    folder = EXPORT / project.name
    for path in source_files(project):
        target = folder / path.relative_to(project)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
    (folder / 'test.json').write_text(document)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--export-corpus', action='store_true',
                        help=f'write each project and its jadpo test --json to {EXPORT.relative_to(ROOT)}')
    args = parser.parse_args()
    found = projects()
    problems = reading_order_problems(found)
    documents = {}
    for project in found:
        found_problems, document = check(project)
        problems += found_problems
        if document is not None:
            documents[project] = document
        print(f'{project.name}: {"FAILED" if found_problems else "ok"}')
    if args.export_corpus:
        if EXPORT.exists():
            shutil.rmtree(EXPORT)
        for project, document in documents.items():
            export(project, document)
        print(f'wrote {len(documents)} example project(s) to {EXPORT.relative_to(ROOT)}')
    for problem in problems:
        print(f'  FAIL {problem}')
    print(f'{len(found)} example project(s) checked, formatted and tested')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
