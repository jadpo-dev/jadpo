#!/usr/bin/env python3
"""Check and render the language reference in docs/language.

A ```jadpo check block is compiled by `jadpo`, built from this checkout
(docs/compiler-runtime.md §2.7), with `jadpo check`: a block without
`expect=` must check cleanly, exit 0 and be canonically formatted (`jadpo fmt
--check`), and an `expect=rule.id` block must report exactly one diagnostic,
with that rule, read from `jadpo check --json`, exit 1, be
formatted unless the rule is a syntax or import error (`jadpo fmt` fixes
imports), and be followed by a
```text output block holding the exact plain output, which `--bless`
rewrites. A project with an `app.jadpo` block is checked as a project, with
the layout rules on; one without is checked as loose files.

A `check` project that declares tests is also run (docs/compiler-runtime.md
§2.7): without `expect=`, `jadpo test --json` must pass; with
`expect=test.<code>`, the project must check cleanly and exactly one test
must fail with that code, and the ```text output block after it holds
`jadpo test`'s exact plain output. Both run with `--seed 1`, so the order
and the rerun commands are the same every time. `--export-corpus` writes
each such project and its `jadpo test --json` document to
build/spec-corpus/<page>/<project>/ for the differential run.

Every other ```jadpo block is a `fragment`, shown and not compiled: code
the compiler doesn't check yet. A block that is neither fails the check.

A ```sh run block shows commands and is checked by running them
(docs/writing.md, "How an error page reads"): its lines run with `sh -e`
in a fresh copy of its project, the `check` blocks with its `project=`,
with this checkout's `jadpo` in plain mode and an empty home folder, and
what they print must be the ```text output block after it, which
`--bless` rewrites. The project's folder prints as the reader's own (`.`),
the home folder as `~`, and a run's duration is left out. With
`expect=<rule>` the commands end failing with that rule's diagnostic and
no other; without it they succeed. `tty` runs them in a terminal, where
`typed=<value>`, or nothing, and Enter are typed at the first prompt.

  python3 tools/docs/language_docs.py --check     # verify, and render into a scratch folder
  python3 tools/docs/language_docs.py             # verify and write build/language-site
  python3 tools/docs/language_docs.py --bless     # rewrite the output blocks after `check` blocks
  python3 tools/docs/language_docs.py --check --export-corpus  # also write build/spec-corpus/
  python3 tools/docs/language_docs.py --bundle <dir>  # build the docs bundle (docs/documentation.md)

The blocks of the area pages, their design notes (docs/language/notes/),
the error pages (docs/errors/) and the primer (docs/primer.md) are all
checked this way. The primer, the page an agent reads first, holds no
`fragment` and no topic ID, and is PRIMER_TOKENS long as the bundle gives
it, its words times 1.3. Every rule in the
compiler's catalogue (`jadpo help --json`) has a page, docs/errors/<rule>.md,
and no page names a rule the catalogue doesn't have. A rule's page needs a
`check` block failing with it, unless only a command raises the rule (area
14), whose page needs a `check` or ```sh run block failing with it only
once it has one at all. On areas
1-9 no `fragment` may check cleanly, since one that does is an example the
compiler can check.

The bundle also holds a page per standard type and their index
(TypeReference), rendered from `jadpo help --json`'s `types` and
`operations` and each type's golden file, so the reference lists exactly
what the compiler builds; the Standard library page's table of types by
group must agree with the compiler's groups (type_table_problems).

The Learn path (docs/learn/) is checked the same way, each page's
project built on the pages before it (LearnPage). The guides
(docs/guides/) are too, each guide a whole project of its own
(GuidePage). They and the primer follow the mechanical rules of
docs/writing.md (user_page_problems).

Every section an app author reads, each `#` and `##` heading of an area page
but `## Errors`, declares its topic ID on the line after it, as
`<!-- topic: <id> -->`: lowercase kebab case, unique across the docs. A
renamed topic keeps its old IDs, `<!-- topic: <id> (was <old-id>) -->`,
and index.json lists them as its `aliases`. A
topic is at most TOPIC_WORDS words as the bundle gives it. An area page
names no area or question by number (numbered_references): on areas 1 to
NUMBERED_FAILS_THROUGH, which is every area, that fails. Every relative
link in the repository's Markdown and llms.txt resolves, to a file and, for
Markdown, a heading GitHub gives that anchor; every area-page anchor the
compiler prints (`help_at`) names a heading of that page.
"""

import argparse
import atexit
import hashlib
import html
import json
import os
import re
import select
import signal
import subprocess
import sys
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / 'docs/language'
# One page per compiler rule, its examples (docs/documentation.md).
ERRORS = ROOT / 'docs/errors'
# Each area's design notes: what a person changing the language reads.
NOTES = DOCS / 'notes'
PRIMER = ROOT / 'docs/primer.md'
LEARN = ROOT / 'docs/learn'
GUIDES = ROOT / 'docs/guides'
# The primer's length in tokens, estimated as its words times 1.3, as the
# bundle gives it (docs/documentation.md, "Where the content lives").
PRIMER_TOKENS = (3000, 5000)
# A topic's ID, on the line after its heading, and the IDs it had before
# a rename, which still reach it: `<!-- topic: new-id (was old-id, older-id) -->`.
TOPIC = re.compile(r'^<!-- topic: (\S*?)(?: \(was ([^)]*)\))? -->$')
TOPIC_ID = re.compile(r'^[a-z0-9]+(?:-[a-z0-9]+)*$')
# The most words a topic holds, as the bundle gives it.
TOPIC_WORDS = 3000
# Where a bundle page's link to a file outside the bundle goes.
REPO_URL = 'https://github.com/jadpo-dev/jadpo/blob/main/'
RAW_URLS = ('https://raw.githubusercontent.com/jadpo-dev/jadpo/main/',
            'https://github.com/jadpo-dev/jadpo/blob/main/',
            'https://github.com/jadpo-dev/jadpo/tree/main/')
# What the page of a rule with no example says.
NO_EXAMPLE = 'No example yet.'
JADPO = ROOT / 'target/debug/jadpo'
OUTPUT_FENCE = '```text output'
TEMPLATE = Path(__file__).resolve().parent / 'language_docs_template.html'
OUT_DEFAULT = ROOT / 'build/language-site'
CORPUS = ROOT / 'build/spec-corpus'
# Runs are ordered by this seed, so outputs and rerun commands are stable.
TEST_SEED = '1'
TEST_DECLARATION = re.compile(r'^\s*test\s+"', re.MULTILINE)
# Optional build/language-site/published.json maps page stems to published URLs for cross-page links.
FENCE = re.compile(r'^```(\S*)(.*)$')
# On areas 1 to this one, and their notes and error pages, no fragment may
# check cleanly.
AREA_OWNED = 9
# `migrate=<purpose>` on a block runs `jadpo migrate new <purpose>` once
# the project's blocks up to it are written, at a fixed time per step, so
# migration folders are named the same on every run.
MIGRATION_EPOCH = 1792504800  # 2026-10-20T14:00:00Z
# Area 14's command errors, shown by the CLI's tests rather than by pages.
COMMAND_AREA = 14
# Set by `tools/verify.py --profile quick`: nothing may start PostgreSQL, so
# a project with migrations is checked and formatted, and its `jadpo test`
# run is reported as skipped.
SKIP_POSTGRES = os.environ.get('JADPO_SKIP_POSTGRES') == '1'
# Seconds rendering one page may take; each takes well under one.
RENDER_LIMIT = 10
# Seconds a ```sh run block's commands may take; each takes well under one.
RUN_LIMIT = 60
# A ```sh run line that takes a permission away, as an error about a
# folder that can't be read or written needs. Root reads and writes past
# any permission, so as root such a block would pass wrongly.
TAKES_PERMISSION = re.compile(r'^\s*chmod\b', re.M)
# Seconds a `tty` block waits, once a prompt shows, for the terminal to stop
# echoing before it types: a prompt for a secret turns the echo off first,
# and one that doesn't ask for a secret never does.
ECHO_WAIT = 2
# A prompt, as the CLI design language draws it: `? Enter API_KEY  hidden`.
PROMPT = re.compile(r'(?:^|\n)\? ')
# A diagnostic's rule line in plain output.
RULE_LINE = re.compile(r'^  rule: (\S+)$', re.MULTILINE)


# A slow machine adds the run's duration to a summary line (` in 0.1 s`);
# output blocks are compared and written without it.
RUN_TIME = re.compile(r' in \d+(?:\.\d+)? s$', re.MULTILINE)
# A long-running command's header gives the clock time it started
# (`jadpo dev, started 21:13:00`); that is left out the same way.
STARTED_AT = re.compile(r', started \d{2}:\d{2}:\d{2}$', re.MULTILINE)


def without_run_time(text):
    return STARTED_AT.sub('', RUN_TIME.sub('', text))


def run(args, cwd=None):
    result = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


_jadpo_built = False


# Projects are checked this many at a time; each is a few `jadpo` runs.
WORKERS = os.cpu_count() or 4


def ensure_jadpo():
    """`jadpo`, built from this checkout: the binary JADPO_BIN names
    (tools/verify.py builds it once), or else one built here. Called before
    any threads run it."""
    global _jadpo_built, JADPO
    if not _jadpo_built:
        if os.environ.get('JADPO_BIN'):
            JADPO = Path(os.environ['JADPO_BIN'])
        else:
            code, out = run(['cargo', 'build', '--quiet', '--locked', '-p', 'jadpo-cli'], cwd=ROOT)
            if code:
                sys.exit(f'could not build jadpo:\n{out}')
        _jadpo_built = True


def jadpo(args, cwd=None, env=None):
    """Runs `jadpo`, built from this checkout, in plain mode."""
    ensure_jadpo()
    extra = env or {}
    env = {k: v for k, v in os.environ.items() if k not in ('FORCE_COLOR', 'CLICOLOR_FORCE', 'COLUMNS')}
    env['JADPO_PLAIN'] = '1'
    # This checkout's own build checks every example, whatever line it
    # names, and never fetches a release (area 14, "Which jadpo runs").
    env['JADPO_TOOLCHAIN'] = 'local'
    env.update(extra)
    result = subprocess.run([str(JADPO), *args], cwd=cwd, capture_output=True, text=True, env=env)
    return result.returncode, result.stdout + result.stderr


def load_help():
    """`jadpo help --json`'s data: its commands, and its rules, each of
    whose `area` names the page it belongs to."""
    code, out = jadpo(['help', '--json'])
    if code:
        sys.exit(f'jadpo help --json failed:\n{out}')
    return json.loads(out)['data']


def load_catalogue():
    """The compiler's rules (`jadpo help --json`), by id."""
    return {rule['id']: rule for rule in load_help()['rules']}


def parse_front_matter(text):
    if not text.startswith('---\n'):
        return {}, text
    end = text.index('\n---\n', 4)
    meta, key = {}, None
    for line in text[4:end].splitlines():
        if line.startswith('  - ') and key:
            meta.setdefault(key, []).append(line[4:].strip())
        elif ':' in line:
            key, value = line.split(':', 1)
            key, value = key.strip(), value.strip()
            meta[key] = value if value else []
    return meta, text[end + 5:]


def parse_attrs(text):
    attrs = {}
    for token in text.split():
        name, _, value = token.partition('=')
        attrs[name] = value if value else True
    return attrs


class Page:
    def __init__(self, path, catalogue):
        self.path = path
        # The compiler's catalogue (`jadpo help --json`), whose `area` field
        # says which page owns each rule.
        self.catalogue = catalogue
        text = path.read_text()
        self.meta, self.body = parse_front_matter(text)
        # File lines before the body, so messages give the file's line numbers.
        self.offset = text[:len(text) - len(self.body)].count('\n')
        self.blocks = []
        # ```sh run blocks: commands run in a copy of their project.
        self.runs = []
        self.problems = []
        self.results = {}
        # Each run block that ran: its expected rule or 'pass', and whether
        # it behaved as stated.
        self.run_results = []
        self.blessings = []
        # Projects with tests that ran: (name, files, jadpo test --json).
        self.corpus = []
        # Projects whose tests weren't run, since they need PostgreSQL.
        self.skipped = []
        # ```sh run database blocks not run under JADPO_SKIP_POSTGRES.
        self.skipped_runs = []

    def collect_blocks(self):
        lines = self.body.splitlines()
        i, n = 0, 0
        self.runs = []
        while i < len(lines):
            match = FENCE.match(lines[i])
            if match and match.group(1) == 'sh' and parse_attrs(match.group(2)).get('run'):
                self.runs.append(self.run_block(lines, i, parse_attrs(match.group(2))))
                i = self.runs[-1]['end'] + 1
                continue
            if match and match.group(1) == 'jadpo':
                attrs = parse_attrs(match.group(2))
                j = i + 1
                while not lines[j].startswith('```'):
                    j += 1
                n += 1
                attrs.setdefault('project', f'example-{n}')
                attrs.setdefault('file', 'main.jadpo')
                block = {'line': i, 'end': j, 'attrs': attrs, 'source': '\n'.join(lines[i + 1:j]) + '\n'}
                # An output block may follow, after blank lines.
                k = j + 1
                while k < len(lines) and not lines[k].strip():
                    k += 1
                if k < len(lines) and lines[k].strip() == OUTPUT_FENCE:
                    m = k + 1
                    while m < len(lines) and not lines[m].startswith('```'):
                        m += 1
                    block['output'] = {'start': k, 'end': m, 'text': '\n'.join(lines[k + 1:m]) + '\n'}
                self.blocks.append(block)
                i = j
            i += 1

    def run_block(self, lines, i, attrs):
        """A ```sh run block at line i, and the output block after it."""
        j = i + 1
        while not lines[j].startswith('```'):
            j += 1
        attrs.setdefault('project', f'run-{len(self.runs) + 1}')
        block = {'line': i, 'end': j, 'attrs': attrs, 'source': '\n'.join(lines[i + 1:j]) + '\n'}
        k = j + 1
        while k < len(lines) and not lines[k].strip():
            k += 1
        if k < len(lines) and lines[k].strip() == OUTPUT_FENCE:
            m = k + 1
            while m < len(lines) and not lines[m].startswith('```'):
                m += 1
            block['output'] = {'start': k, 'end': m, 'text': '\n'.join(lines[k + 1:m]) + '\n'}
        return block

    def check_runs(self, bless):
        """```sh run blocks: each one's commands, run by `sh -e` in a fresh
        copy of its project (the `check` blocks with its `project=`, made
        as a reader's commands make them), print what the output block
        after it holds. With `expect=<rule>` they end failing with that
        rule's diagnostic, and only that one; without it they succeed.
        With `database`, `DATABASE_URL` names a new, empty database of its
        own on the run's PostgreSQL server ([`Databases`]); under
        JADPO_SKIP_POSTGRES the block is reported as skipped. As root, a
        block with a `chmod` line is refused, not run: root ignores the
        permissions it takes away."""
        def run_one(block):
            name = block['attrs']['project']
            if TAKES_PERMISSION.search(block['source']) and os.geteuid() == 0:
                return [f'{name} (line {block["line"] + self.offset + 1}): its commands take a permission away '
                        'with `chmod`, which root ignores, so as root they would pass wrongly; '
                        'run the docs check as an ordinary user'], 'refused'
            database = None
            if block['attrs'].get('database'):
                if SKIP_POSTGRES:
                    return [], 'skipped'
                database = DATABASES.create(name)
                if database.startswith('error: '):
                    return [f'{name} (line {block["line"] + self.offset + 1}): no database for its commands: '
                            f'{database[len("error: "):]}'], 'refused'
            files = [b for b in self.blocks if b['attrs'].get('check') and b['attrs']['project'] == name]
            with tempfile.TemporaryDirectory() as tmp:
                project, home = Path(tmp) / 'project', Path(tmp) / 'home'
                project.mkdir()
                home.mkdir()
                problems = write_project(project, name, files)
                if files:
                    problems += self.prepare(str(project), name, files)
                ran = run_commands(block, project, home, database)
                # A block may leave folders read-only, as its error needs;
                # make them writable again so they can be removed.
                for folder, _, _ in os.walk(tmp):
                    os.chmod(folder, 0o700)
            return problems, ran

        with ThreadPoolExecutor(WORKERS) as pool:
            ran = list(pool.map(run_one, self.runs))
        for block, (problems, result) in zip(self.runs, ran):
            where = f'{block["attrs"]["project"]} (line {block["line"] + self.offset + 1})'
            self.problems += problems
            expected = block['attrs'].get('expect')
            expected = expected if isinstance(expected, str) else 'pass'
            if result == 'refused':
                self.run_results.append({'expected': expected, 'ok': False})
                continue
            if result == 'skipped':
                # Its commands need PostgreSQL, which this run may not start.
                self.skipped_runs.append(block['attrs']['project'])
                self.run_results.append({'expected': expected, 'ok': True})
                continue
            if result is None:
                self.problems.append(f'{where}: its commands didn\'t finish in {RUN_LIMIT} s')
                self.run_results.append({'expected': expected, 'ok': False})
                continue
            code, out = result
            rules = RULE_LINE.findall(out)
            ok = True
            if expected == 'pass' and (code or rules):
                ok = False
                self.problems.append(f'{where}: expected to succeed, but it exits {code}'
                                     + (f' with {rules[:3]}' if rules else ''))
            elif expected != 'pass' and (rules != [expected] or not code):
                ok = False
                self.problems.append(f'{where}: expected exactly one {expected} and a failing exit, but got '
                                     f'{rules[:4] or "no rule"} and exit {code}')
            written = block.get('output', {}).get('text')
            if written != out:
                if bless:
                    self.blessings.append((block, out))
                elif written is None:
                    ok = False
                    self.problems.append(f'{where}: add a {OUTPUT_FENCE} block after it (run --bless)')
                else:
                    ok = False
                    self.problems.append(f'{where}: its output block differs from what the commands print '
                                         '(run --bless)')
            self.run_results.append({'expected': expected, 'ok': ok})

    def check_projects(self, bless):
        """`check` blocks, compiled by `jadpo`."""
        projects = {}
        for block in self.blocks:
            if block['attrs'].get('check'):
                projects.setdefault(block['attrs']['project'], []).append(block)
        def expectation(blocks):
            expected = {b['attrs']['expect'] for b in blocks if 'expect' in b['attrs']} or {'pass'}
            has_tests = any(TEST_DECLARATION.search(b['source']) for b in blocks
                            if b['attrs']['file'].endswith('.jadpo'))
            agreed = len(expected) == 1
            expected = expected.pop()
            return agreed, expected, expected == 'pass' and has_tests or expected.startswith('test.')

        def compile_one(item):
            """Runs jadpo over one project: its migrations' problems, and
            what `check`, `fmt` and, when it has tests, `test` said. A
            project with tests runs `jadpo test --json` once; only one that
            shows a failed test also runs the plain `jadpo test` its output
            block holds. Under JADPO_SKIP_POSTGRES a project with
            migrations runs no tests (`test_json` is None)."""
            name, blocks = item
            _, expected, runs = expectation(blocks)
            if SKIP_POSTGRES and any(b['attrs'].get('migrate') for b in blocks):
                runs = False
            problems, test_code, test_out, test_json_code, test_json = [], None, None, None, None
            with tempfile.TemporaryDirectory() as tmp:
                problems += write_project(Path(tmp), name, blocks)
                problems += self.prepare(tmp, name, blocks)
                code, out = jadpo(['check', tmp])
                json_code, json_out = jadpo(['check', '--json', tmp])
                formatted, _ = jadpo(['fmt', '--check', tmp])
                if runs:
                    # From inside the project, so paths and rerun commands
                    # are the project's own.
                    if expected.startswith('test.'):
                        test_code, test_out = jadpo(['test', '.', '--seed', TEST_SEED], cwd=tmp)
                    test_json_code, test_json = jadpo(['test', '.', '--json', '--seed', TEST_SEED], cwd=tmp)
            out = without_run_time(out.replace(tmp + '/', '').replace(tmp, '.'))
            if test_out is not None:
                test_out = without_run_time(test_out)
            return problems, code, out, json_code, json_out, formatted, test_code, test_out, test_json_code, test_json

        with ThreadPoolExecutor(WORKERS) as pool:
            compiled = list(pool.map(compile_one, projects.items()))
        for (name, blocks), ran in zip(projects.items(), compiled):
            agreed, expected, runs = expectation(blocks)
            if not agreed:
                self.problems.append(f'{name}: blocks disagree on expect=')
            (problems, code, out, json_code, json_out, formatted,
             test_code, test_out, test_json_code, test_json) = ran
            self.problems += problems
            if runs:
                self.run_tests(name, blocks, expected, code, formatted,
                               (test_code, test_out, test_json_code, test_json), bless)
                continue
            # The rules come from the JSON report, not the text, so a code
            # frame quoting `rule: ...` can't stand in for a diagnostic.
            try:
                rules = [d['rule'] for d in json.loads(json_out)['diagnostics']]
            except (ValueError, KeyError):
                rules = []
                self.problems.append(f'{name}: jadpo check --json gave no report')
            if json_code != code:
                self.problems.append(f'{name}: jadpo check exits {code} but --json exits {json_code}')
            shown = [b for b in blocks if 'expect' in b['attrs']] or blocks
            last = shown[-1]
            if expected == 'pass':
                ok = code == 0 and formatted == 0
                if code:
                    self.problems.append(f'{name}: expected to check cleanly, but jadpo check says {rules[:3] or "no"}')
                elif formatted:
                    self.problems.append(f'{name}: checks but is not canonically formatted (run jadpo fmt)')
            else:
                # Exactly the expected error, once, and the failing exit
                # code: an example shows one mistake.
                # Every finding is an error (area 1).
                exit_code = 1
                ok = rules == [expected] and code == exit_code
                if rules != [expected]:
                    self.problems.append(f'{name}: expected exactly one {expected} but got {rules[:4] or "a pass"}')
                elif code != exit_code:
                    self.problems.append(f'{name}: gives {expected} but exits {code}, not {exit_code}')
                # `jadpo fmt` fixes imports, so an import mistake is
                # never formatted as written.
                elif formatted and not expected.startswith(('syntax.', 'import.')):
                    ok = False
                    self.problems.append(f'{name}: not canonically formatted (run jadpo fmt)')
                written = last.get('output', {}).get('text')
                if written != out:
                    if bless:
                        self.blessings.append((last, out))
                    elif written is None:
                        self.problems.append(f'{name}: add a {OUTPUT_FENCE} block after it (run --bless)')
                    else:
                        self.problems.append(f'{name}: its output block differs from jadpo\'s output (run --bless)')
            self.results[name] = {'expected': expected, 'ok': ok, 'output': out.strip(), 'rules': rules, 'check': True}

    def run_tests(self, name, blocks, expected, check_code, formatted, ran, bless):
        """A `check` project with tests, run by `jadpo test`: it must check
        cleanly and be formatted; then all its tests pass, or, for
        `expect=test.<code>`, exactly one fails with that code and the
        output block shows the run."""
        test_code, test_out, test_json_code, test_json = ran
        ok = check_code == 0 and formatted == 0
        if check_code:
            self.problems.append(f'{name}: has tests, so it must check cleanly before they run')
        elif formatted:
            self.problems.append(f'{name}: checks but is not canonically formatted (run jadpo fmt)')
        if test_json is None:
            # Its tests need PostgreSQL, which this run may not start.
            self.skipped.append(name)
            self.results[name] = {'expected': expected, 'ok': ok, 'output': '', 'rules': [], 'check': True}
            return
        try:
            document = json.loads(test_json)
        except ValueError:
            document = {}
            self.problems.append(f'{name}: jadpo test --json gave no document')
        if test_code is not None and test_json_code != test_code:
            self.problems.append(f'{name}: jadpo test exits {test_code} but --json exits {test_json_code}')
        tests = (document.get('data') or {}).get('tests') or []
        codes = [t['failure']['code'] for t in tests if t.get('outcome') == 'failed']
        if expected == 'pass':
            if test_json_code != 0 or codes or not tests:
                ok = False
                shown = codes[:3] or [d.get('rule') for d in document.get('diagnostics', [])][:3]
                self.problems.append(f'{name}: its tests must pass, but jadpo test says {shown or "nothing ran"}')
        else:
            if codes != [expected] or test_json_code != 1:
                ok = False
                self.problems.append(f'{name}: expected exactly one test to fail with {expected} but got {codes or "no failures"}')
            last = [b for b in blocks if 'expect' in b['attrs']][-1]
            written = last.get('output', {}).get('text')
            if written != test_out:
                if bless:
                    self.blessings.append((last, test_out))
                elif written is None:
                    self.problems.append(f'{name}: add a {OUTPUT_FENCE} block after it (run --bless)')
                else:
                    self.problems.append(f'{name}: its output block differs from jadpo test\'s output (run --bless)')
        files = [(b['attrs']['file'], b['source']) for b in blocks]
        if ok:
            self.corpus.append((name, files, test_json))
        self.results[name] = {'expected': expected, 'ok': ok, 'output': (test_out or '').strip(), 'rules': codes,
                              'check': True}

    def prepare(self, tmp, name, blocks):
        """Anything a project needs after its files are written and before
        it is checked; problems found doing it. A reader keeps the records
        with `jadpo records update` (area 18), so a project's check makes
        them too ([`keep_records`])."""
        return keep_records(tmp, blocks)

    def bless(self):
        """Writes each check block's real output after it."""
        if not self.blessings:
            return 0
        text = self.path.read_text()
        head = text[:len(text) - len(self.body)]
        lines = self.body.splitlines()
        for block, out in sorted(self.blessings, key=lambda b: b[0]['end'], reverse=True):
            output = [OUTPUT_FENCE, *out.rstrip('\n').split('\n'), '```']
            if 'output' in block:
                lines[block['output']['start']:block['output']['end'] + 1] = output
            else:
                lines[block['end'] + 1:block['end'] + 1] = ['', *output]
        self.path.write_text(head + '\n'.join(lines) + '\n')
        return len(self.blessings)

    def check_marked(self):
        """Every block is checked or a fragment: nothing else compiles it."""
        for block in self.blocks:
            if not (block['attrs'].get('check') or block['attrs'].get('fragment')):
                self.problems.append(f'{block["attrs"]["project"]} (line {block["line"] + self.offset + 1}) is '
                                     'neither `check` nor `fragment`: make it a check block, or a fragment if '
                                     'the compiler can\'t check it yet')

    def covered_rules(self):
        """Rules a `check` example or a ```sh run block here shows failing
        as stated."""
        results = [*self.results.values(), *self.run_results]
        return {r['expected'] for r in results if r['expected'] != 'pass' and r['ok']}

    def area_owned(self):
        return 1 <= self.area() <= AREA_OWNED

    def area(self):
        return int(self.meta.get('area', 0))

    def check_fragments(self):
        """On areas 1 to AREA_OWNED, no fragment is a program the compiler
        checks cleanly: one that is should be a check block."""
        if not self.area_owned():
            return

        def check_fragment(block):
            with tempfile.TemporaryDirectory() as tmp:
                (Path(tmp) / 'main.jadpo').write_text(block['source'])
                return jadpo(['check', tmp])[0]

        fragments = [block for block in self.blocks if block['attrs'].get('fragment')]
        with ThreadPoolExecutor(WORKERS) as pool:
            codes = list(pool.map(check_fragment, fragments))
        for block, code in zip(fragments, codes):
            if code == 0:
                self.problems.append(f'the fragment at line {block["line"] + self.offset + 1} checks cleanly: '
                                     'make it a check block')

    def check(self, bless=False):
        self.collect_blocks()
        self.check_marked()
        self.check_projects(bless)
        self.check_runs(bless)
        self.check_fragments()

    def topics(self):
        """The page's topics, in order: each `#` and `##` heading but
        `## Errors`, with the ID on the line after it, up to the next of
        them or `## Errors`. Each is a dict of `id` (None when missing),
        `aliases` (the IDs it had before a rename), `title`, `level`, and
        its `start` and `end` lines in the body."""
        lines = self.body.split('\n')
        found = []
        for i, line, fenced in body_lines(lines):
            heading = HEADING.match(line) if not fenced else None
            if not heading or len(heading.group(1)) > 2:
                continue
            title = heading.group(2)
            if found:
                found[-1]['end'] = i
            if len(heading.group(1)) == 2 and title == 'Errors':
                found.append(None)
                continue
            declared = TOPIC.match(lines[i + 1]) if i + 1 < len(lines) else None
            aliases = [a.strip() for a in declared.group(2).split(',')] if declared and declared.group(2) else []
            found.append({'id': declared.group(1) if declared else None, 'aliases': aliases, 'title': title,
                          'level': len(heading.group(1)), 'start': i, 'end': len(lines)})
        return [t for t in found if t]

    def check_topics(self):
        """Problems with the page's topic IDs: every topic declares a
        well-formed ID beside its heading, and no ID line stands anywhere
        else."""
        lines = self.body.split('\n')
        declared, problems = set(), []
        for topic in self.topics():
            where = f'line {topic["start"] + self.offset + 1}'
            if topic['id'] is None:
                problems.append(f'{where}: "{topic["title"]}" declares no topic ID '
                                '(add <!-- topic: <id> --> on the line after it)')
            elif not all(TOPIC_ID.match(t) for t in [topic['id'], *topic['aliases']]):
                bad = next(t for t in [topic['id'], *topic['aliases']] if not TOPIC_ID.match(t))
                problems.append(f'{where}: topic ID "{bad}" is not lowercase kebab case')
            else:
                declared.add(topic['start'] + 1)
        for i, line, fenced in body_lines(lines):
            if not fenced and TOPIC.match(line) and i not in declared:
                problems.append(f'line {i + self.offset + 1}: a topic ID belongs on the line after '
                                'a `#` or `##` heading')
        return problems


class Readme(Page):
    """README.md's `jadpo check project=...` blocks, checked as an area page's
    are. Its other fences are only illustrations, so they are not compiled."""

    def collect_blocks(self):
        """A file shown without its `use` lines is a block with `hide=use`,
        and a hidden block for the same file with `uses` holds those lines.
        The two are joined before compiling, so what compiles is the shown
        code with its imports in front."""
        super().collect_blocks()
        self.blocks = [b for b in self.blocks if b['attrs'].get('check')]
        uses = {b['attrs']['file']: b for b in self.blocks if b['attrs'].get('uses')}
        for block in self.blocks:
            attrs = block['attrs']
            if attrs.get('hide') == 'use':
                imports = uses.get(attrs['file'])
                if imports is None:
                    self.problems.append(f'line {block["line"] + 1}: hide=use needs a block with `uses` for {attrs["file"]}')
                else:
                    block['source'] = imports['source'].rstrip('\n') + '\n\n' + block['source']
        self.blocks = [b for b in self.blocks if not b['attrs'].get('uses')]

    def check(self, bless=False):
        self.collect_blocks()
        self.check_projects(bless)


class ErrorPage(Page):
    """docs/errors/<rule>.md: one rule's examples, headed by the rule. Its
    area is the catalogue's."""

    def __init__(self, path, catalogue):
        super().__init__(path, catalogue)
        self.rule = path.stem
        self.meta = {'title': self.rule}

    def area(self):
        return int(self.catalogue.get(self.rule, {}).get('area', 0))

    # Whether the page must have the parts docs/writing.md, "How an error
    # page reads", gives it; load_pages sets it from error_format_scope.
    format_required = False

    def check(self, bless=False):
        super().check(bless)
        self.check_rule()
        if self.format_required or ERROR_PARTS[0] in self.body.split('\n'):
            self.problems.extend(error_format_problems(self))
            # It reads like a user page, but shows a fix's changed lines
            # as a fragment its hidden block compiles.
            self.problems.extend(p for p in user_page_problems(self) if 'holds no `fragment`' not in p)

    def check_rule(self):
        """The page names a rule of the catalogue, is headed by it, and
        shows it failing, but for a rule only a command raises."""
        entry = self.catalogue.get(self.rule)
        if entry is None:
            self.problems.append(f'{self.rule} is not a rule in the compiler\'s catalogue (jadpo help --json)')
            return
        if not self.body.startswith(f'# `{self.rule}`\n'):
            self.problems.append(f'the page starts with its heading, # `{self.rule}`')
        has_checks = any(b['attrs'].get('check') and 'expect' in b['attrs'] for b in self.blocks) or bool(self.runs)
        # Only a command raises an area-14 rule, so its CLI tests show it;
        # once its page has a check block that fails or a ```sh run block,
        # one of them must show the rule. A page whose output is copied
        # from a run no check can make compiles its project's check blocks
        # all the same (docs/writing.md, "How an error page reads").
        needs_example = entry.get('area') != COMMAND_AREA or has_checks
        if needs_example and self.rule not in self.covered_rules():
            what = 'a ```sh run block' if entry.get('area') == COMMAND_AREA else 'a check block'
            self.problems.append(f'no example for {self.rule}: add {what} failing with it')
        if not has_checks and entry.get('area') == COMMAND_AREA and NO_EXAMPLE not in self.body \
                and '```' not in self.body:
            self.problems.append(f'say "{NO_EXAMPLE}" on a page with no example')


# An error page's sections, in order (docs/writing.md, "How an error page
# reads"), after its heading and the sentence saying what happened.
ERROR_PARTS = ('## Why Jadpo refuses it', '## What it looks like', '## How to fix it', '## Related')
# What a page says the next step's marker means, in the standard wording
# (docs/writing.md, "The `(human)` and `(agent)` markers").
MARKER_WORDING = {
    'human': '`(human)` at the end of the `next:` line tells your coding agent to stop and ask you '
             'which fix you want, because the choice is yours to make',
    'agent': '`(agent)` at the end of the `next:` line means your coding agent can make the fix itself',
}
NEXT_MARKER = re.compile(r'^\s*next: .*\((human|agent)\)\s*$', re.MULTILINE)
# Where one sentence ends and another starts, outside code.
SENTENCE_BREAK = re.compile(r'[.?!] +[A-Z]')
# A rule named on a page: linked, shown on a `rule:` line, or expected.
RULE_NAMED = re.compile(r'(?:errors/|rule: |expect=)([a-z_]+\.[a-z_]+)')
# A rule named in a teaching example's README, in backticks or linked.
EXAMPLE_RULE = re.compile(r'(?:`|errors/)([a-z_]+\.[a-z_]+)(?:`|\.md)')
# Whether the sentence under an error page's heading must be its rule's
# catalogue summary; a writer's own run before the compiler is rebuilt
# turns it off.
CHECK_SUMMARIES = True
# The prefix of the rules about the command line itself: its commands,
# flags and arguments.
COMMAND_LINE = 'cli.'
# The command that checks source; a rule it raises is shown by a check
# block, not a command's run.
CHECK_COMMAND = 'check'
# Command errors whose pages are still a copied transcript, brought to the
# format by roadmap chunk D13b3c, which empties this.
FORMAT_WAITS = frozenset({
    'install.in_use', 'publish.changelog_missing', 'publish.description_missing',
    'publish.example_failed', 'publish.example_missing',
})


def command_errors(catalogue, commands):
    """The rules only a command raises: those of area 14 that a command
    lists among the rules it raises (`jadpo help --json`), but for those
    `jadpo check` lists."""
    listed = {rule for command in commands for rule in command.get('rules', [])}
    checked = {rule for command in commands if command.get('name') == CHECK_COMMAND
               for rule in command.get('rules', [])}
    return {rule for rule, entry in catalogue.items()
            if entry.get('area') == COMMAND_AREA and rule in listed - checked}


def error_format_scope(catalogue, root=ROOT, commands=()):
    """The rules whose pages must have the parts docs/writing.md, "How an
    error page reads", gives them: those the primer, the Learn path, the
    guides or a teaching example's README names, those whose next step a
    person decides, the command line's own (`cli.*`), and every other rule
    only a command raises, but for FORMAT_WAITS. Every other page follows
    once D13c is built."""
    named = set()
    for path in [root / 'docs/primer.md', *sorted((root / 'docs/learn').glob('*.md')),
                 *sorted((root / 'docs/guides').glob('*.md'))]:
        if path.exists():
            named |= set(RULE_NAMED.findall(path.read_text()))
    for path in sorted((root / 'examples').glob('*/README.md')):
        named |= set(EXAMPLE_RULE.findall(path.read_text()))
    named |= {rule for rule, entry in catalogue.items()
              if entry.get('decides') == 'human' or rule.startswith(COMMAND_LINE)}
    named |= command_errors(catalogue, commands) - FORMAT_WAITS
    return named & catalogue.keys()


def prose_paragraphs(lines, start, end):
    """(first line, text joined by spaces) of each paragraph of prose in
    lines[start:end], outside fences."""
    out, current, first = [], [], None
    for i, line, fenced in body_lines(lines[start:end]):
        if fenced or not line.strip():
            if current:
                out.append((first + start, ' '.join(current)))
            current = []
            continue
        if not current:
            first = i
        current.append(line.strip())
    if current:
        out.append((first + start, ' '.join(current)))
    return out


def fences(lines, start, end):
    """(opening line, closing line, info) of each fence in lines[start:end]."""
    out, opening = [], None
    for i, line, _ in body_lines(lines[start:end]):
        if not line.startswith('```'):
            continue
        if opening is None:
            opening = (i + start, line[3:].strip())
        else:
            out.append((opening[0], i + start, opening[1]))
            opening = None
    return out


def error_format_problems(page):
    """Problems with an error page against docs/writing.md, "How an error
    page reads": what happened in one sentence, its rule's catalogue
    summary; then why Jadpo refuses it; the example and its output, and
    what the next step's marker means in the standard wording; each fix
    as **If …**, its code and what it changes, a fragment of changed lines
    followed by a hidden check block of the fixed file it is part of; and
    the related topics, linked."""
    problems = []
    lines = page.body.split('\n')
    entry = page.catalogue.get(page.rule, {})

    def where(i):
        return f'line {i + page.offset + 1}'

    heads = [(i, line.strip()) for i, line, fenced in body_lines(lines) if not fenced and line.startswith('## ')]
    found = [heading for _, heading in heads]
    if found != list(ERROR_PARTS):
        problems.append(f'its sections are {", ".join(found) or "none"}; an error page has '
                        f'{", ".join(ERROR_PARTS)}, in that order (docs/writing.md, "How an error page reads")')
        return problems
    spans = {heading: (i, heads[n + 1][0] if n + 1 < len(heads) else len(lines))
             for n, (i, heading) in enumerate(heads)}
    what = prose_paragraphs(lines, 1, heads[0][0])
    if len(what) != 1 or fences(lines, 1, heads[0][0]):
        problems.append(f'say what happened in one sentence under the heading, before `{ERROR_PARTS[0]}`')
    else:
        i, text = what[0]
        if not text.endswith('.') or SENTENCE_BREAK.search(re.sub(r'`[^`]*`', '', text)):
            problems.append(f'{where(i)}: say what happened in one sentence, ending with a full stop')
        elif CHECK_SUMMARIES and entry.get('summary') and text != entry['summary'] + '.':
            problems.append(f'{where(i)}: what happened is the rule\'s catalogue summary with a full stop; '
                            f'the summary is "{entry["summary"]}" (crates/jadpo-diag/src/lib.rs)')
    for heading in (ERROR_PARTS[0], ERROR_PARTS[2]):
        start, end = spans[heading]
        if not prose_paragraphs(lines, start + 1, end):
            problems.append(f'{where(start)}: `{heading}` says something in prose')

    start, end = spans[ERROR_PARTS[1]]
    shown = fences(lines, start + 1, end)
    expected = [b for b in [*page.blocks, *page.runs] if start < b['line'] < end
                and (b['attrs'].get('check') or b['attrs'].get('run'))
                and b['attrs'].get('expect') == page.rule and 'output' in b]
    expected.sort(key=lambda b: b['line'])
    command = entry.get('area') == COMMAND_AREA and any(info.startswith('text') for _, _, info in shown)
    if not expected and not command:
        problems.append(f'{where(start)}: `{ERROR_PARTS[1]}` shows a check block with expect={page.rule} '
                        'and its output')
    else:
        after = expected[0]['output']['end'] if expected else next(c for _, c, info in shown if info.startswith('text'))
        said = ' '.join(re.sub(r'\s+', ' ', text) for _, text in prose_paragraphs(lines, after + 1, end))
        outputs = '\n'.join('\n'.join(lines[o:c]) for o, c, info in shown if info.startswith('text'))
        markers = sorted(set(NEXT_MARKER.findall(outputs))) or [entry.get('decides', 'agent')]
        for marker in markers:
            if MARKER_WORDING[marker] not in said:
                problems.append(f'{where(start)}: after the output, say what `({marker})` means in the standard '
                                f'wording: "{MARKER_WORDING[marker]}" (docs/writing.md)')

    start, end = spans[ERROR_PARTS[2]]
    fixes = [i for i, line, fenced in body_lines(lines[:end]) if start < i and not fenced and line.startswith('**If ')]
    if not fixes:
        problems.append(f'{where(start)}: `{ERROR_PARTS[2]}` gives each fix as "**If …**", then its code')
    for n, first in enumerate(fixes):
        last = fixes[n + 1] if n + 1 < len(fixes) else end
        code = fences(lines, first, last)
        if not code:
            problems.append(f'{where(first)}: the fix shows its code')
            continue
        if not prose_paragraphs(lines, code[-1][1] + 1, last):
            problems.append(f'{where(first)}: after the fix\'s code, say what it changes once made')
        blocks = [b for b in page.blocks if first < b['line'] < last]
        for k, block in enumerate(blocks):
            if not block['attrs'].get('fragment'):
                continue
            # `...` stands for the lines around the change, which the
            # fragment shows so it parses on its own.
            shown_lines = [line.strip() for line in block['source'].split('\n') if line.strip() not in ('', '...')]
            proof = next((b for b in blocks[k + 1:] if b['attrs'].get('check') and b['attrs'].get('hidden')
                          and 'expect' not in b['attrs']), None)
            whole = {line.strip() for line in proof['source'].split('\n')} if proof else set()
            if proof is None or not all(line in whole for line in shown_lines):
                problems.append(f'{where(block["line"])}: a fix\'s fragment is followed by a hidden check block '
                                'of the fixed file, holding its lines, so the gate compiles the fix')

    start, end = spans[ERROR_PARTS[3]]
    if not any(LINK.search(text) for _, text in prose_paragraphs(lines, start + 1, end)):
        problems.append(f'{where(start)}: `{ERROR_PARTS[3]}` links the topics the error touches, by title')
    return problems


class Notes(Page):
    """docs/language/notes/NN-*.md: an area's design notes, whose blocks are
    checked as the area page's are."""


class Primer(Page):
    """docs/primer.md: the page an agent reads before writing an app
    (docs/documentation.md). Its examples all compile, and its topic ID is
    `primer`, so it declares none."""

    def check(self, bless=False):
        super().check(bless)
        self.check_primer()

    def check_primer(self):
        for block in self.blocks:
            if block['attrs'].get('fragment'):
                self.problems.append(f'the fragment at line {block["line"] + self.offset + 1}: every example '
                                     'in the primer is compiled, so make it a check block')
        for i, line, fenced in body_lines(self.body.split('\n')):
            if not fenced and TOPIC.match(line):
                self.problems.append(f'line {i + self.offset + 1}: the primer\'s topic ID is `primer`, '
                                     'so it declares none')
        tokens = primer_tokens(self.body)
        low, high = PRIMER_TOKENS
        if not low <= tokens <= high:
            self.problems.append(f'the primer is about {tokens} tokens (its words times 1.3), '
                                 f'outside {low:,} to {high:,}')


class Databases:
    """New, empty databases for ```sh run database blocks, on the server
    JADPO_TEST_DATABASE_URL names or a disposable one: jadpo-test-server
    (crates/jadpo-test), started the first time a block asks and kept until
    this process ends. Each line written to it is answered with a new
    database's `postgresql://` URL, or `error: …`."""

    def __init__(self):
        self.lock = threading.Lock()
        self.process = None
        self.failed = None

    def create(self, stem):
        with self.lock:
            if self.process is None and self.failed is None:
                self.start()
            if self.failed:
                return f'error: {self.failed}'
            self.process.stdin.write(f'docs_{stem}\n')
            self.process.stdin.flush()
            answer = self.process.stdout.readline().strip()
            return answer or 'error: jadpo-test-server stopped'

    def start(self):
        server = server_binary()
        if server is None:
            self.failed = 'jadpo-test-server could not be built'
            return
        self.process = subprocess.Popen([str(server)], cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True)
        admin = self.process.stdout.readline()
        if not admin:
            self.failed = self.process.stderr.read().strip() or 'jadpo-test-server stopped'
            self.process = None
            return
        atexit.register(self.stop)

    def stop(self):
        if self.process:
            self.process.stdin.close()
            self.process.wait()
            self.process = None


DATABASES = Databases()


def server_binary():
    """jadpo-test-server, built from this checkout: beside the `jadpo`
    JADPO_BIN names (tools/verify.py builds both), or else one built here."""
    ensure_jadpo()
    if not os.environ.get('JADPO_BIN'):
        code, _ = run(['cargo', 'build', '--quiet', '--locked', '-p', 'jadpo-test', '--bin', 'jadpo-test-server'],
                      cwd=ROOT)
        if code:
            return None
    server = Path(JADPO).with_name('jadpo-test-server')
    return server if server.exists() else None


def write_project(root, name, blocks):
    """Writes a project's blocks into `root`, in order, making each
    `migrate=` block's migration once the blocks up to it are written;
    problems found doing it."""
    problems, steps = [], 0
    for block in blocks:
        target = root / block['attrs']['file']
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(block['source'])
        purpose = block['attrs'].get('migrate')
        if purpose:
            migrated, said = jadpo(['migrate', 'new', purpose], cwd=str(root),
                                   env={'SOURCE_DATE_EPOCH': str(MIGRATION_EPOCH + 60 * steps)})
            steps += 1
            if migrated:
                problems.append(f'{name}: jadpo migrate new {purpose} failed:\n{said}')
    return problems


def run_commands(block, project, home, database=None):
    """Runs a ```sh run block's commands with `sh -e` in `project`, as a
    reader would: `jadpo` is this checkout's, in plain mode, and HOME is
    an empty folder of its own, so nothing of the machine's leaks in;
    `database`, a `postgresql://` URL, is given in DATABASE_URL. A
    `tty` block runs in a terminal and types its `typed=` value, or
    nothing, then Enter, at the first prompt. (exit code, what it printed
    with paths as the reader's), or None when it didn't finish."""
    ensure_jadpo()
    bin_dir = home.parent / 'bin'
    bin_dir.mkdir()
    (bin_dir / 'jadpo').symlink_to(Path(JADPO).resolve())
    env = {'PATH': f'{bin_dir}{os.pathsep}{os.environ.get("PATH", "/usr/bin:/bin")}', 'HOME': str(home),
           'JADPO_PLAIN': '1', 'JADPO_TOOLCHAIN': 'local', 'SOURCE_DATE_EPOCH': str(MIGRATION_EPOCH)}
    for name in ('TMPDIR', 'JADPO_TEST_DATABASE_URL'):
        if os.environ.get(name):
            env[name] = os.environ[name]
    if database:
        env['DATABASE_URL'] = database
    command = ['sh', '-e', '-c', block['source']]
    attrs = block['attrs']
    if attrs.get('tty'):
        typed = attrs.get('typed')
        result = in_terminal(command, project, env, '' if typed in (None, True) else typed)
    else:
        try:
            done = subprocess.run(command, cwd=project, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT, timeout=RUN_LIMIT)
        except subprocess.TimeoutExpired:
            return None
        result = (done.returncode, done.stdout)
    if result is None:
        return None
    code, out = result
    text = out.decode('utf-8', 'replace').replace('\r\n', '\n')
    for path, shown in ((project, '.'), (home, '~'), (project.parent, '.')):
        # The resolved spelling first, since the other is part of it
        # (/private/var/folders/... and /var/folders/... on macOS).
        for spelling in sorted({str(path), os.path.realpath(path)}, key=len, reverse=True):
            text = text.replace(spelling + '/', '' if shown == '.' else shown + '/').replace(spelling, shown)
    text = without_run_time('\n'.join(line.rstrip() for line in text.split('\n')))
    return code, text.strip('\n') + '\n'


def in_terminal(command, cwd, env, typed):
    """Runs `command` in a pseudo-terminal, typing `typed` and Enter once a
    prompt shows and the terminal has stopped echoing (or ECHO_WAIT has
    passed, for a prompt that doesn't hide what is typed). (exit code,
    bytes printed), or None when it didn't finish in RUN_LIMIT."""
    import termios
    master, slave = os.openpty()
    process = subprocess.Popen(command, cwd=cwd, env=env, stdin=slave, stdout=slave, stderr=slave,
                               start_new_session=True)
    out, prompted, sent, start = b'', None, False, time.monotonic()
    try:
        while process.poll() is None:
            if time.monotonic() - start > RUN_LIMIT:
                process.kill()
                process.wait()
                return None
            if select.select([master], [], [], 0.02)[0]:
                out += os.read(master, 65536)
            if sent:
                continue
            if prompted is None and PROMPT.search(out.decode('utf-8', 'replace')):
                prompted = time.monotonic()
            if prompted is not None:
                hidden = not termios.tcgetattr(slave)[3] & termios.ECHO
                if hidden or time.monotonic() - prompted > ECHO_WAIT:
                    os.write(master, typed.encode() + b'\n')
                    sent = True
        os.close(slave)
        slave = None
        while select.select([master], [], [], 0.1)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            out += chunk
    finally:
        if slave is not None:
            os.close(slave)
        os.close(master)
    return process.returncode, out


def keep_records(tmp, blocks):
    """`jadpo records update` in the project at `tmp`, as a reader runs it,
    then each record a block shows written back over what it wrote, so a
    shown record is checked as shown. A project whose blocks show a
    record's own error (`record.*`) keeps only the records that hold who can
    do what and the outside hosts, so the record it is about stays as the
    page has it."""
    root = Path(tmp)
    before = {p for p in root.rglob('*.jadpo_record')}
    jadpo(['records', 'update'], cwd=tmp)
    if any(b['attrs'].get('expect', '').startswith('record.') for b in blocks):
        for path in set(root.rglob('*.jadpo_record')) - before:
            if path.name not in ('access.jadpo_record', 'network.jadpo_record'):
                path.unlink()
    for block in blocks:
        if block['attrs']['file'].endswith('.jadpo_record'):
            (root / block['attrs']['file']).write_text(block['source'])
    return []


class LearnPage(Page):
    """docs/learn/: the Learn path, one app built a page at a time
    (docs/documentation.md). A page shows only the files it adds or
    changes; its project is the one the pages before it built, less the
    files its front matter `removes:`, with its own on top. A reader makes
    the migrations and the records with commands, so a page's check
    makes them too: `jadpo migrate new initial` and `jadpo records update`,
    with any record the page shows written back over it. A record is never
    inherited, since an
    earlier page's is out of date once the routes change. Each page is
    one topic, its `#` heading."""

    def __init__(self, path, catalogue, inherited=None, root=ROOT):
        super().__init__(path, catalogue)
        # The files the pages before this one built: path -> source.
        self.inherited = inherited or {}
        self.root = root

    def removes(self):
        return set(str(self.meta.get('removes', '')).split())

    def own_files(self):
        """The files this page's check blocks write, in order."""
        found = {}
        for block in self.blocks:
            if block['attrs'].get('check') and not block['attrs'].get('inherited'):
                found[block['attrs']['file']] = block['source']
        return found

    def collect_blocks(self):
        super().collect_blocks()
        own = [b for b in self.blocks if b['attrs'].get('check')]
        if not own:
            return
        project = own[0]['attrs']['project']
        shown = {b['attrs']['file'] for b in own}
        inherited = [{'line': -1, 'end': -1, 'source': source,
                      'attrs': {'check': True, 'project': project, 'file': file, 'inherited': True}}
                     for file, source in self.inherited.items() if file not in shown]
        self.blocks = inherited + self.blocks

    def prepare(self, tmp, name, blocks):
        problems = []
        code, said = jadpo(['migrate', 'new', 'initial'], cwd=tmp, env={'SOURCE_DATE_EPOCH': str(MIGRATION_EPOCH)})
        if code:
            problems.append(f'{name}: jadpo migrate new initial failed:\n{said}')
        return problems + keep_records(tmp, blocks)

    def area(self):
        return 0

    def topics(self):
        lines = self.body.split('\n')
        for i, line, fenced in body_lines(lines):
            heading = HEADING.match(line) if not fenced else None
            if heading and len(heading.group(1)) == 1:
                declared = TOPIC.match(lines[i + 1]) if i + 1 < len(lines) else None
                aliases = [a.strip() for a in declared.group(2).split(',')] if declared and declared.group(2) else []
                return [{'id': declared.group(1) if declared else None, 'aliases': aliases,
                         'title': heading.group(2), 'level': 1, 'start': i, 'end': len(lines)}]
        return []


def load_learn(catalogue, root=ROOT):
    """The Learn path in order, its outline first, each page holding the
    files the pages before it built."""
    learn = root / 'docs/learn'
    paths = ([learn / 'README.md'] if (learn / 'README.md').exists() else []) + sorted(learn.glob('[0-9][0-9]-*.md'))
    pages, built = [], {}
    for path in paths:
        page = LearnPage(path, catalogue, root=root)
        inherited = {f: s for f, s in built.items() if f not in page.removes()}
        page.inherited = inherited
        Page.collect_blocks(page)
        built = dict(inherited)
        built.update({f: s for f, s in page.own_files().items() if not f.endswith('.jadpo_record')})
        page.blocks = []
        pages.append(page)
    return pages


class GuidePage(LearnPage):
    """docs/guides/: one job done from start to finish
    (docs/documentation.md). Each guide is one topic, its `#` heading,
    and a whole project of its own: it shows every file its project
    needs, built on nothing before it, and its check makes the
    migrations and the records as a reader's commands do, as a Learn
    page's does."""


def load_guides(catalogue, root=ROOT):
    """The guides in order, their index first."""
    guides = root / 'docs/guides'
    paths = ([guides / 'README.md'] if (guides / 'README.md').exists() else []) + sorted(guides.glob('[0-9][0-9]-*.md'))
    return [GuidePage(path, catalogue, root=root) for path in paths]


# What docs/writing.md says a user page never does, outside code.
NUMBERED = re.compile(r'\b(?:areas?|questions?) \d+', re.IGNORECASE)
BUILD_STATUS = re.compile(r"\b(?:not built yet|isn't built|aren't built|not yet built)\b|not_built_yet",
                          re.IGNORECASE)
SPEC_WORDS = re.compile(r'^Decided\.|\bcarried to\b')
NOT_BUILT = '## Not built yet'
REPOSITORY_URL = re.compile(r'^https://github\.com/jadpo-dev/jadpo/(?:blob|tree)/')


def user_page_problems(page, root=ROOT):
    """Problems with a user page (the primer, the Learn path, a guide) against the
    mechanical rules of docs/writing.md: no area or question by number, build
    status only in a final `## Not built yet` section, no spec vocabulary,
    no `fragment`, and links only to pages of the docs."""
    problems = []
    lines = page.body.split('\n')
    sections = [i for i, line, fenced in body_lines(lines) if not fenced and line.startswith('## ')]
    note = next((i for i in sections if lines[i].strip() == NOT_BUILT), None)
    if note is not None and note != sections[-1]:
        problems.append(f'`{NOT_BUILT}` is the last section of a page')
    for i, line, fenced in body_lines(lines):
        if fenced:
            continue
        prose = re.sub(r'`[^`]*`', '', line)
        where = f'line {i + page.offset + 1}'
        for match in NUMBERED.finditer(prose):
            problems.append(f'{where}: "{match.group(0)}": link the topic by its title instead')
        if BUILD_STATUS.search(prose) and (note is None or i < note):
            problems.append(f'{where}: build status belongs in the final `{NOT_BUILT}` section')
        if SPEC_WORDS.search(prose):
            problems.append(f'{where}: spec vocabulary ("Decided.", "carried to") belongs on area pages')
    for block in page.blocks:
        if block['attrs'].get('fragment'):
            problems.append(f'line {block["line"] + page.offset + 1}: a user page holds no `fragment`')
    docs_dir = (root / 'docs').resolve()
    for number, target in links_in(page.body):
        where = f'line {number + page.offset}'
        if REPOSITORY_URL.match(target):
            problems.append(f'{where}: {target} links a file in this repository; link a page of the docs')
            continue
        if re.match(r'^[a-z][a-z0-9+.-]*:', target) or target.startswith('#'):
            continue
        resolved = (page.path.parent / target.partition('#')[0]).resolve()
        allowed = (resolved == docs_dir / 'primer.md'
                   or (resolved.parent == docs_dir / 'language' and re.match(r'^\d\d-.*\.md$', resolved.name))
                   or (resolved.parent == docs_dir / 'errors' and resolved.suffix == '.md')
                   or (resolved.parent in (docs_dir / 'learn', docs_dir / 'guides')
                       and resolved.suffix == '.md'))
        if not allowed:
            problems.append(f'{where}: {target} links a file in this repository, which a reader can\'t open; '
                            'link a page of the docs')
    return problems


# An area page names no area or question by number in what an app author
# reads either (docs/writing.md, "How a reference topic reads"). This fails
# on areas 1 to this one, whose pages follow the guide, and warns on any
# after it. It is the last area, so the rule fails on every area page.
NUMBERED_FAILS_THROUGH = 21


def numbered_references(page):
    """(line, text) for each area or question an area page names by
    number outside code. Its design notes are left alone: they are where
    question numbers belong."""
    found = []
    for i, line, fenced in body_lines(page.body.split('\n')):
        if fenced:
            continue
        for match in NUMBERED.finditer(re.sub(r'`[^`]*`', '', line)):
            found.append((i + page.offset + 1, match.group(0)))
    return found


def body_lines(lines):
    """(index, line, inside a fence) for each line, fences as the block
    collector reads them: a line starting ``` opens one and the next closes
    it."""
    fenced = False
    for i, line in enumerate(lines):
        if line.startswith('```'):
            yield i, line, True
            fenced = not fenced
            continue
        yield i, line, fenced


HEADING = re.compile(r'^(#{1,6}) (.+?)\s*$')
LINK = re.compile(r'(?<!!)\[((?:[^\[\]]|\[[^\]]*\])*)\]\(([^)\s]+)\)')


def github_slug(text, seen):
    """The anchor GitHub gives a heading, counting repeats in `seen`."""
    text = re.sub(r'<[^>]+>', '', text)
    text = re.sub(r'\[([^\]]*)\]\([^)]*\)', r'\1', text)
    text = re.sub(r'[`*]', '', text).strip().lower()
    text = re.sub(r'[^\w\- ]', '', text).replace(' ', '-')
    count = seen.get(text, 0)
    seen[text] = count + 1
    return text if not count else f'{text}-{count}'


def headings_of(text):
    """(line, level, title, GitHub anchor) of each heading outside fences."""
    seen, out = {}, []
    for i, line, fenced in body_lines(text.split('\n')):
        heading = None if fenced else HEADING.match(line)
        if heading:
            out.append((i, len(heading.group(1)), heading.group(2), github_slug(heading.group(2), seen)))
    return out


def anchors_of(text):
    found = {anchor for _, _, _, anchor in headings_of(text)}
    return found | set(re.findall(r'<a (?:id|name)="([^"]+)"', text))


def map_links(text, change):
    """`text` with each link's target outside code passed through
    `change(target)`; returns the new text."""
    out = []
    for _, line, fenced in body_lines(text.split('\n')):
        if fenced:
            out.append(line)
            continue
        spans = code_spans(line)
        out.append(LINK.sub(lambda m: m.group(0) if in_code(m.start(), spans)
                            else f'[{m.group(1)}]({change(m.group(2))})', line))
    return '\n'.join(out)


def code_spans(line):
    return [m.span() for m in re.finditer(r'`[^`]*`', line)]


def in_code(at, spans):
    return any(start <= at < end for start, end in spans)


def links_in(text):
    """(line number, target) of each link outside code; a link's text may
    hold code."""
    for i, line, fenced in body_lines(text.split('\n')):
        if not fenced:
            spans = code_spans(line)
            for m in LINK.finditer(line):
                if not in_code(m.start(), spans):
                    yield i + 1, m.group(2)


def markdown_files(root=ROOT):
    """The repository's Markdown and llms.txt, tracked or new."""
    listed = subprocess.run(['git', 'ls-files', '-co', '--exclude-standard', '*.md', 'llms.txt'],
                            cwd=root, capture_output=True, text=True)
    if listed.returncode:
        return sorted(p for p in root.rglob('*.md') if not {'target', 'build', 'node_modules'} & set(p.parts))
    return sorted(root / name for name in set(listed.stdout.split()) if (root / name).exists())


def check_links(files, root=ROOT):
    """Problems with the relative links in `files`: a file that isn't
    there, or an anchor no heading of a Markdown target has. llms.txt's
    links name the repository's main branch, so they are read as paths."""
    anchors, problems = {}, []
    for path in files:
        text = path.read_text()
        for number, target in links_in(text):
            for prefix in RAW_URLS:
                if target.startswith(prefix):
                    target = str(root / target[len(prefix):])
                    break
            else:
                if re.match(r'^[a-z][a-z0-9+.-]*:', target):
                    continue
            name, _, fragment = target.partition('#')
            resolved = (path.parent / name).resolve() if name else path.resolve()
            where = f'{path.relative_to(root)}:{number}'
            if not resolved.exists():
                problems.append(f'{where}: links to {target}, which isn\'t there')
            elif fragment and resolved.suffix == '.md':
                if resolved not in anchors:
                    anchors[resolved] = anchors_of(resolved.read_text())
                if fragment not in anchors[resolved]:
                    problems.append(f'{where}: links to {target}, but {resolved.name} has no heading '
                                    f'with that anchor')
    return problems


ERROR_PAGES = 'https://jadpo.dev/docs/errors/'
HELP_LINE = re.compile(r'^\s*help(?::\s+|\s{2,})(\S+)\s*$|"help"\s*:\s*"([^"]*)"')


def check_help_lines(files, catalogue, root=ROOT):
    """Every `help` line the docs show, in an error's output or its JSON,
    is a rule's page on the site, `https://jadpo.dev/docs/errors/<rule>`,
    of a rule the catalogue has, as the compiler prints it."""
    problems = []
    for path in files:
        for number, line in enumerate(path.read_text().split('\n'), 1):
            match = HELP_LINE.search(line)
            if not match:
                continue
            target = match.group(1) or match.group(2)
            where = f'{path.relative_to(root)}:{number}'
            if not target.startswith(ERROR_PAGES):
                problems.append(f'{where}: help line names {target}, not a rule\'s page on the site ({ERROR_PAGES}<rule>)')
            elif target[len(ERROR_PAGES):] not in catalogue:
                problems.append(f'{where}: help line names {target}, a rule the catalogue doesn\'t have')
    return problems


# --- the bundle --------------------------------------------------------------

def words(text):
    return len(text.split())


def without_hidden(text):
    """Markdown with its `hidden` ```jadpo blocks left out, and the blank
    line before each, as the bundle gives a page."""
    lines, out, i = text.split('\n'), [], 0
    while i < len(lines):
        match = FENCE.match(lines[i])
        if match and match.group(1) == 'jadpo' and parse_attrs(match.group(2)).get('hidden'):
            i += 1
            while not lines[i].startswith('```'):
                i += 1
            if out and not out[-1].strip():
                out.pop()
        else:
            out.append(lines[i])
        i += 1
    return re.sub(r'\n{3,}', '\n\n', '\n'.join(out))


def bare_fences(text):
    """Markdown with each code fence's opening line kept to its language
    (`jadpo`, `text`), as the bundle gives a page: the attributes after it
    (`check project=... file=...`, `fragment`, `output`) are for the docs
    check, never for a reader. What a fence holds is left as it is."""
    out, fence = [], None
    for line in text.split('\n'):
        match = re.match(r'^(`{3,})(.*)$', line)
        if fence is None and match:
            fence = match.group(1)
            words = match.group(2).split()
            line = fence + (words[0] if words else '')
        elif fence is not None and match and match.group(1).startswith(fence) and not match.group(2).strip():
            fence = None
        out.append(line)
    return '\n'.join(out)


def primer_tokens(text):
    """The primer's size as the bundle gives it: its words times 1.3."""
    return round(words(without_hidden(text)) * 1.3)


def plain(text):
    """Markdown inline text as plain words."""
    text = re.sub(r'\[([^\]]*)\]\([^)]*\)', r'\1', text)
    return re.sub(r'\*\*?([^*]+)\*\*?', r'\1', text).strip()


def summary_of(markdown, fallback):
    """The first sentence of a page's first paragraph of prose, at most 200
    characters."""
    paragraph = []
    for _, line, fenced in body_lines(markdown.split('\n')[1:]):
        if fenced or line.startswith(('#', '|', '>', '<!--', '- ', '* ')) or re.match(r'^\d+\. ', line):
            if paragraph:
                break
            continue
        if not line.strip():
            if paragraph:
                break
            continue
        paragraph.append(line.strip())
    text = plain(' '.join(paragraph))
    if not text:
        return fallback
    sentence = re.split(r'(?<=[.!?])\s+(?=[A-Z`*])', text)[0]
    if len(sentence) > 200:
        sentence = sentence[:200].rsplit(' ', 1)[0] + '…'
    return sentence


# The agent skill (docs/documentation.md, "The agent skill"): where it is
# written, and where the bundle holds it.
SKILL = Path('docs/agent-skill/SKILL.md')
SKILL_IN_BUNDLE = Path('skills/jadpo/SKILL.md')

# What `--json` gives, as every command page says it
# (docs/cli-design-language.md, "Plain and `--json`").
JSON_OUTPUT = (
    '`jadpo {name} --json` writes one JSON document to standard output instead of text, '
    'with the same exit code. The document has `schemaVersion`, `command` (`"{name}"`), '
    '`ok`, `data` (what the command produced, `null` when it failed) and `diagnostics` '
    '(each error with its `rule`, `reason`, `next` and `help`). '
    'A command that needs a terminal refuses `--json` and names the flags that do the same job.'
)


# The standard type reference (docs/documentation.md, "Standard types"):
# a page per standard type, rendered from `jadpo help --json`'s `types` and
# `operations`, which are the compiler's own contract data, and from each
# type's golden file, which pins every form its examples take.
GOLDENS = Path('crates/jadpo-interp/tests/goldens/standard')
# A standard type's page ID is this and its name in kebab case; their index
# is TYPES_INDEX.
TYPE_PREFIX = 'type-'
TYPES_INDEX = 'types'
# The area page topics a type's page links: the contract every type keeps,
# and the list it explains.
CONTRACT_TOPIC = 'every-standard-type-meets-one-contract'
STANDARD_TYPES_TOPIC = 'the-standard-types'
# An enum with more variants than this lists them in a paragraph, not a
# table, and one with more than VARIANTS_LISTED names only a few.
VARIANTS_IN_TABLE = 40
VARIANTS_LISTED = 500
# The area a type's description cites first: `(area 19, ISO 13616)`.
CITED_AREA = re.compile(r'\(area (\d+)')


def type_id(name):
    """A standard type's page ID: `type-` and its name in kebab case,
    `type-uk-sort-code` for `UkSortCode`."""
    words = re.sub(r'(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])', '-', name)
    return TYPE_PREFIX + words.lower()


def without_area_numbers(text):
    """A stub's description as a reader sees it: the area it cites by
    number left out, the standard it names kept, `(ISO 13616)`."""
    text = re.sub(r';\s*area \d+, "[^"]*"', '', text)
    text = re.sub(r'\s*\(area \d+(?:\'s [^)]*)?\)', '', text)
    text = re.sub(r'\s*\(area \d+, "[^"]*"\)', '', text)
    return re.sub(r'\(area \d+, ', '(', text)


def read_golden(path):
    """A standard type's golden file: its columns, each (suffix, type,
    comment), the suffix '' for a type stored in one column, and each
    example's forms, its text ('none' where it has none), its JSON and each
    column's value as SQL, by suffix."""
    lines = path.read_text().split('\n')
    columns, examples = [], []
    for line in lines[1:]:
        if line.startswith('column '):
            rest, _, comment = line[len('column '):].partition(' -- ')
            columns.append((rest, comment or None))
        elif line.startswith('example '):
            examples.append({'example': line[len('example '):], 'text': None, 'json': None, 'stored': []})
        elif examples and line.startswith('text '):
            examples[-1]['text'] = line[len('text '):]
        elif examples and line.startswith('json '):
            examples[-1]['json'] = line[len('json '):]
        elif examples and line.startswith('stored '):
            examples[-1]['stored'].append(line[len('stored '):])
    # One column is the field's own, unsuffixed; several each have one.
    single = len(columns) == 1
    named = []
    for rest, comment in columns:
        suffix, ty = ('', rest) if single else rest.split(' ', 1)
        named.append((suffix, ty, comment))
    for example in examples:
        stored = example['stored']
        if stored == ['never']:
            example['stored'] = []
        elif single:
            example['stored'] = [('', stored[0])] if stored else []
        else:
            example['stored'] = [tuple(value.split(' ', 1)) for value in stored]
    return {'columns': named, 'examples': examples}


def cell(text):
    """`text` as code in a table's cell; text with a line break or a tab
    as a JSON string, `"two\\nlines"`, so the cell stays one line."""
    if re.search(r'[\x00-\x1f]', text):
        text = json.dumps(text, ensure_ascii=False)
    text = text.replace('|', '\\|')
    fence = '``' if '`' in text else '`'
    pad = ' ' if fence == '``' else ''
    return f'{fence}{pad}{text}{pad}{fence}' if text else '`""`'


def code(text):
    return cell(text).replace('\\|', '|')


def joined(items, last='and'):
    items = list(items)
    if len(items) <= 1:
        return ''.join(items)
    return ', '.join(items[:-1]) + f' {last} ' + items[-1]


def first_sentence(text):
    return re.split(r'(?<=[.!?])\s+(?=[A-Z`*])', text)[0] if text else ''


class TypeReference:
    """The standard type pages (docs/documentation.md, "Standard types"):
    one per entry of `jadpo help --json`'s `types`, in its groups' order,
    and their index."""

    def __init__(self, help_data, root=ROOT):
        self.root = root
        self.types = list(help_data.get('types', []))
        self.operations = {op['name']: op for op in help_data.get('operations', [])}
        self.by_name = {entry['name']: entry for entry in self.types}
        self.groups = []
        for entry in self.types:
            if entry.get('group') not in self.groups:
                self.groups.append(entry.get('group'))
        self.problems = []

    def in_order(self):
        """The types in their groups' order, as `jadpo help --json` gives
        them."""
        return self.types

    def summary(self, entry):
        text = without_area_numbers(entry.get('summary') or '')
        if re.search(r'\barea \d+|\bquestion \d+', text):
            self.problems.append(f'{entry["name"]}: its description in std/standard.jadpo names an area or question '
                                 f'by number in a way the type reference can\'t leave out')
        return text

    def area(self, entry):
        cited = CITED_AREA.search(entry.get('summary') or '')
        return int(cited.group(1)) if cited else 19

    def golden(self, entry):
        path = self.root / GOLDENS / f'{entry["name"]}.golden'
        if not path.exists():
            self.problems.append(f'{entry["name"]} has no golden file, {GOLDENS / (entry["name"] + ".golden")}')
            return {'columns': [], 'examples': []}
        return read_golden(path)

    def type_link(self, written, linked):
        """A type as written, `List<Money>?`, with each standard type in it
        linking its page."""
        def one(match):
            name = match.group(0)
            if name in self.by_name:
                linked.add(type_id(name))
                return f'[`{name}`]({type_id(name)}.md)'
            return f'`{name}`'
        parts = re.split(r'([A-Za-z][A-Za-z0-9_.]*)', written)
        out = []
        for part in parts:
            if re.fullmatch(r'[A-Za-z][A-Za-z0-9_.]*', part or ''):
                out.append(re.sub(r'.+', one, part))
            elif part:
                out.append(f'`{part}`')
        return ''.join(out).replace('``', '')

    def written_name(self, entry):
        """The type as a program names it: `Range<CalendarDate>` for `Range`,
        with the type its examples hold."""
        if entry['holds'] == 'range' and entry.get('examples', {}).get('of'):
            return f'{entry["name"]}<{entry["examples"]["of"]}>'
        return entry['name']

    def signature(self, name):
        operation = self.operations.get(name, {})
        params = ', '.join(f'{p["name"]}: {p["type"]}{"?" if p.get("optional") else ""}'
                           for p in operation.get('params', []))
        return f'{name}({params}) -> {operation.get("result", "")}'

    def make_one(self, entry):
        name, holds = entry['name'], entry['holds']
        valid = entry.get('examples', {}).get('valid', [])
        out = []
        fields = [f['name'] for f in entry.get('fields', []) if not f.get('optional')] or \
                 [f['name'] for f in entry.get('fields', [])]
        if holds == 'text' and entry.get('text') == 'secret':
            out.append('It comes from outside your code, a secret value that never appears in it.')
        elif holds == 'text':
            sample = valid[0]['text'] if valid else 'raw'
            out.append(f'Build one from its text, `{name}("{sample}")`, which checks it: text that isn\'t '
                       f'one breaks `format`.')
        elif holds == 'whole':
            sample = valid[0]['text'] if valid else entry.get('min')
            out.append(f'Build one from a whole number from {entry.get("min")} to {entry.get("max")}, '
                       f'`{name}({sample})`; a number outside them breaks `min` or `max`.')
        elif holds == 'bounded':
            base = entry.get('base')
            out.append(f'It is a `{base}` with bounds of its own: build one as you build a `{base}`, '
                       f'and a value outside its bounds breaks `min` or `max`.')
        elif holds == 'enum':
            variants = entry.get('variants', [])
            if variants:
                out.append(f'Pick one of its variants, `{name}.{variants[0]["name"]}`.')
        elif holds == 'choice':
            variants = entry.get('variants', [])
            if variants:
                first = variants[0]
                given = '  '.join(f'{f["name"]}: ...' for f in first.get('fields', []))
                out.append(f'Pick one of its variants and give its fields, '
                           f'`{name}.{first["name"]} {{ {given} }}`.' if given else
                           f'Pick one of its variants, `{name}.{first["name"]}`.')
        elif holds in ('object', 'range', 'feature', 'featureCollection'):
            shown = self.written_name(entry)
            given = '  '.join(f'{field}: ...' for field in fields[:3])
            out.append(f'Build one from its fields, `{shown} {{ {given} }}`.')
        elif holds == 'list':
            item = (entry.get('arguments') or ['its item'])[0]
            out.append(f'It is a list of `{item}`: build one from a list of them, as you build a named list '
                       f'type of your own.')
        elif holds == 'map':
            key, value = (entry.get('arguments') or ['K', 'V'])[:2]
            out.append(f'It maps `{key}` to `{value}`: build one from a map of them, as you build a named map '
                       f'type of your own.')
        elif holds == 'bytes':
            out.append(f'Build one from `Bytes`, `{name}(bytes)`, which checks them: bytes that aren\'t one '
                       f'break `format`.')
        if entry.get('text') in ('canonical', 'secret'):
            # A few short ones: a long token says little in a sentence.
            malformed = [m for m in entry.get('examples', {}).get('malformed', []) if len(m) <= 40][:3]
            such = f', such as {joined((code(m) for m in malformed), "or")}' if malformed else ''
            out.append(f'`text.parse(raw, as: {self.written_name(entry)})` reads one from its text; text that '
                       f'isn\'t one at all fails `malformed`{such}.')
        gives = [op['name'] for op in entry.get('operations', []) if op.get('gives')]
        if gives:
            out.append(f'{"This operation gives" if len(gives) == 1 else "These operations give"} one: '
                       f'{joined(f"`{g}`" for g in gives)}.')
        rules = entry.get('rules', [])
        text = '\n\n'.join(out)
        if rules:
            text += ('\n\nIts parts keep these rules; a value that breaks one fails with the rule\'s name:\n\n' +
                     '\n'.join(f'- `{r["name"]}`: `{r["written"]}`' for r in rules))
        return text

    def members(self, entry, linked):
        """Its fields, or its variants, as a section."""
        out = []
        if entry.get('fields'):
            out += ['## Fields', '', '| Field | Type |', '|---|---|']
            for field in entry['fields']:
                ty = field['type'] + ('?' if field.get('optional') else '')
                out.append(f'| `{field["name"]}` | {self.type_link(ty, linked)} |')
            out.append('')
        variants = entry.get('variants', [])
        if entry['holds'] == 'choice' and variants:
            out += ['## Variants', '', '| Variant | Fields |', '|---|---|']
            for variant in variants:
                given = ', '.join(f'`{f["name"]}`: {self.type_link(f["type"] + ("?" if f.get("optional") else ""), linked)}'
                                  for f in variant.get('fields', [])) or 'none'
                out.append(f'| `{variant["name"]}` | {given} |')
            out.append('')
        elif entry['holds'] == 'enum' and variants:
            same = all(v['name'] == v['code'] for v in variants)
            out += ['## Variants', '']
            if same:
                out += [f'{len(variants)} variants, each written as its own name: '
                        f'{", ".join(f"`{v["name"]}`" for v in variants)}.', '']
            elif len(variants) <= VARIANTS_IN_TABLE:
                out += ['| Variant | Written |', '|---|---|']
                out += [f'| `{v["name"]}` | {cell(v["code"])} |' for v in variants]
                out.append('')
            elif all(v['name'] == re.sub(r'[^a-z0-9]+', '_', v['code'].lower()).strip('_') for v in variants):
                sample = variants[0]
                out.append(f'{len(variants)} variants. Each is its code in lower case, with `_` for every other '
                           f'character: `{entry["name"]}.{sample["name"]}` is written {code(sample["code"])}.')
                if len(variants) <= VARIANTS_LISTED:
                    out[-1] += f' The codes: {", ".join(code(v["code"]) for v in variants)}.'
                else:
                    out[-1] += (f' The first {VARIANTS_IN_TABLE}: '
                                f'{", ".join(code(v["code"]) for v in variants[:VARIANTS_IN_TABLE])}.')
                out.append('')
            else:
                shown = variants if len(variants) <= VARIANTS_LISTED else variants[:VARIANTS_IN_TABLE]
                more = '' if shown is variants else f' The first {len(shown)}:'
                out += [f'{len(variants)} variants, each with the code its text is written in.{more} '
                        f'{", ".join(f"`{v["name"]}` ({code(v["code"])})" for v in shown)}.', '']
        return out

    def forms(self, entry, golden):
        """How its values are written, sent and stored, from its golden
        file."""
        name = entry['name']
        examples = golden['examples']
        out = ['## Text, JSON and storage', '']
        if entry.get('text') == 'secret':
            out += [f'`text.parse` reads one from its text, as it is typed. Nothing writes it: `text.from` and '
                    f'`text.format` don\'t take it, JSON writes it as {code(examples[0]["json"]) if examples else "redacted"}, '
                    f'and it is never stored.', '']
            return out
        if entry.get('text') == 'canonical':
            out += [f'`text.from` writes its canonical text, the one form `text.parse` reads back. JSON sends '
                    f'{self.json_words(entry)}.', '']
            out += ['| Text | JSON |', '|---|---|']
            out += [f'| {cell(e["text"])} | {cell(e["json"])} |' for e in examples]
        else:
            out += [f'It has no single text form, so `text.from` and `text.parse` don\'t take it. JSON sends '
                    f'{self.json_words(entry)}.', '']
            out += ['| JSON |', '|---|']
            out += [f'| {cell(e["json"])} |' for e in examples]
        out.append('')
        columns = golden['columns']
        # The middle example: the first is often an edge, such as zero.
        first = examples[len(examples) // 2] if examples else None
        written = (first['text'] if first and first['text'] != 'none' else first['json']) if first else ''
        if not columns:
            out += ['It is never stored.', '']
        elif len(columns) == 1:
            _, ty, comment = columns[0]
            what = f', holding the {comment[0].lower() + comment[1:]}' if comment else ''
            out.append(f'It is stored in one column named for the field, of type `{ty}`{what}.')
            if first and first['stored']:
                out[-1] += f' {code(written)} is stored as {code(first["stored"][0][1])}.'
            out.append('')
        else:
            out += [f'It is stored in {len(columns)} columns, each named for the field, `_` and its suffix. '
                    f'This is {code(written)}:', '']
            values = dict(first['stored']) if first else {}
            out += ['| Column | Type | Holds |', '|---|---|---|']
            for suffix, ty, _ in columns:
                out.append(f'| `<field>_{suffix}` | `{ty}` | {cell(values.get(suffix, ""))} |')
            out.append('')
        return out

    def json_words(self, entry):
        return {
            'text': 'its canonical text as a string',
            'object': 'an object of its parts',
            'tagged': 'an object of its variant\'s name, as `tag`, and the variant\'s fields',
            'number': 'it as a number',
            'array': 'an array of its items',
            'keyed': 'an object of its entries, each key written as its text',
            'geojson': 'a GeoJSON geometry, its positions longitude first',
            'feature': 'a GeoJSON object of its geometry and properties',
            'secret': 'it redacted',
        }.get(entry.get('json'), 'one shape')

    def shown(self, entry):
        out = ['## Shown to people', '']
        name = entry['name']
        if entry.get('text') == 'secret':
            return out + [f'`text.format` doesn\'t take it: a `{name}` is never shown.', '']
        options = entry.get('format', {}).get('options', [])
        needed = [f'`{o["name"]}:`' for o in options if o.get('needed')]
        optional = [f'`{o["name"]}:`' for o in options if not o.get('needed')]
        sentence = '`text.format(value, locale:)` shows it to people in their locale.'
        if needed:
            sentence += f' A call also gives {joined(needed)}.'
        if optional:
            sentence += f' A call may also give {joined(optional, "or")}.'
        out += [sentence, '']
        valid = entry.get('examples', {}).get('valid', [])
        if valid:
            given = ['`locale: Locale.en_gb`']
            if any(o['name'] == 'zone' for o in options):
                given.append('`zone: Zone.europe_london`')
            if entry.get('examples', {}).get('format'):
                given.append(f'`format: TimeFormat.{entry["examples"]["format"]}`')
            out += [f'With {joined(given)}:', '', '| Value | Shown |', '|---|---|']
            out += [f'| {cell(e["text"])} | {cell(e["shown"])} |' for e in valid]
            out.append('')
        return out

    def operations_section(self, entry, linked):
        out = ['## Operations', '']
        related = [op['name'] for op in entry.get('operations', [])]
        for op in related:
            summary = (self.operations.get(op) or {}).get('summary') or ''
            out.append(f'- `{self.signature(op)}`' + (f': {summary}' if summary else ''))
        if related:
            out.append('')
        baseline = []
        if entry.get('text') in ('canonical', 'secret'):
            baseline.append('`text.parse`')
        if entry.get('text') == 'canonical':
            baseline.append('`text.from`')
        if entry.get('text') != 'secret':
            baseline.append('`text.format`')
        baseline += ['`json.read`', '`json.write`', '`==`']
        linked.add(CONTRACT_TOPIC)
        lead = 'Like every standard type, it also has' if related else 'It has'
        order = ('It is ordered, so `<` and sorting take it.' if entry.get('ordered') else
                 'It isn\'t ordered, so `<` and sorting don\'t take it.')
        out += [f'{lead} the contract\'s own operations: {joined(baseline)}. {order} '
                f'[Every standard type meets one contract](../topics/{CONTRACT_TOPIC}.md) says how they work.', '']
        return out

    def type_markdown(self, entry, linked):
        golden = self.golden(entry)
        group = entry.get('group') or 'Standard types'
        linked.add(TYPES_INDEX)
        facts = [f'[{group}]({TYPES_INDEX}.md#{github_slug(group, {})})']
        facts.append('ordered' if entry.get('ordered') else 'not ordered')
        if entry.get('personal'):
            facts.append(f'personal data ({entry["personal"]})')
        facts.append(f'since Jadpo {entry.get("release")}')
        out = [f'# `{entry["name"]}`', '', self.summary(entry), '', '; '.join(facts) + '.', '',
               '## Make one', '', self.make_one(entry), '']
        out += self.members(entry, linked)
        out += self.forms(entry, golden)
        out += self.shown(entry)
        out += self.operations_section(entry, linked)
        return '\n'.join(out).rstrip('\n') + '\n'

    def index_markdown(self, linked):
        linked.add(STANDARD_TYPES_TOPIC)
        out = ['# Standard types', '',
               f'Every standard type Jadpo has, {len(self.types)} in all, by group. Each page says how to make one '
               f'and how it is written, sent as JSON, stored and shown to people, with its examples. '
               f'[The standard types](../topics/{STANDARD_TYPES_TOPIC}.md) explains the list.', '']
        for group in self.groups:
            out += [f'## {group}', '', '| Type | What it is |', '|---|---|']
            for entry in self.in_order():
                if entry.get('group') == group:
                    linked.add(type_id(entry['name']))
                    out.append(f'| [`{entry["name"]}`]({type_id(entry["name"])}.md) | '
                               f'{first_sentence(self.summary(entry)).replace("|", chr(92) + "|")} |')
            out.append('')
        return '\n'.join(out).rstrip('\n') + '\n'

    def build(self):
        """The index, then each type's page, as (kind, id, path, title,
        area, markdown, the IDs it links)."""
        if not self.types:
            return []
        linked = set()
        built = [('type', TYPES_INDEX, f'types/{TYPES_INDEX}.md', 'Standard types', 19,
                  self.index_markdown(linked), linked)]
        for entry in self.in_order():
            linked = set()
            text = self.type_markdown(entry, linked)
            built.append(('type', type_id(entry['name']), f'types/{type_id(entry["name"])}.md', entry['name'],
                          self.area(entry), text, linked))
        return built


class Bundle:
    """The built docs (docs/documentation.md, "The bundle"): the primer,
    a page per topic, per standard type, per rule and per command, and
    index.json. Pages link
    each other by ID; a link to anything else names the file on GitHub."""

    def __init__(self, pages, errors, help_data, root=ROOT, primer=None, learn=(), guides=()):
        self.pages, self.errors, self.root = pages, errors, root
        self.learn = list(learn)
        self.guides = list(guides)
        # Each Learn page's and guide's file -> its topic ID.
        self.learn_of = {page.path.resolve(): page.topics()[0]['id'] for page in self.learn
                         if page.topics() and page.topics()[0]['id']}
        self.guide_of = {page.path.resolve(): page.topics()[0]['id'] for page in self.guides
                         if page.topics() and page.topics()[0]['id']}
        self.catalogue = {r['id']: r for r in help_data.get('rules', [])}
        self.commands = help_data.get('commands', [])
        self.global_flags = help_data.get('flags', [])
        self.version = help_data.get('version')
        self.primer = primer
        # The standard type reference, from the compiler's contract data.
        self.types = TypeReference(help_data, root)
        self.problems = []
        # A renamed topic's ID -> the IDs it had before, which still reach it.
        self.aliases = {t['id']: t['aliases'] for page in pages for t in page.topics() if t['aliases']}
        # Where each anchor of an area page went: (page, anchor) -> topic ID.
        self.topic_of, self.overview = {}, {}
        for page in pages:
            topics = page.topics()
            if not topics:
                continue
            path = page.path.resolve()
            self.overview[path] = topics[0]['id']
            spans = [(t['start'], t['end'], t['id']) for t in topics]
            for line, _, _, anchor in headings_of(page.body):
                owner = next((tid for start, end, tid in spans if start <= line < end), topics[0]['id'])
                self.topic_of[(path, anchor)] = owner

    def link(self, source, target, here, linked):
        """`target`, written in `source`, as a bundle page in folder `here`
        ('', 'learn', 'guides', 'topics', 'errors' or 'commands') links it."""
        if re.match(r'^[a-z][a-z0-9+.-]*:', target):
            return target
        name, _, fragment = target.partition('#')
        resolved = (source.parent / name).resolve() if name else source.resolve()
        tail = f'#{fragment}' if fragment else ''

        def to(folder, page_id, keep=True):
            linked.add(page_id)
            path = f'{folder}/{page_id}.md' if folder else f'{page_id}.md'
            return os.path.relpath(path, here or '.') + (tail if keep else '')

        if resolved in self.learn_of:
            return to('learn', self.learn_of[resolved])
        if resolved in self.guide_of:
            return to('guides', self.guide_of[resolved])
        if resolved in self.overview:
            topic = self.topic_of.get((resolved, fragment)) if fragment else self.overview[resolved]
            if topic is None:
                self.problems.append(f'{source.relative_to(self.root)}: no topic holds {target}')
                topic = self.overview[resolved]
            own = fragment and github_slug(self.title_of(topic), {}) == fragment
            return to('topics', topic, keep=bool(fragment) and not own)
        if resolved.parent == (self.root / 'docs/errors').resolve() and resolved.suffix == '.md':
            return to('errors', resolved.stem)
        if self.primer is not None and resolved == self.primer.resolve():
            return to('', 'primer')
        try:
            relative = resolved.relative_to(self.root.resolve())
        except ValueError:
            return target
        return REPO_URL + str(relative) + tail

    def title_of(self, topic_id):
        for page in self.pages:
            for topic in page.topics():
                if topic['id'] == topic_id:
                    return topic['title']
        return ''

    def topic_markdown(self, page, topic, linked, here='topics'):
        """A topic as its own page: its heading the page's `#`, its hidden
        blocks and topic ID left out, its links by ID."""
        lines = page.body.split('\n')[topic['start']:topic['end']]
        hidden = {b['line'] - topic['start'] for b in page.blocks if b['attrs'].get('hidden')
                  and topic['start'] <= b['line'] < topic['end']}
        out, skip_to = [], -1
        shift = topic['level'] - 1
        for i, line, fenced in body_lines(lines):
            if i <= skip_to:
                continue
            if i in hidden:
                skip_to = next(j for j in range(i + 1, len(lines)) if lines[j].startswith('```'))
                if out and not out[-1].strip():
                    out.pop()
                continue
            if not fenced and TOPIC.match(line):
                continue
            heading = None if fenced else HEADING.match(line)
            if heading and shift:
                line = '#' * max(1, len(heading.group(1)) - shift) + ' ' + heading.group(2)
            out.append(line)
        text = '\n'.join(out).strip('\n') + '\n'
        text = re.sub(r'\n{3,}', '\n\n', text)
        return map_links(text, lambda t: self.link(page.path, t, here, linked))

    def error_markdown(self, page, linked):
        entry = self.catalogue[page.rule]
        area = int(entry.get('area', 0))
        area_page = next((p for p in self.pages if p.area() == area), None)
        lines = page.body.split('\n')
        hidden = {b['line'] for b in page.blocks if b['attrs'].get('hidden')}
        out, skip_to = [], -1
        for i, line, _ in body_lines(lines[1:]):
            if i + 1 <= skip_to:
                continue
            if i + 1 in hidden:
                skip_to = next(j for j in range(i + 2, len(lines)) if lines[j].startswith('```'))
                continue
            out.append(line)
        body = '\n'.join(out).strip('\n')
        # The sentence saying what happened is the rule's summary, which
        # the head already gives.
        first, _, rest = body.partition('\n\n')
        if ' '.join(first.split()) == f'{entry["summary"]}.':
            body = rest.strip('\n')
        body = map_links(body, lambda t: self.link(page.path, t, 'errors', linked))
        facts = [f'Area {area}']
        if area_page is not None and area_page.topics():
            overview = area_page.topics()[0]
            linked.add(overview['id'])
            facts[0] = f'Area {area}, [{overview["title"]}](../topics/{overview["id"]}.md)'
        facts.append('declinable' if entry.get('declinable') else 'not declinable')
        if entry.get('decides'):
            facts.append(f'the next step is usually the {entry["decides"]}\'s')
        head = f'# `{page.rule}`\n\n{entry["summary"]}.\n\n{"; ".join(facts)}.\n'
        return head + (f'\n{body}\n' if body else '')

    def every_command_markdown(self, command):
        """The section every command page ends its flags with: the flags
        `jadpo help --json` lists for every command, and what `--json` gives,
        in the same words on each page."""
        out = ['## On every command', '']
        for flag in self.global_flags:
            value = f' <{flag["value"]}>' if flag.get('value') else ''
            out.append(f'- `{flag["name"]}{value}`: {flag.get("description", "")}')
        out += ['', JSON_OUTPUT.format(name=command['name']), '']
        return out

    def command_markdown(self, command, linked):
        name = command['name']
        out = [f'# `jadpo {name}`', '', f'{command["summary"]}.', '']
        if command.get('description'):
            out += [command['description'], '']
        if command.get('synonyms'):
            out += ['Also: ' + ', '.join(f'`jadpo {s}`' for s in command['synonyms']) + '.', '']
        out += ['```text', command.get('usage', f'jadpo {name}'), '```', '']
        if command.get('arguments'):
            out += ['## Arguments', '']
            for arg in command['arguments']:
                need = '' if arg.get('required') else ' (optional)'
                out.append(f'- `{arg["name"]}`{need}: {arg.get("description", "")}')
            out.append('')
        if command.get('flags'):
            out += ['## Flags', '']
            for flag in command['flags']:
                value = f' <{flag["value"]}>' if flag.get('value') else ''
                out.append(f'- `{flag["name"]}{value}`: {flag.get("description", "")}')
            out.append('')
        out += self.every_command_markdown(command)
        if command.get('examples'):
            out += ['## Examples', '', '```text']
            for example in command['examples']:
                out += [f'# {example["description"]}', example['command']]
            out += ['```', '']
        rules = [r for r in command.get('rules', []) if r in self.catalogue]
        if rules:
            out += ['## Errors', '']
            for rule in rules:
                linked.add(rule)
                out.append(f'- [`{rule}`](../errors/{rule}.md): {self.catalogue[rule]["summary"]}')
            out.append('')
        return '\n'.join(out).rstrip('\n') + '\n'

    def build(self):
        """Every page, as (kind, id, path in the bundle, title, area,
        markdown, the IDs it links)."""
        built = []
        if self.primer is not None and self.primer.exists():
            linked = set()
            text = map_links(without_hidden(self.primer.read_text()),
                             lambda t: self.link(self.primer, t, '', linked))
            built.append(('primer', 'primer', 'primer.md', 'Primer', None, text, linked))
        for page in self.learn:
            for topic in page.topics():
                linked = set()
                text = self.topic_markdown(page, topic, linked, here='learn')
                built.append(('learn', topic['id'], f'learn/{topic["id"]}.md', plain(topic['title'].replace('`', '')),
                              None, text, linked))
        for page in self.guides:
            for topic in page.topics():
                linked = set()
                text = self.topic_markdown(page, topic, linked, here='guides')
                built.append(('guide', topic['id'], f'guides/{topic["id"]}.md', plain(topic['title'].replace('`', '')),
                              None, text, linked))
        for page in self.pages:
            topics = page.topics()
            for topic in topics:
                linked = set()
                text = self.topic_markdown(page, topic, linked)
                title = plain(topic['title'].replace('`', ''))
                if title == 'What is built' and topics:
                    # Every area has one; the title says whose.
                    title = f'What is built: {plain(topics[0]["title"].replace("`", ""))}'
                built.append(('topic', topic['id'], f'topics/{topic["id"]}.md', title, page.area(), text, linked))
        built += self.types.build()
        self.problems += self.types.problems
        for page in sorted(self.errors, key=lambda p: p.rule):
            if page.rule in self.catalogue:
                linked = set()
                text = self.error_markdown(page, linked)
                built.append(('error', page.rule, f'errors/{page.rule}.md', page.rule,
                              int(self.catalogue[page.rule].get('area', 0)) or None, text, linked))
        for command in self.commands:
            linked = set()
            command_id = 'jadpo-' + re.sub(r'[^a-z0-9]+', '-', command['name'])
            built.append(('command', command_id, f'commands/{command_id}.md', f'jadpo {command["name"]}',
                          COMMAND_AREA, self.command_markdown(command, linked), linked))
        # Every page's fences keep only their language (docs/documentation.md,
        # "The bundle").
        return [(kind, page_id, path, title, area, bare_fences(text), linked)
                for kind, page_id, path, title, area, text, linked in built]

    def index(self, built):
        areas = {area: title for kind, page_id, _, title, area, _, _ in built
                 if kind == 'topic' and page_id in self.overview.values()}
        entries = []
        for kind, page_id, _, title, area, text, linked in built:
            aliases = self.aliases.get(page_id) if kind == 'topic' else None
            fallback = title
            if kind == 'topic' and title.startswith('What is built: '):
                # Each area's table of what is built leads with the table.
                fallback = f'Which parts of {areas.get(area, "the area")} are built, and what the rest waits on'
            summary = (self.catalogue[page_id]['summary'] if kind == 'error'
                       else summary_of(text, fallback))
            entries.append({
                'id': page_id, 'kind': kind, 'title': title, 'summary': summary,
                # About four characters a token, as tokenizers average on English.
                'tokens': (len(text) + 3) // 4,
                'hash': 'sha256:' + hashlib.sha256(text.encode()).hexdigest(),
                'area': area, 'links': sorted(linked - {page_id}),
                # Only a renamed topic has them: its old IDs, which still reach it.
                **({'aliases': list(aliases)} if aliases else {}),
            })
        return {'version': self.version, 'pages': entries}

    def write(self, out):
        """Writes the bundle into `out`, replacing what was there."""
        import shutil
        built = self.build()
        if out.exists():
            shutil.rmtree(out)
        for _, _, path, _, _, text, _ in built:
            target = out / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
        (out / 'index.json').write_text(json.dumps(self.index(built), indent=2, ensure_ascii=False) + '\n')
        # The agent skill goes in byte for byte, outside the index.
        skill = self.root / SKILL
        if skill.exists():
            (out / SKILL_IN_BUNDLE).parent.mkdir(parents=True, exist_ok=True)
            (out / SKILL_IN_BUNDLE).write_bytes(skill.read_bytes())
        return built


def check_skill(root=ROOT):
    """Problems with the agent skill (docs/documentation.md, "The agent
    skill"): it exists, and its front matter has a `name` that is its
    folder in the bundle and a one-line `description`."""
    path = root / SKILL
    if not path.exists():
        return [f'{SKILL} is missing: it is the skill the site publishes for coding agents']
    text = path.read_text()
    if not text.startswith('---\n') or '\n---\n' not in text[4:]:
        return [f'{SKILL} has no front matter with a name and a description']
    meta, _ = parse_front_matter(text)
    problems = []
    if meta.get('name') != SKILL_IN_BUNDLE.parent.name:
        problems.append(f'{SKILL} front matter name is "{meta.get("name")}", not "{SKILL_IN_BUNDLE.parent.name}"')
    if not meta.get('description') or len(meta['description']) > 1024:
        problems.append(f'{SKILL} front matter needs a description of 1 to 1,024 characters')
    return problems


def type_table_problems(pages, types):
    """Where the table of standard types on the Standard library page
    disagrees with the compiler's groups (`jadpo help --json`'s `types`): a
    group the data doesn't have, or in another order, or a type listed in a
    group the data puts it outside."""
    page = next((p for p in pages if p.area() == 19), None)
    if page is None or not types:
        return []
    group_of = {entry['name']: entry.get('group') for entry in types}
    groups = list(dict.fromkeys(entry.get('group') for entry in types))
    lines = page.body.split('\n')
    start = next((i for i, line in enumerate(lines) if line.startswith('| Group | Types |')), None)
    if start is None:
        return [f'{page.path.name}: no table of the standard types by group (`| Group | Types |`)']
    problems, listed = [], []
    for line in lines[start + 2:]:
        if not line.startswith('|'):
            break
        cells = line.split('|')
        group = re.sub(r'\s*\(.*\)$', '', cells[1].strip())
        listed.append(group)
        for name in re.findall(r'\*\*`([^`<]+)(?:<[^`]*>)?`\*\*', cells[2]):
            if name in group_of and group_of[name] != group:
                problems.append(f'{page.path.name}: `{name}` is listed under "{group}", but the compiler groups it '
                                f'under "{group_of[name]}" (crates/jadpo-std/src/standard/groups.rs)')
    if listed != groups:
        problems.append(f'{page.path.name}: the standard types\' groups are {listed}, but the compiler\'s are '
                        f'{groups} (crates/jadpo-std/src/standard/groups.rs)')
    return problems


def check_structure(pages, errors, catalogue, commands=(), root=ROOT, learn=(), guides=(), help_data=None):
    """Problems with the docs' shape, none of which needs an example
    compiled: topic IDs, a page per rule, topic sizes and links."""
    problems = []
    seen = {}
    types = (help_data or {}).get('types', [])
    for entry in types:
        seen.setdefault(type_id(entry['name']), []).append(f'the standard type `{entry["name"]}`')
        if not TOPIC_ID.match(type_id(entry['name'])):
            problems.append(f'standard type `{entry["name"]}`: its page ID {type_id(entry["name"])} isn\'t kebab case')
    if types:
        seen.setdefault(TYPES_INDEX, []).append('the standard types\' index')
    problems += type_table_problems(pages, types)
    for page in list(pages) + list(learn) + list(guides):
        problems += [f'{page.path.name} {problem}' for problem in page.check_topics()]
        for topic in page.topics():
            if topic['id']:
                seen.setdefault(topic['id'], []).append(f'{page.path.name} "{topic["title"]}"')
            for alias in topic['aliases']:
                seen.setdefault(alias, []).append(f'{page.path.name} "{topic["title"]}" (its old ID)')
    for command in commands:
        seen.setdefault('jadpo-' + re.sub(r'[^a-z0-9]+', '-', command['name']), []).append(
            f'the command `jadpo {command["name"]}`')
    seen.setdefault('primer', []).append('the primer')
    for topic_id, places in sorted(seen.items()):
        if len(places) > 1:
            problems.append(f'topic ID "{topic_id}" is used by {", ".join(places)}')
    pages_by_rule = {page.rule for page in errors}
    for rule, entry in sorted(catalogue.items()):
        if rule not in pages_by_rule:
            problems.append(f'{rule} (area {entry.get("area")}) has no page: add docs/errors/{rule}.md')
    bundle = Bundle(pages, errors, {'rules': list(catalogue.values()), 'commands': list(commands), 'types': types,
                                    'operations': (help_data or {}).get('operations', [])}, root,
                    primer=root / 'docs/primer.md', learn=learn, guides=guides)
    for kind, page_id, _, _, _, text, _ in bundle.build():
        if kind in ('topic', 'learn', 'guide') and words(text) > TOPIC_WORDS:
            problems.append(f'topic "{page_id}" is {words(text)} words, over {TOPIC_WORDS}: split it')
    problems += bundle.problems
    files = markdown_files(root)
    problems += check_links(files, root)
    problems += check_help_lines(files, catalogue, root)
    return problems


# --- rendering -------------------------------------------------------------

def inline(text):
    codes = []

    def keep(match):
        codes.append(f'<code>{html.escape(match.group(1))}</code>')
        return f'\x00{len(codes) - 1}\x00'

    text = re.sub(r'`([^`]+)`', keep, text)
    text = html.escape(text, quote=False)
    text = re.sub(r'\*\*(.+?)\*\*', r'<strong>\1</strong>', text)
    text = re.sub(r'(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])', r'<em>\1</em>', text)
    text = re.sub(r'\[([^\]]+)\]\(([^)\s]+)\)', r'<a href="\2">\1</a>', text)
    return re.sub(r'\x00(\d+)\x00', lambda m: codes[int(m.group(1))], text)


def slug(text):
    return re.sub(r'[^a-z0-9]+', '-', re.sub(r'<[^>]+>', '', text).lower()).strip('-')


# A list item: its indent, its marker, and the text on the line.
LIST_ITEM = re.compile(r'^( *)(- |\d+\. )(.*)$')


def parse_list(lines, i, end):
    """The list starting at lines[i], up to a blank line, a line at or left
    of its items' indent that isn't one of its items, or `end`. A line
    indented further continues the last item; an item indented further
    starts a list inside it. Returns (ordered, items, the next line); each
    item has its `parts` (text, or a nested (ordered, items)) and `flat`,
    its text with any nested items' lines as written."""
    first = LIST_ITEM.match(lines[i])
    indent, ordered = len(first.group(1)), first.group(2)[0].isdigit()
    items = []
    while i < end and lines[i].strip():
        line = lines[i]
        depth = len(line) - len(line.lstrip(' '))
        item = LIST_ITEM.match(line)
        if item and depth == indent:
            items.append({'parts': [item.group(3)], 'flat': [item.group(3)]})
            i += 1
        elif depth <= indent:
            break
        elif item:
            start = i
            nested = parse_list(lines, i, end)
            i = nested[2]
            items[-1]['parts'].append(nested[:2])
            items[-1]['flat'].extend(lines[k].strip() for k in range(start, i))
        else:
            items[-1]['parts'].append(line.strip())
            items[-1]['flat'].append(line.strip())
            i += 1
    return ordered, items, i


def render_list(ordered, items):
    tag = 'ol' if ordered else 'ul'
    out = []
    for item in items:
        body, text = '', []
        for part in item['parts'] + [None]:
            if isinstance(part, str):
                text.append(part)
                continue
            if text:
                body += (' ' if body else '') + inline(' '.join(text))
                text = []
            if part is not None:
                body += render_list(*part)
        out.append(f'<li>{body}</li>')
    return f'<{tag}>' + ''.join(out) + f'</{tag}>'


class Renderer:
    def __init__(self, page):
        self.page = page
        self.toc = []
        self.in_errors = False
        self.in_review = False
        self.in_questions = False
        self.review_mode = False

    def code_block(self, block):
        attrs, result = block['attrs'], self.page.results.get(block['attrs']['project'])
        if attrs.get('hidden'):
            return ''
        if attrs.get('fragment'):
            status = '<span class="muted">fragment, not compiled</span>'
        elif result is None:
            status = ''
        elif not result['ok']:
            status = '<span class="broken">✗ example does not behave as stated</span>'
        elif result['expected'] == 'pass':
            status = '<span class="check">✓ checks · formatted</span>'
        else:
            status = f'<span class="check">✓ fails with {html.escape(result["expected"])}</span>'
        return (f'<div class="code"><div class="label"><span>{html.escape(attrs["file"])}</span>{status}</div>'
                f'<pre><code class="jadpo">{html.escape(block["source"].rstrip())}</code></pre></div>')

    def heading(self, level, text):
        rendered = inline(text)
        anchor = slug(text)
        if level == 2:
            self.in_errors = text.strip().lower() == 'errors'
            if text.strip().lower() == 'review':
                self.in_review = True
                self.toc.append((anchor, 'Review', 'review-link'))
                return (f'<section class="review" id="review"><h2 id="{anchor}">Review</h2>'
                        '<p class="review-note">For the language review only. Generated public documentation leaves this section out.</p>')
            self.toc.append((anchor, re.sub(r'<[^>]+>', '', rendered), ''))
        if level == 3:
            if (self.in_review or self.review_mode) and text.strip().lower() == 'open questions':
                self.in_questions = True
                return f'<h3 id="{anchor}">{rendered}</h3><ol class="questions">'
        if level == 3 and self.in_errors and re.fullmatch(r'`[a-z_.]+`', text.strip()):
            rule = text.strip('` ')
            entry = self.page.catalogue.get(rule)
            summary = html.escape(entry['summary']) if entry else 'Not in the compiler catalogue'
            return (f'<h3 class="rule-heading" id="{anchor}"><code>{html.escape(rule)}</code>'
                    f'<span class="summary">{summary}</span>'
                    f'<button type="button" class="comment-btn" data-comment-target>Comment</button></h3>')
        return f'<h{level} id="{anchor}">{rendered}</h{level}>'

    def close_questions(self):
        if self.in_questions:
            self.in_questions = False
            return '</ol>'
        return ''

    def close_sections(self):
        out = self.close_questions()
        self.in_errors = False
        if self.in_review:
            out += '</section>'
            self.in_review = False
        return out

    def render(self, start=0, end=None):
        lines = self.page.body.splitlines()
        end = len(lines) if end is None else end
        blocks = {b['line']: b for b in self.page.blocks}
        out, i, para = [], start, []

        def flush():
            if para:
                out.append(f'<p>{inline(" ".join(para))}</p>')
                para.clear()

        while i < end:
            line = lines[i]
            if line.startswith('```'):
                flush()
                j = i + 1
                while not lines[j].startswith('```'):
                    j += 1
                if i in blocks:
                    out.append(self.code_block(blocks[i]))
                else:
                    label = (line[3:].split() or ['text'])[0]
                    if line.strip() == OUTPUT_FENCE:
                        label = 'jadpo check'
                    out.append(f'<div class="code plain"><div class="label"><span>{html.escape(label)}</span></div>'
                               f'<pre><code>{html.escape(chr(10).join(lines[i + 1:j]))}</code></pre></div>')
                i = j + 1
                continue
            # A topic ID, and any other comment on a line of its own, is for
            # tools, not readers.
            if line.startswith('<!--') and line.rstrip().endswith('-->'):
                flush()
                i += 1
                continue
            heading = re.match(r'^(#{1,4}) (.+)$', line)
            if heading:
                flush()
                level = len(heading.group(1))
                if level == 1:
                    out.append(f'<h1>{inline(heading.group(2))}</h1>')
                else:
                    closing = self.close_sections() if level == 2 else (self.close_questions() if level == 3 else '')
                    out.append(closing + self.heading(level, heading.group(2)))
                i += 1
                continue
            if line.startswith('|'):
                flush()
                rows = []
                while i < len(lines) and lines[i].startswith('|'):
                    rows.append([c.strip() for c in lines[i].strip('|').split('|')])
                    i += 1
                head, body = rows[0], [r for r in rows[1:] if not set(''.join(r)) <= set('-: ')]
                cells = ''.join(f'<th>{inline(c)}</th>' for c in head)
                trs = ''.join('<tr>' + ''.join(f'<td>{inline(c)}</td>' for c in r) + '</tr>' for r in body)
                out.append(f'<div class="table-wrap"><table><thead><tr>{cells}</tr></thead><tbody>{trs}</tbody></table></div>')
                continue
            if line.startswith('>'):
                flush()
                quote = []
                while i < len(lines) and lines[i].startswith('>'):
                    quote.append(lines[i][1:].strip())
                    i += 1
                out.append(f'<blockquote class="note"><p>{inline(" ".join(quote))}</p></blockquote>')
                continue
            if LIST_ITEM.match(line):
                flush()
                ordered, items, i = parse_list(lines, i, end)
                if self.in_questions and ordered:
                    for k, item in enumerate(items, 1):
                        out.append(self.question(k, ' '.join(item['flat'])))
                else:
                    out.append(render_list(ordered, items))
                continue
            if not line.strip():
                flush()
            else:
                para.append(line.strip())
            i += 1
        flush()
        out.append(self.close_sections())
        return '\n'.join(out)

    def question(self, number, text):
        title, _, rest = text.partition('** ')
        title = title.lstrip('*').rstrip('*')
        body, _, recommendation = rest.partition('*Recommendation:*')
        rec = f'<p class="rec"><strong>Recommendation:</strong> {inline(recommendation.strip())}</p>' if recommendation else ''
        return (f'<li class="q" id="q{number}"><h4><span class="qn">Q{number}</span>{inline(title)}</h4>'
                f'<p>{inline(body.strip())}</p>{rec}'
                '<div class="decide"><button type="button" class="comment-btn" data-comment-target>Comment on this question</button></div></li>')


def load_published():
    links = Path(OUT_DEFAULT) / 'published.json'
    return json.loads(links.read_text()) if links.exists() else {}


def build_nav(page, pages, published):
    titles = {int(p.meta.get('area', 0)): p for p in pages}
    current = ' class="current" aria-current="page"' if page is None else ''
    nav = [f'<li><a{current} href="{published.get("decisions", "decisions.html")}"><span class="n">★</span>Decision log</a></li>']
    for number, name in AREAS:
        if number in titles:
            current = ' class="current" aria-current="page"' if titles[number] is page else ''
            href = published.get(titles[number].path.stem, f'{titles[number].path.stem}.html')
            nav.append(f'<li><a{current} href="{href}"><span class="n">{number:02d}</span>{name}</a></li>')
        else:
            nav.append(f'<li><span><span class="n">{number:02d}</span>{name}</span></li>')
    return nav


def render_page(page, pages, notes=None, source=None):
    """An area page, its design notes collapsed under it; or an error page."""
    template = TEMPLATE.read_text()
    renderer = Renderer(page)
    content = renderer.render()
    if notes is not None:
        review = Renderer(notes)
        review.review_mode = True
        content += ('<details class="collapsed review" id="review"><summary>Design notes: the decisions, '
                    'how the area fits the rest of the language and the questions as asked</summary>'
                    + review.render() + review.close_sections() + '</details>')
        renderer.toc.append(('review', 'Design notes (collapsed)', 'review-link'))
    published = load_published()
    nav = build_nav(page, pages, published)
    toc = ''.join(f'<li><a class="{cls}" href="#{a}">{html.escape(t)}</a></li>' for a, t, cls in renderer.toc)
    return (template.replace('{{title}}', html.escape(page.meta.get('title', page.path.stem)))
            .replace('{{area}}', f'{int(page.meta.get("area", 0)):02d}')
            .replace('{{checked}}', html.escape(page.meta.get('checked-with', '')))
            .replace('{{source}}', source or f'docs/language/{page.path.name}')
            .replace('{{nav}}', ''.join(nav))
            .replace('{{toc}}', toc)
            .replace('{{content}}', content))



def section(body, title):
    """The lines under the `##` heading `title` (a design notes page's
    sections), up to the next `##` or `###`."""
    lines = body.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == f'## {title}')
    except StopIteration:
        return []
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith('## ') or lines[i].startswith('### ')), len(lines))
    return lines[start + 1:end]


def table_rows(lines):
    rows = [[c.strip() for c in line.strip().strip('|').split('|')] for line in lines if line.startswith('|')]
    return [r for r in rows[1:] if not set(''.join(r)) <= set('-: ')]


def question_titles(lines):
    items = []
    for line in lines:
        if re.match(r'^\d+\. ', line):
            items.append(line.strip())
        elif items and line.strip():
            items[-1] += ' ' + line.strip()
    out = []
    for item in items:
        m = re.match(r'^(\d+)\. \*\*(.+?)\*\*(.*)$', item)
        if m:
            out.append((m.group(1), m.group(2), '*Decided' in m.group(3)))
    return out


def render_log(pages, published, notes=None):
    """One page listing every decision, open question and carried item,
    built from the areas' design notes."""
    notes = notes or {}
    out = ['<h1>Decision log</h1>',
           '<p>Everything decided in the language review so far, every question still open, and items carried '
           'to later areas. Built from the area pages each time they are generated, so it cannot drift from them.</p>']
    toc = []
    totals = [0, 0]
    for page in pages:
        area = int(page.meta.get('area', 0))
        name = page.meta.get('title', page.path.stem)
        anchor = f'area-{area}'
        href = published.get(page.path.stem, f'{page.path.stem}.html')
        body = notes[page.path.name].body if page.path.name in notes else ''
        decided = [r for r in table_rows(section(body, 'Decisions')) if len(r) > 1 and r[1]]
        questions = question_titles(section(body, 'Open questions'))
        open_q = [q for q in questions if not q[2]]
        carried_title = 'Carried to later areas' if section(body, 'Carried to later areas') else 'Related areas'
        carried = table_rows(section(body, carried_title))
        totals[0] += len(decided)
        totals[1] += len(open_q)
        toc.append((anchor, f'{area:02d} {name}', ''))
        out.append(f'<h2 id="{anchor}">{area:02d} · {html.escape(name)}</h2>')
        out.append(f'<p class="muted">Status: {html.escape(page.meta.get("status", ""))} · {len(decided)} decided · '
                   f'{len(open_q)} open · <a href="{href}">open the page</a></p>')
        if decided:
            rows = ''.join(f'<tr><td>{inline(r[0])}</td><td>{inline(r[1])}</td></tr>' for r in decided)
            out.append(f'<h3>Decided</h3><div class="table-wrap"><table><thead><tr><th>Topic</th><th>Decision</th></tr></thead><tbody>{rows}</tbody></table></div>')
        if open_q:
            items = ''.join(f'<li><strong>Q{n}</strong> {inline(t)}</li>' for n, t, _ in open_q)
            out.append(f'<h3>Open</h3><ul>{items}</ul>')
        if carried:
            rows = ''.join('<tr>' + ''.join(f'<td>{inline(c)}</td>' for c in r) + '</tr>' for r in carried)
            out.append(f'<h3>{carried_title}</h3><div class="table-wrap"><table><thead><tr><th>Item</th><th>Area</th><th>Status</th></tr></thead><tbody>{rows}</tbody></table></div>')
    out.insert(2, f'<p><strong>{totals[0]} decided, {totals[1]} open</strong> across {len(pages)} area page(s).</p>')
    return '\n'.join(out), toc

def export_corpus(corpus):
    """Writes each project with tests, as its blocks give it, and its
    `jadpo test --json` document to build/spec-corpus/<page>/<project>/,
    replacing what was there (docs/compiler-runtime.md §2.7)."""
    import shutil
    # The pages' projects only: tools/docs/examples.py owns examples/.
    if CORPUS.exists():
        for folder in CORPUS.iterdir():
            if folder.name != 'examples':
                shutil.rmtree(folder) if folder.is_dir() else folder.unlink()
    for page, (name, files, document) in corpus:
        folder = CORPUS / page.path.stem / name
        for path, source in files:
            target = folder / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(source)
        (folder / 'test.json').write_text(document)
    print(f'wrote {len(corpus)} project(s) to {CORPUS.relative_to(ROOT)}')


AREAS = [
    (1, 'Project layout and modules'), (2, 'Values and types'), (3, 'Logic'), (4, 'Failures'),
    (5, 'Data'), (6, 'Reads and writes'), (7, 'HTTP'), (8, 'Identity'), (9, 'Policy'),
    (10, 'Configuration and time'), (11, 'External services'), (12, 'Jobs and events'),
    (13, 'Testing'), (14, 'Tooling'), (15, 'Personal data'), (16, 'Built-in services'), (17, 'Debugging and review'), (18, 'Records and manifest'),
    (19, 'Standard library'), (20, 'Geospatial'),
    (21, 'AI'),
]


def write_site(pages, out, notes=None, errors=()):
    """Every area page with its design notes, every error page under
    errors/, and the decision log, as HTML in `out`, each page under
    RENDER_LIMIT."""
    notes = notes or {}
    out.mkdir(parents=True, exist_ok=True)
    for page in pages:
        with time_limit(page.path.name):
            (out / f'{page.path.stem}.html').write_text(render_page(page, pages, notes.get(page.path.name)))
    if errors:
        (out / 'errors').mkdir(exist_ok=True)
    for page in errors:
        with time_limit(page.path.name):
            (out / 'errors' / f'{page.rule}.html').write_text(
                render_page(page, pages, source=f'docs/errors/{page.path.name}'))
    published = load_published()
    with time_limit('the decision log'):
        content, toc = render_log(pages, published, notes)
    toc_html = ''.join(f'<li><a href="#{a}">{html.escape(t)}</a></li>' for a, t, _ in toc)
    (out / 'decisions.html').write_text(
        TEMPLATE.read_text()
        .replace('{{area}}', 'log').replace('{{checked}}', 'all pages')
        .replace('{{source}}', 'docs/language/notes/*.md (Decisions, Open questions, Carried tables)')
        .replace('{{nav}}', ''.join(build_nav(None, pages, published)))
        .replace('{{toc}}', toc_html).replace('{{content}}', content))


@contextmanager
def time_limit(what):
    """Raises TimeoutError when the body runs past RENDER_LIMIT, so a
    renderer that stops moving fails instead of hanging."""
    def stop(*_):
        raise TimeoutError(f'rendering {what} took over {RENDER_LIMIT} s')
    previous = signal.signal(signal.SIGALRM, stop)
    signal.alarm(RENDER_LIMIT)
    try:
        yield
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, previous)


def load_pages(catalogue, root=ROOT, commands=()):
    """The area pages, their design notes by page name, and the error
    pages; `commands`, `jadpo help --json`'s, say which rules only a
    command raises."""
    docs = root / 'docs/language'
    pages = [Page(p, catalogue) for p in sorted(docs.glob('[0-9][0-9]-*.md'))]
    notes = {p.name: Notes(p, catalogue) for p in sorted((docs / 'notes').glob('[0-9][0-9]-*.md'))}
    errors = [ErrorPage(p, catalogue) for p in sorted((root / 'docs/errors').glob('*.md'))]
    scope = error_format_scope(catalogue, root, commands)
    for page in errors:
        page.format_required = page.rule in scope
    return pages, notes, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='verify examples and render the pages without writing them')
    parser.add_argument('--out', default=str(OUT_DEFAULT))
    parser.add_argument('--bless', action='store_true', help='rewrite the output blocks after check blocks')
    parser.add_argument('--export-corpus', action='store_true',
                        help=f'write the projects with tests and their jadpo test --json to {CORPUS.relative_to(ROOT)}')
    parser.add_argument('--bundle', metavar='DIR',
                        help='build the docs bundle into DIR; alone, it checks only the docs\' shape, not their examples')
    args = parser.parse_args()
    help_data = load_help()
    catalogue = {rule['id']: rule for rule in help_data['rules']}
    pages, notes, errors = load_pages(catalogue, commands=help_data.get('commands', []))
    learn = load_learn(catalogue)
    guides = load_guides(catalogue)
    failed = False
    compile_examples = args.check or args.bless or args.export_corpus or not args.bundle
    checked = pages + list(notes.values()) + errors + learn + guides if compile_examples else []
    for page in checked:
        page.check(bless=args.bless)
        blessed = page.bless() if args.bless else 0
        if blessed:
            print(f'{page.path.relative_to(ROOT)}: blessed {blessed} output block(s)')
    for page in pages:
        if not compile_examples:
            page.collect_blocks()
        examples = len([b for b in page.blocks if not b['attrs'].get('fragment')])
        print(f'{page.path.name}: {examples} examples, {len(page.topics())} topics')
        for problem in page.problems:
            failed = True
            print(f'  FAIL {problem}')
        numbered = numbered_references(page)
        if numbered and page.area() <= NUMBERED_FAILS_THROUGH:
            failed = True
            for line, text in numbered:
                print(f'  FAIL line {line}: "{text}": link the topic by its title, or move it to the design notes')
        elif numbered:
            print(f'  WARN {page.path.name}: {len(numbered)} area or question reference(s) by number, the first at line '
                  f'{numbered[0][0]} ("{numbered[0][1]}"); it fails once NUMBERED_FAILS_THROUGH reaches {page.area()}')
    for page in list(notes.values()) + errors:
        if not compile_examples:
            page.collect_blocks()
        for problem in page.problems:
            failed = True
            print(f'  FAIL {page.path.relative_to(ROOT)}: {problem}')
    for page in learn + guides:
        if not compile_examples:
            page.collect_blocks()
        projects = len({b['attrs']['project'] for b in page.blocks if b['attrs'].get('check')})
        print(f'{page.path.relative_to(ROOT)}: {projects} project(s) checked')
        for problem in page.problems + user_page_problems(page):
            failed = True
            print(f'  FAIL {page.path.relative_to(ROOT)}: {problem}')
    if compile_examples:
        covered = sum(1 for page in errors if page.rule in page.covered_rules())
        print(f'{len(errors)} error page(s) for {len(catalogue)} rules; {covered} with a checked example')
    for problem in check_structure(pages, errors, catalogue, help_data.get('commands', []), learn=learn,
                                   guides=guides, help_data=help_data):
        failed = True
        print(f'  FAIL {problem}')
    for problem in check_skill():
        failed = True
        print(f'  FAIL {problem}')
    primer = Primer(PRIMER, catalogue) if PRIMER.exists() else None
    if primer is None:
        failed = True
        print(f'  FAIL {PRIMER.relative_to(ROOT)} is missing: it is the page an agent reads first')
    else:
        if compile_examples:
            primer.check(bless=args.bless)
            if args.bless and primer.bless():
                print('docs/primer.md: blessed output block(s)')
        else:
            primer.collect_blocks()
            primer.check_primer()
        print(f'docs/primer.md: {len(primer.blocks)} examples, about {primer_tokens(primer.body):,} tokens')
        for problem in primer.problems + user_page_problems(primer):
            failed = True
            print(f'  FAIL docs/primer.md: {problem}')
    if compile_examples:
        # The README's error example and the home page's shop example
        # (docs/shop-example.md) are compiled like a page's examples.
        readmes = [Readme(ROOT / name, catalogue) for name in ('README.md', 'docs/shop-example.md')]
        for readme in readmes:
            name = readme.path.relative_to(ROOT)
            readme.check(bless=args.bless)
            if args.bless and readme.bless():
                print(f'{name}: blessed output block(s)')
            print(f'{name}: {len([b for b in readme.blocks if b["attrs"]["file"].endswith(".jadpo")])} checked file(s)')
            for problem in readme.problems:
                failed = True
                print(f'  FAIL {name}: {problem}')
        everything = pages + list(notes.values()) + errors + readmes + ([primer] if primer else [])
        corpus = [(page, entry) for page in everything for entry in page.corpus]
        print(f'{len(corpus)} spec project(s) with tests ran under jadpo test')
        skipped = [f'{page.path.name} {name}' for page in everything for name in page.skipped]
        if skipped:
            print(f'skipped: jadpo test for {len(skipped)} project(s) with migrations, which need PostgreSQL '
                  f'(JADPO_SKIP_POSTGRES is set): {", ".join(skipped)}')
        skipped = [f'{page.path.name} {name}' for page in everything for name in page.skipped_runs]
        if skipped:
            print(f'skipped: {len(skipped)} command example(s) that need PostgreSQL '
                  f'(JADPO_SKIP_POSTGRES is set): {", ".join(skipped)}')
        if args.export_corpus:
            export_corpus(corpus)
    if args.bundle:
        out = Path(args.bundle)
        bundle = Bundle(pages, errors, help_data, primer=PRIMER, learn=learn, guides=guides)
        built = bundle.write(out)
        kinds = {}
        for kind, *_ in built:
            kinds[kind] = kinds.get(kind, 0) + 1
        print(f'wrote the docs bundle to {out}: ' + ', '.join(f'{n} {k}(s)' for k, n in kinds.items()))
    # `--check` renders too, into a scratch folder, so a page the renderer
    # can't finish fails the gate rather than only `language_docs.py`.
    if args.check:
        with tempfile.TemporaryDirectory() as scratch:
            write_site(pages, Path(scratch), notes, errors)
        print(f'rendered {len(pages)} page(s) and {len(errors)} error page(s)')
    elif not args.bundle:
        write_site(pages, Path(args.out), notes, errors)
        print(f'wrote {len(pages)} page(s) to {args.out}')
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
