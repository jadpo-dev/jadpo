#!/usr/bin/env python3
"""Check the teaching examples in examples/ (docs/compiler-runtime.md §2.7).

Each project in examples/ (a folder holding app.jadpo, including golden-todo/) is copied to a temporary folder, so the
manifest `jadpo check` writes never lands in the repository, and must:

- check with no diagnostics at all (`jadpo check --json`);
- be canonically formatted (`jadpo fmt --check`);
- have tests, all passing under `jadpo test --json --seed 1`;
- have its own README.md, and appear in examples/README.md's reading order;
- give, for each edit its README's "Try next" suggests, the outcome it
  names. Each such bullet ("In `<file>`, ...") is followed by a comment a
  reader of the rendered page never sees:

    <!-- try {"file": "domains/x/actions/y.jadpo", "replace": "a", "with": "b",
              "check": ["type.mismatch"]} -->

  `replace` must occur exactly once in the file, as must each further
  `{"replace": …, "with": …}` in an optional `also` list (the `use` line a
  new name needs, say); `check` lists the rules
  `jadpo check --json` then reports (each at least once, nothing else), or
  `test` the codes the failed tests of `jadpo test --json --seed 1` have,
  with `failed` the number of failed tests when the bullet counts them.

Under JADPO_SKIP_POSTGRES=1 (`tools/verify.py --profile quick`) a project
with migrations is checked and formatted, but its `jadpo test` runs, and
the "Try next" edits that name test outcomes, are reported as skipped,
since they need PostgreSQL.

A project with a `nearest` query needs pgvector 0.8.0 for its HNSW index
(area 21). Where the server offers an older one (Postgres.app ships
0.7.0), `jadpo test` refuses with `migration.extension_outdated` before
any test runs, and the project's tests and test-outcome edits are
reported as skipped, as the live tests of crates/jadpo-cli/tests/vectors.rs
are; under CI, whose image has 0.8.7, that refusal fails.

`--export-corpus` writes each project and its `jadpo test --json` document
to build/spec-corpus/examples/<project>/, beside the language pages'
projects, for the differential run. Which built features the examples
cover is checked by `cargo test -p jadpo-check --test integration examples::`.

  python3 tools/examples.py                    # check
  python3 tools/examples.py --export-corpus    # check and export
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from language_docs import CORPUS, ROOT, SKIP_POSTGRES, TEST_SEED, WORKERS, ensure_jadpo, jadpo

EXAMPLES = ROOT / 'examples'
EXPORT = CORPUS / 'examples'


def projects() -> list[Path]:
    return sorted(p for p in EXAMPLES.iterdir() if (p / 'app.jadpo').is_file())


def needs_postgres(project: Path) -> bool:
    """Whether the project's tests need PostgreSQL: it has migrations."""
    return any((project / 'database/migrations').glob('*/*.sql'))


def source_files(project: Path) -> list[Path]:
    return sorted(p for p in project.rglob('*') if p.is_file() and 'build' not in p.relative_to(project).parts)


# `jadpo test`'s refusal when the server's pgvector is older than a
# `nearest` query's HNSW index needs.
PGVECTOR_OUTDATED = 'migration.extension_outdated'


def pgvector_outdated(document: str) -> bool:
    """Whether `jadpo test --json` stopped on the server's pgvector being
    too old for a `nearest` query, which skips the project's tests outside
    CI (CI's image has 0.8.7, so there it is a failure)."""
    try:
        rules = [d.get('rule') for d in json.loads(document).get('diagnostics', [])]
    except ValueError:
        return False
    return rules == [PGVECTOR_OUTDATED] and not os.environ.get('CI')


# The projects whose tests were skipped for an old pgvector, by name.
OLD_PGVECTOR: set[str] = set()


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
        if SKIP_POSTGRES and needs_postgres(project):
            return problems, None
        code, document = jadpo(['test', '.', '--json', '--seed', TEST_SEED], cwd=copy)
        if pgvector_outdated(document):
            OLD_PGVECTOR.add(name)
            return problems, None
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
    edits, problems = try_next(project)
    # Each edit is tried on a copy of its own, so they run at once.
    with ThreadPoolExecutor(WORKERS) as pool:
        for found in pool.map(lambda edit: try_edit(project, edit), edits):
            problems += found
    return problems


def try_edit(project: Path, edit: dict) -> list[str]:
    """The problems with one "Try next" edit, applied to a copy."""
    name = project.name
    problems: list[str] = []
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
            return problems
        text = target.read_text()
        # `also` lists further replacements in the same file, such as
        # the `use` line a new name needs.
        replacements = [edit] + list(edit.get('also', []))
        bad = [r for r in replacements if not r.get('replace') or text.count(r['replace']) != 1]
        if bad:
            problems.append(f'{label}: the text to replace must occur exactly once: {bad[0].get("replace")!r}')
            return problems
        for replacement in replacements:
            text = text.replace(replacement['replace'], replacement.get('with', ''))
        target.write_text(text)
        if 'check' in edit:
            _, out = jadpo(['check', '--json', '.'], cwd=copy)
            try:
                rules = sorted({d['rule'] for d in json.loads(out)['diagnostics']})
            except (ValueError, KeyError):
                problems.append(f'{label}: jadpo check --json gave no report')
                return problems
            if rules != sorted(edit['check']):
                problems.append(f'{label}: the README says {", ".join(edit["check"])}, but jadpo check reports {", ".join(rules) or "nothing"}')
        if 'test' in edit and not (SKIP_POSTGRES and needs_postgres(project)):
            _, out = jadpo(['test', '.', '--json', '--seed', TEST_SEED], cwd=copy)
            if pgvector_outdated(out):
                return problems
            try:
                document = json.loads(out)
            except ValueError:
                problems.append(f'{label}: jadpo test --json gave no document')
                return problems
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

    # Every project, and its "Try next" edits, at once.
    ensure_jadpo()
    with ThreadPoolExecutor(WORKERS) as pool:
        checked = [pool.submit(check, project) for project in found]
        tried = [pool.submit(try_outcomes, project) for project in found]
        examined = [(c.result(), t.result()) for c, t in zip(checked, tried)]
    for project, ((found_problems, document), tried_problems) in zip(found, examined):
        found_problems += tried_problems
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
    skipped = [p.name for p in found if SKIP_POSTGRES and needs_postgres(p)]
    if skipped:
        print(f'skipped: jadpo test for {", ".join(skipped)}, whose migrations need PostgreSQL '
              f'(JADPO_SKIP_POSTGRES is set)')
    if OLD_PGVECTOR:
        print(f'skipped: jadpo test for {", ".join(sorted(OLD_PGVECTOR))}, whose nearest queries need '
              f'pgvector 0.8.0, which the server doesn\'t offer')
    for problem in problems:
        print(f'  FAIL {problem}')
    print(f'{len(found)} example project(s) checked, formatted and tested')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
