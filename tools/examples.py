#!/usr/bin/env python3
"""Check the teaching examples in examples/ (docs/compiler-runtime.md §2.7).

Each project in examples/ (a folder holding app.jadpo; golden-todo/ holds
only the golden app's contract) is copied to a temporary folder, so the
manifest `jadpo check` writes never lands in the repository, and must:

- check with no diagnostics at all, errors or warnings (`jadpo check --json`);
- be canonically formatted (`jadpo fmt --check`);
- have tests, all passing under `jadpo test --json --seed 1`;
- have its own README.md, and appear in examples/README.md's reading order;
- give, for each edit its README's "Try next" suggests, the outcome it
  names. Each such bullet ("In `<file>`, ...") is followed by a comment a
  reader of the rendered page never sees:

    <!-- try {"file": "domains/x/actions/y.jadpo", "replace": "a", "with": "b",
              "check": ["type.mismatch"]} -->

  `replace` must occur exactly once in the file; `check` lists the rules
  `jadpo check --json` then reports (each at least once, nothing else), or
  `test` the codes the failed tests of `jadpo test --json --seed 1` have,
  with `failed` the number of failed tests when the bullet counts them.

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
import re
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


TRY = re.compile(r'<!-- try (\{.*?\}) -->', re.DOTALL)


def try_next(project: Path) -> tuple[list[dict], list[str]]:
    """The edits the README's "Try next" section suggests, and the
    bullets that suggest an edit without saying it in a `try` comment."""
    readme = project / 'README.md'
    if not readme.is_file():
        return [], []
    text = readme.read_text()
    section = text.split('## Try next', 1)[1] if '## Try next' in text else ''
    section = re.split(r'^## ', section, maxsplit=1, flags=re.MULTILINE)[0]
    edits, problems = [], []
    bullets = re.split(r'^- ', section, flags=re.MULTILINE)[1:]
    for bullet in bullets:
        found = TRY.search(bullet)
        if found is None:
            if bullet.startswith('In `'):
                first = bullet.split('\n', 1)[0][:60]
                problems.append(f'{project.name}: README "Try next" bullet "{first}…" needs a <!-- try {{...}} --> comment')
            continue
        try:
            edits.append(json.loads(found.group(1)))
        except ValueError as error:
            problems.append(f'{project.name}: README try comment isn\'t JSON: {error}')
    return edits, problems


def try_outcomes(project: Path) -> list[str]:
    """Applies each "Try next" edit to a copy and checks it gives what the
    README says."""
    name = project.name
    edits, problems = try_next(project)
    for edit in edits:
        label = f'{name}: README "Try next" edit of {edit.get("file")}'
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / name
            for path in source_files(project):
                target = copy / path.relative_to(project)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
            target = copy / edit.get('file', '')
            if not target.is_file():
                problems.append(f'{label}: no such file')
                continue
            text = target.read_text()
            if text.count(edit.get('replace', '')) != 1 or not edit.get('replace'):
                problems.append(f'{label}: the text to replace must occur exactly once, and occurs {text.count(edit.get("replace", "")) if edit.get("replace") else 0} times')
                continue
            target.write_text(text.replace(edit['replace'], edit.get('with', '')))
            if 'check' in edit:
                _, out = jadpo(['check', '--json', '.'], cwd=copy)
                try:
                    rules = sorted({d['rule'] for d in json.loads(out)['diagnostics']})
                except (ValueError, KeyError):
                    problems.append(f'{label}: jadpo check --json gave no report')
                    continue
                if rules != sorted(edit['check']):
                    problems.append(f'{label}: the README says {", ".join(edit["check"])}, but jadpo check reports {", ".join(rules) or "nothing"}')
            if 'test' in edit:
                _, out = jadpo(['test', '.', '--json', '--seed', TEST_SEED], cwd=copy)
                try:
                    document = json.loads(out)
                except ValueError:
                    problems.append(f'{label}: jadpo test --json gave no document')
                    continue
                tests = (document.get('data') or {}).get('tests') or []
                codes = [t['failure']['code'] for t in tests if t.get('outcome') == 'failed']
                if sorted(set(codes)) != sorted(edit['test']):
                    rules = [d.get('rule') for d in document.get('diagnostics', [])]
                    problems.append(f'{label}: the README says {", ".join(edit["test"])}, but jadpo test gives {", ".join(sorted(set(codes))) or ", ".join(rules) or "a pass"}')
                elif 'failed' in edit and len(codes) != edit['failed']:
                    problems.append(f'{label}: the README says {edit["failed"]} tests fail, but {len(codes)} do')
    return problems


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
        found_problems += try_outcomes(project)
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
