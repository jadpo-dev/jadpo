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
14), whose page needs one only once it has a `check` block at all. On areas
1-9 no `fragment` may check cleanly, since one that does is an example the
compiler can check.

The Learn path (docs/learn/) is checked the same way, each page's
project built on the pages before it (LearnPage), and it and the primer
follow the mechanical rules of docs/writing.md (user_page_problems).

Every section an app author reads, each `#` and `##` heading of an area page
but `## Errors`, declares its topic ID on the line after it, as
`<!-- topic: <id> -->`: lowercase kebab case, unique across the docs. A
renamed topic keeps its old IDs, `<!-- topic: <id> (was <old-id>) -->`,
and index.json lists them as its `aliases`. A
topic is at most TOPIC_WORDS words as the bundle gives it. An area page
names no area or question by number (numbered_references): on areas 1 to
NUMBERED_FAILS_THROUGH that fails, on the rest it warns. Every relative
link in the repository's Markdown and llms.txt resolves, to a file and, for
Markdown, a heading GitHub gives that anchor; every area-page anchor the
compiler prints (`help_at`) names a heading of that page.
"""

import argparse
import hashlib
import html
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
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


# A slow machine adds the run's duration to a summary line (` in 0.1 s`);
# output blocks are compared and written without it.
RUN_TIME = re.compile(r' in \d+(?:\.\d+)? s$', re.MULTILINE)


def without_run_time(text):
    return RUN_TIME.sub('', text)


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
        self.problems = []
        self.results = {}
        self.blessings = []
        # Projects with tests that ran: (name, files, jadpo test --json).
        self.corpus = []
        # Projects whose tests weren't run, since they need PostgreSQL.
        self.skipped = []

    def collect_blocks(self):
        lines = self.body.splitlines()
        i, n = 0, 0
        while i < len(lines):
            match = FENCE.match(lines[i])
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
                steps = 0
                for block in blocks:
                    target = Path(tmp) / block['attrs']['file']
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(block['source'])
                    purpose = block['attrs'].get('migrate')
                    if purpose:
                        migrated, said = jadpo(['migrate', 'new', purpose], cwd=tmp,
                                               env={'SOURCE_DATE_EPOCH': str(MIGRATION_EPOCH + 60 * steps)})
                        steps += 1
                        if migrated:
                            problems.append(f'{name}: jadpo migrate new {purpose} failed:\n{said}')
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
        it is checked; problems found doing it. Only a Learn page has any."""
        return []

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
        """Rules a `check` example here shows failing as stated."""
        return {r['expected'] for r in self.results.values() if r['expected'] != 'pass' and r['ok']}

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

    def check(self, bless=False):
        super().check(bless)
        self.check_rule()

    def check_rule(self):
        """The page names a rule of the catalogue, is headed by it, and
        shows it failing, but for a rule only a command raises."""
        entry = self.catalogue.get(self.rule)
        if entry is None:
            self.problems.append(f'{self.rule} is not a rule in the compiler\'s catalogue (jadpo help --json)')
            return
        if not self.body.startswith(f'# `{self.rule}`\n'):
            self.problems.append(f'the page starts with its heading, # `{self.rule}`')
        has_checks = any(b['attrs'].get('check') for b in self.blocks)
        # Only a command raises an area-14 rule, so its CLI tests show it;
        # once its page has a check block, one of them must show the rule.
        needs_example = entry.get('area') != COMMAND_AREA or has_checks
        if needs_example and self.rule not in self.covered_rules():
            self.problems.append(f'no example for {self.rule}: add a check block failing with it')
        if not has_checks and entry.get('area') == COMMAND_AREA and NO_EXAMPLE not in self.body \
                and '```' not in self.body:
            self.problems.append(f'say "{NO_EXAMPLE}" on a page with no example')


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


class LearnPage(Page):
    """docs/learn/: the Learn path, one app built a page at a time
    (docs/documentation.md). A page shows only the files it adds or
    changes; its project is the one the pages before it built, less the
    files its front matter `removes:`, with its own on top. A reader makes
    the migrations and the API record with commands, so a page's check
    makes them too: `jadpo migrate new initial` and, unless the page shows
    the record, `jadpo api update`. A record is never inherited, since an
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
        steps = [(['migrate', 'new', 'initial'], {'SOURCE_DATE_EPOCH': str(MIGRATION_EPOCH)})]
        if not any(b['attrs']['file'].endswith('.jadpo_record') for b in blocks):
            steps.append((['api', 'update'], None))
        for args, env in steps:
            code, said = jadpo(args, cwd=tmp, env=env)
            if code:
                problems.append(f'{name}: jadpo {" ".join(args)} failed:\n{said}')
        return problems

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


# What docs/writing.md says a user page never does, outside code.
NUMBERED = re.compile(r'\b(?:areas?|questions?) \d+', re.IGNORECASE)
BUILD_STATUS = re.compile(r"\b(?:not built yet|isn't built|aren't built|not yet built)\b|not_built_yet",
                          re.IGNORECASE)
SPEC_WORDS = re.compile(r'^Decided\.|\bcarried to\b')
NOT_BUILT = '## Not built yet'
REPOSITORY_URL = re.compile(r'^https://github\.com/jadpo-dev/jadpo/(?:blob|tree)/')


def user_page_problems(page, root=ROOT):
    """Problems with a user page (the primer, the Learn path) against the
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
                   or (resolved.parent == docs_dir / 'learn' and resolved.suffix == '.md'))
        if not allowed:
            problems.append(f'{where}: {target} links a file in this repository, which a reader can\'t open; '
                            'link a page of the docs')
    return problems


# An area page names no area or question by number in what an app author
# reads either (docs/writing.md, "How a reference topic reads"). This fails
# on areas 1 to this one, whose pages follow the guide, and warns on the
# rest; once every page does, it is the last area and the rule fails on all.
NUMBERED_FAILS_THROUGH = 7


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


HELP_ANCHOR = re.compile(r'"(\d\d-[a-z0-9-]+)\.html#([a-z0-9-]+)"')


def check_help_anchors(root=ROOT):
    """Every area-page section the compiler's source names for a `help`
    line, `"NN-page.html#anchor"`, is a heading of that page as the site
    renders it."""
    slugs, problems = {}, []
    for path in sorted(root.glob('crates/*/src/**/*.rs')):
        for number, line in enumerate(path.read_text().split('\n'), 1):
            for stem, anchor in HELP_ANCHOR.findall(line):
                page = root / 'docs/language' / f'{stem}.md'
                if page not in slugs:
                    slugs[page] = {slug(t) for _, _, t, _ in headings_of(page.read_text())} if page.exists() else set()
                if anchor not in slugs[page]:
                    problems.append(f'{path.relative_to(root)}:{number}: help names {stem}.html#{anchor}, '
                                    'which is no heading of that page')
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


class Bundle:
    """The built docs (docs/documentation.md, "The bundle"): the primer,
    a page per topic, per rule and per command, and index.json. Pages link
    each other by ID; a link to anything else names the file on GitHub."""

    def __init__(self, pages, errors, help_data, root=ROOT, primer=None, learn=()):
        self.pages, self.errors, self.root = pages, errors, root
        self.learn = list(learn)
        # Each Learn page's file -> its topic ID.
        self.learn_of = {page.path.resolve(): page.topics()[0]['id'] for page in self.learn
                         if page.topics() and page.topics()[0]['id']}
        self.catalogue = {r['id']: r for r in help_data.get('rules', [])}
        self.commands = help_data.get('commands', [])
        self.version = help_data.get('version')
        self.primer = primer
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
        ('', 'topics', 'errors' or 'commands') links it."""
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
        body = map_links('\n'.join(out).strip('\n'), lambda t: self.link(page.path, t, 'errors', linked))
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
        return built

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
        return built


def check_structure(pages, errors, catalogue, commands=(), root=ROOT, learn=()):
    """Problems with the docs' shape, none of which needs an example
    compiled: topic IDs, a page per rule, topic sizes and links."""
    problems = []
    seen = {}
    for page in list(pages) + list(learn):
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
    bundle = Bundle(pages, errors, {'rules': list(catalogue.values()), 'commands': list(commands)}, root,
                    primer=root / 'docs/primer.md', learn=learn)
    for kind, page_id, _, _, _, text, _ in bundle.build():
        if kind in ('topic', 'learn') and words(text) > TOPIC_WORDS:
            problems.append(f'topic "{page_id}" is {words(text)} words, over {TOPIC_WORDS}: split it')
    problems += bundle.problems
    problems += check_links(markdown_files(root), root)
    problems += check_help_anchors(root)
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
                    label = line[3:].strip() or 'text'
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


def load_pages(catalogue, root=ROOT):
    """The area pages, their design notes by page name, and the error
    pages."""
    docs = root / 'docs/language'
    pages = [Page(p, catalogue) for p in sorted(docs.glob('[0-9][0-9]-*.md'))]
    notes = {p.name: Notes(p, catalogue) for p in sorted((docs / 'notes').glob('[0-9][0-9]-*.md'))}
    errors = [ErrorPage(p, catalogue) for p in sorted((root / 'docs/errors').glob('*.md'))]
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
    pages, notes, errors = load_pages(catalogue)
    learn = load_learn(catalogue)
    failed = False
    compile_examples = args.check or args.bless or args.export_corpus or not args.bundle
    checked = pages + list(notes.values()) + errors + learn if compile_examples else []
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
    for page in learn:
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
    for problem in check_structure(pages, errors, catalogue, help_data.get('commands', []), learn=learn):
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
        if args.export_corpus:
            export_corpus(corpus)
    if args.bundle:
        out = Path(args.bundle)
        bundle = Bundle(pages, errors, help_data, primer=PRIMER, learn=learn)
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
