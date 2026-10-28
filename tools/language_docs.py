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

  python3 tools/language_docs.py --check     # verify, and render into a scratch folder
  python3 tools/language_docs.py             # verify and write build/language-site
  python3 tools/language_docs.py --bless     # rewrite the output blocks after `check` blocks
  python3 tools/language_docs.py --check --export-corpus  # also write build/spec-corpus/

Areas 1-9 own exactly the compiler's rules whose catalogue entry (`jadpo
help --json`) names their area; the other pages list the rules they own in
front matter, as globs over the same catalogue. Each owned rule needs a
`check` block failing with it, and every rule belongs to one page (area 14's
command errors are shown by the CLI's own tests). On areas 1-9 no `fragment`
may check cleanly, since one that does is an example the compiler can check.
"""

import argparse
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
from fnmatch import fnmatch
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / 'docs/language'
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
# Areas 1 to this one own the compiler's rules by the catalogue's `area`;
# later pages name theirs in front matter.
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


def load_catalogue():
    """The compiler's rules (`jadpo help --json`), by id; each entry's
    `area` says which page owns it."""
    code, out = jadpo(['help', '--json'])
    if code:
        sys.exit(f'jadpo help --json failed:\n{out}')
    return {rule['id']: rule for rule in json.loads(out)['data']['rules']}


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

    def claimed_rules(self):
        """The rules this page owns. Areas 1 to AREA_OWNED own exactly the
        compiler's rules whose catalogue entry names their area; the other
        pages list theirs in front matter."""
        if self.area_owned():
            return sorted(r for r, entry in self.catalogue.items() if entry.get('area') == self.area())
        patterns = self.meta.get('rules', [])
        return sorted(r for r in self.catalogue if any(fnmatch(r, p) for p in patterns))

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
        missing = [r for r in self.claimed_rules() if r not in self.covered_rules()]
        for rule in missing:
            self.problems.append(f'no example for {rule}')
        return missing


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
        if self.in_errors:
            out += self.missing_rules()
            self.in_errors = False
        if self.in_review:
            out += '</section>'
            self.in_review = False
        return out

    def missing_rules(self):
        missing = [r for r in self.page.claimed_rules() if r not in self.page.covered_rules()]
        if not missing:
            return f'<p class="coverage">All {len(self.page.claimed_rules())} errors this area owns have an example above.</p>'
        items = ''.join(f'<li><code>{html.escape(r)}</code> {html.escape(self.page.catalogue[r]["summary"])}</li>' for r in missing)
        return f'<div class="gap"><h3>Errors without an example yet</h3><ul>{items}</ul></div>'

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

    def render_decided(self):
        """Lead with the decisions; collapse the rest of the body and the review notes."""
        lines = self.page.body.splitlines()
        heading = lambda i, level: lines[i].startswith('#' * level + ' ')
        review = next(i for i, line in enumerate(lines) if line.strip() == '## Review')
        first_h2 = next(i for i, line in enumerate(lines) if heading(i, 2))
        h3 = [i for i in range(review + 1, len(lines)) if heading(i, 3)] + [len(lines)]
        sections = [(h3[k], h3[k + 1], lines[h3[k]][4:].strip()) for k in range(len(h3) - 1)]
        decided = [x for x in sections if x[2].lower().startswith(('decided', 'carried')) or x[2].lower() == 'decisions']
        questions = next((x for x in sections if x[2].lower() == 'open questions'), None)
        if questions and any(not q[2] for q in question_titles(lines[questions[0] + 1:questions[1]])):
            decided.insert(0, questions)
        notes = [x for x in sections if x not in decided]
        inline_decisions = not any(t.lower().startswith('decided') for _, _, t in sections)
        if inline_decisions:
            # Decisions live in the sections they change: keep the body.
            out = [self.render(0, review)]
            self.toc = [(a, t, c) for a, t, c in self.toc]
            for a, b, title in decided:
                self.toc.append((slug(title), title, ''))
                self.review_mode = title.lower() == 'open questions'
                out.append(self.render(a, b))
                self.review_mode = False
            review_notes = Renderer(self.page)
            review_notes.review_mode = True
            body = ''.join(review_notes.render(a, b) for a, b, _ in notes)
            out.append('<details class="collapsed review" id="review"><summary>Review notes: status, consistency and the questions as asked</summary>'
                       + body + '</details>')
            self.toc.append(('review', 'Review notes (collapsed)', 'review-link'))
            return '\n'.join(out)
        out = [self.render(0, first_h2)]
        for a, b, title in decided:
            self.toc.append((slug(title), title, ''))
            self.review_mode = title.lower() == 'open questions'
            part = self.render(a, b)
            self.review_mode = False
            for level in (3, 4):
                part = part.replace(f'<h{level} ', f'<h{level - 1} ').replace(f'</h{level}>', f'</h{level - 1}>')
            out.append(part)
        full = Renderer(self.page)
        out.append('<details class="collapsed" id="reference"><summary>Reference: the full page, with every example and error</summary>'
                   + full.render(first_h2, review) + '</details>')
        self.toc.append(('reference', 'Full reference (collapsed)', ''))
        review_notes = Renderer(self.page)
        review_notes.review_mode = True
        body = ''.join(review_notes.render(a, b) for a, b, _ in notes)
        out.append('<details class="collapsed review" id="review"><summary>Review notes: status, consistency and the questions as asked</summary>'
                   + body + '</details>')
        self.toc.append(('review', 'Review notes (collapsed)', 'review-link'))
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


def render_page(page, pages):
    template = TEMPLATE.read_text()
    renderer = Renderer(page)
    content = renderer.render_decided() if page.meta.get('status') == 'decided' else renderer.render()
    published = load_published()
    nav = build_nav(page, pages, published)
    toc = ''.join(f'<li><a class="{cls}" href="#{a}">{html.escape(t)}</a></li>' for a, t, cls in renderer.toc)
    return (template.replace('{{title}}', html.escape(page.meta.get('title', page.path.stem)))
            .replace('{{area}}', f'{int(page.meta.get("area", 0)):02d}')
            .replace('{{checked}}', html.escape(page.meta.get('checked-with', '')))
            .replace('{{source}}', f'docs/language/{page.path.name}')
            .replace('{{nav}}', ''.join(nav))
            .replace('{{toc}}', toc)
            .replace('{{content}}', content))



def section(body, title):
    lines = body.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == f'### {title}')
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


def render_log(pages, published):
    """One page listing every decision, open question and carried item, built from the area pages."""
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
        decided = [r for r in table_rows(section(page.body, 'Decisions')) if len(r) > 1 and r[1]]
        questions = question_titles(section(page.body, 'Open questions'))
        open_q = [q for q in questions if not q[2]]
        carried_title = 'Carried to later areas' if section(page.body, 'Carried to later areas') else 'Related areas'
        carried = table_rows(section(page.body, carried_title))
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
    # The pages' projects only: tools/examples.py owns examples/.
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


def write_site(pages, out):
    """Every page and the decision log, as HTML in `out`, each page under
    RENDER_LIMIT."""
    out.mkdir(parents=True, exist_ok=True)
    for page in pages:
        with time_limit(page.path.name):
            (out / f'{page.path.stem}.html').write_text(render_page(page, pages))
    published = load_published()
    with time_limit('the decision log'):
        content, toc = render_log(pages, published)
    toc_html = ''.join(f'<li><a href="#{a}">{html.escape(t)}</a></li>' for a, t, _ in toc)
    (out / 'decisions.html').write_text(
        TEMPLATE.read_text()
        .replace('{{area}}', 'log').replace('{{checked}}', 'all pages')
        .replace('{{source}}', 'docs/language/*.md (Decisions, Open questions, Carried tables)')
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


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='verify examples and render the pages without writing them')
    parser.add_argument('--out', default=str(OUT_DEFAULT))
    parser.add_argument('--bless', action='store_true', help='rewrite the output blocks after check blocks')
    parser.add_argument('--export-corpus', action='store_true',
                        help=f'write the projects with tests and their jadpo test --json to {CORPUS.relative_to(ROOT)}')
    args = parser.parse_args()
    catalogue = load_catalogue()
    pages = [Page(p, catalogue) for p in sorted(DOCS.glob('[0-9][0-9]-*.md'))]
    failed = False
    owners = {}
    for page in pages:
        page.check(bless=args.bless)
        blessed = page.bless() if args.bless else 0
        if blessed:
            print(f'{page.path.name}: blessed {blessed} output block(s)')
        for rule in page.claimed_rules():
            owners.setdefault(rule, []).append(page.path.name)
        examples = len([b for b in page.blocks if not b['attrs'].get('fragment')])
        claimed = page.claimed_rules()
        covered = page.covered_rules() & set(claimed)
        print(f'{page.path.name}: {examples} examples, {len(covered)}/{len(claimed)} errors covered')
        for problem in page.problems:
            failed = True
            print(f'  FAIL {problem}')
    # The homepage example in README.md is compiled like a page's examples.
    readme = Readme(ROOT / 'README.md', catalogue)
    readme.check(bless=args.bless)
    if args.bless and readme.bless():
        print('README.md: blessed output block(s)')
    print(f'README.md: {len([b for b in readme.blocks if b["attrs"]["file"].endswith(".jadpo")])} checked file(s)')
    for problem in readme.problems:
        failed = True
        print(f'  FAIL README.md: {problem}')
    for rule, files in owners.items():
        if len(files) > 1:
            failed = True
            print(f'  FAIL {rule} is claimed by {", ".join(files)}')
    # Every rule the compiler has belongs to a page. Area 14's are
    # errors from running a command, not from checking source, so the CLI's
    # own tests show them (crates/jadpo-cli/tests).
    for rule, entry in sorted(catalogue.items()):
        if rule not in owners and entry.get('area') != COMMAND_AREA:
            failed = True
            print(f'  FAIL {rule} (area {entry.get("area")}) is claimed by no page')
    print(f'{len(owners)}/{len(catalogue)} of the compiler\'s rules are assigned to an area')
    corpus = [(page, entry) for page in pages + [readme] for entry in page.corpus]
    print(f'{len(corpus)} spec project(s) with tests ran under jadpo test')
    skipped = [f'{page.path.name} {name}' for page in pages + [readme] for name in page.skipped]
    if skipped:
        print(f'skipped: jadpo test for {len(skipped)} project(s) with migrations, which need PostgreSQL '
              f'(JADPO_SKIP_POSTGRES is set): {", ".join(skipped)}')
    if args.export_corpus:
        export_corpus(corpus)
    # `--check` renders too, into a scratch folder, so a page the renderer
    # can't finish fails the gate rather than only `language_docs.py`.
    if args.check:
        with tempfile.TemporaryDirectory() as scratch:
            write_site(pages, Path(scratch))
        print(f'rendered {len(pages)} page(s)')
    else:
        write_site(pages, Path(args.out))
        print(f'wrote {len(pages)} page(s) to {args.out}')
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
