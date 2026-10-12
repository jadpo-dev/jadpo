#!/usr/bin/env python3
"""Check and render the language reference in docs/language.

Every ```jadpo block is compiled with the current compiler. Passing projects
must also be canonically formatted; failing projects must report the rule named
in `expect=`. `## Errors` sections show the compiler's real output, and every
catalogue rule an area claims must have an example there.

  python3 tools/language_docs.py --check     # verify only
  python3 tools/language_docs.py             # verify and write build/language-site
"""

import argparse
import html
import json
import os
import re
import subprocess
import sys
import tempfile
from fnmatch import fnmatch
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / 'docs/language'
COMPILER = ROOT / 'jadpo/target/debug/jadpo'
TEMPLATE = Path(__file__).resolve().parent / 'language_docs_template.html'
OUT_DEFAULT = ROOT / 'build/language-site'
# Optional build/language-site/published.json maps page stems to published URLs for cross-page links.
FENCE = re.compile(r'^```(\S*)(.*)$')


def run(args, cwd=None):
    result = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def load_catalogue():
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / 'a.jadpo').write_text('type Count = Int {\n    min: 0\n}\n')
        code, out = run([str(COMPILER), 'artifacts', tmp])
        if code:
            sys.exit(f'could not generate the diagnostic catalogue:\n{out}')
        data = json.loads((Path(tmp) / 'build/diagnostics/catalogue.json').read_text())
    return {entry['ruleId']: entry for entry in data['diagnostics']}


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
        self.catalogue = catalogue
        self.meta, self.body = parse_front_matter(path.read_text())
        self.blocks = []
        self.problems = []
        self.results = {}

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
                self.blocks.append({'line': i, 'attrs': attrs, 'source': '\n'.join(lines[i + 1:j]) + '\n'})
                i = j
            i += 1

    def compile_projects(self):
        projects = {}
        for block in self.blocks:
            if block['attrs'].get('fragment'):
                continue
            projects.setdefault(block['attrs']['project'], []).append(block)
        for name, blocks in projects.items():
            expected = {b['attrs']['expect'] for b in blocks if 'expect' in b['attrs']} or {'pass'}
            if len(expected) > 1:
                self.problems.append(f'{name}: blocks disagree on expect=')
            expected = expected.pop()
            with tempfile.TemporaryDirectory() as tmp:
                for block in blocks:
                    target = Path(tmp) / block['attrs']['file']
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(block['source'])
                code, out = run([str(COMPILER), 'check', tmp])
                out = out.replace(tmp + '/', '').replace(tmp, '.')
                rules = re.findall(r'rule: (\S+)', out)
                formatted = None
                if code == 0:
                    fmt_code, _ = run([str(COMPILER), 'fmt', tmp, '--check'])
                    formatted = fmt_code == 0
            if expected == 'pass':
                ok = code == 0 and formatted
                if code:
                    self.problems.append(f'{name}: expected to compile but failed with {rules[:3]}')
                elif not formatted:
                    self.problems.append(f'{name}: compiles but is not canonically formatted (run jadpo fmt)')
            else:
                ok = expected in rules
                if not ok:
                    self.problems.append(f'{name}: expected {expected} but got {rules[:3] or "a pass"}')
            self.results[name] = {'expected': expected, 'ok': ok, 'output': out.strip(), 'rules': rules}

    def claimed_rules(self):
        patterns = self.meta.get('rules', [])
        return sorted(r for r in self.catalogue if any(fnmatch(r, p) for p in patterns))

    def covered_rules(self):
        return {r['expected'] for r in self.results.values() if r['expected'] != 'pass' and r['ok']}

    def check(self):
        self.collect_blocks()
        self.compile_projects()
        missing = [r for r in self.claimed_rules() if r not in self.covered_rules()]
        for rule in missing:
            self.problems.append(f'no example for {rule}')
        return missing


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


def render_diagnostic(output):
    lines = []
    for line in html.escape(output).splitlines():
        line = re.sub(r'^(error|warning):', r'<span class="e">\1:</span>', line)
        line = re.sub(r'^(\s*)(rule:) (\S+)', r'\1<span class="dim">\2</span> <span class="rule">\3</span>', line)
        line = re.sub(r'^(\s*)(reason:|next:|help:|note:|behavioral impact:|public contract impact:)',
                      r'\1<span class="dim">\2</span>', line)
        line = re.sub(r'^(\s*\|\s*)(\^+)', r'\1<span class="caret">\2</span>', line)
        lines.append(line)
    return '\n'.join(lines)


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
            status = '<span class="check">✓ compiles · formatted</span>'
        else:
            status = f'<span class="check">✓ fails with {html.escape(result["expected"])}</span>'
        out = (f'<div class="code"><div class="label"><span>{html.escape(attrs["file"])}</span>{status}</div>'
               f'<pre><code class="jadpo">{html.escape(block["source"].rstrip())}</code></pre></div>')
        if result and 'expect' in attrs and result['expected'] != 'pass':
            out += (f'<div class="diag"><div class="label">jadpo check</div>'
                    f'<pre>{render_diagnostic(result["output"])}</pre></div>')
        return out

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
            item = re.match(r'^(\s*)(- |\d+\. )(.*)$', line)
            if item:
                flush()
                ordered = item.group(2)[0].isdigit()
                items = []
                while i < len(lines):
                    m = re.match(r'^(- |\d+\. )(.*)$', lines[i])
                    if m:
                        items.append([m.group(2)])
                    elif lines[i].startswith('   ') and lines[i].strip() and items:
                        items[-1].append(lines[i].strip())
                    else:
                        break
                    i += 1
                if self.in_questions and ordered:
                    for k, parts in enumerate(items, 1):
                        out.append(self.question(k, ' '.join(parts)))
                else:
                    tag = 'ol' if ordered else 'ul'
                    out.append(f'<{tag}>' + ''.join(f'<li>{inline(" ".join(p))}</li>' for p in items) + f'</{tag}>')
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
        """Lead with the decisions; collapse today's behaviour and the review notes."""
        lines = self.page.body.splitlines()
        heading = lambda i, level: lines[i].startswith('#' * level + ' ')
        review = next(i for i, line in enumerate(lines) if line.strip() == '## Review')
        first_h2 = next(i for i, line in enumerate(lines) if heading(i, 2))
        h3 = [i for i in range(review + 1, len(lines)) if heading(i, 3)] + [len(lines)]
        sections = [(h3[k], h3[k + 1], lines[h3[k]][4:].strip()) for k in range(len(h3) - 1)]
        decided = [x for x in sections if x[2].lower().startswith('decided') or x[2].lower() == 'decisions']
        notes = [x for x in sections if x not in decided]
        out = [self.render(0, first_h2)]
        for a, b, title in decided:
            self.toc.append((slug(title), title, ''))
            part = self.render(a, b)
            for level in (3, 4):
                part = part.replace(f'<h{level} ', f'<h{level - 1} ').replace(f'</h{level}>', f'</h{level - 1}>')
            out.append(part)
        today = Renderer(self.page)
        out.append('<details class="collapsed" id="today"><summary>How the compiler works today, before the rework</summary>'
                   + today.render(first_h2, review) + '</details>')
        self.toc.append(('today', 'Today (collapsed)', ''))
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


def render_page(page, pages):
    template = TEMPLATE.read_text()
    renderer = Renderer(page)
    content = renderer.render_decided() if page.meta.get('status') == 'decided' else renderer.render()
    nav = []
    titles = {int(p.meta.get('area', 0)): p for p in pages}
    published = {}
    links = Path(OUT_DEFAULT) / 'published.json'
    if links.exists():
        published = json.loads(links.read_text())
    for number, name in AREAS:
        if number in titles:
            current = ' class="current" aria-current="page"' if titles[number] is page else ''
            href = published.get(titles[number].path.stem, f'{titles[number].path.stem}.html')
            nav.append(f'<li><a{current} href="{href}"><span class="n">{number:02d}</span>{name}</a></li>')
        else:
            nav.append(f'<li><span><span class="n">{number:02d}</span>{name}</span></li>')
    toc = ''.join(f'<li><a class="{cls}" href="#{a}">{html.escape(t)}</a></li>' for a, t, cls in renderer.toc)
    return (template.replace('{{title}}', html.escape(page.meta.get('title', page.path.stem)))
            .replace('{{area}}', f'{int(page.meta.get("area", 0)):02d}')
            .replace('{{checked}}', html.escape(page.meta.get('checked-with', '')))
            .replace('{{source}}', f'docs/language/{page.path.name}')
            .replace('{{nav}}', ''.join(nav))
            .replace('{{toc}}', toc)
            .replace('{{content}}', content))


AREAS = [
    (1, 'Project layout and modules'), (2, 'Values and types'), (3, 'Logic'), (4, 'Failures'),
    (5, 'Data'), (6, 'Reads and writes'), (7, 'HTTP'), (8, 'Identity'), (9, 'Policy'),
    (10, 'Configuration and time'), (11, 'External services'), (12, 'Jobs and events'),
    (13, 'Testing'), (14, 'Tooling'),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='verify examples without writing pages')
    parser.add_argument('--out', default=str(OUT_DEFAULT))
    args = parser.parse_args()
    if not COMPILER.exists():
        sys.exit('build the compiler first: cargo build --manifest-path jadpo/Cargo.toml -p jadpo-cli')
    catalogue = load_catalogue()
    pages = [Page(p, catalogue) for p in sorted(DOCS.glob('[0-9][0-9]-*.md'))]
    failed = False
    owners = {}
    for page in pages:
        page.check()
        for rule in page.claimed_rules():
            owners.setdefault(rule, []).append(page.path.name)
        examples = len([b for b in page.blocks if not b['attrs'].get('fragment')])
        print(f'{page.path.name}: {examples} examples, {len(page.covered_rules())}/{len(page.claimed_rules())} errors covered')
        for problem in page.problems:
            failed = True
            print(f'  FAIL {problem}')
    for rule, files in owners.items():
        if len(files) > 1:
            failed = True
            print(f'  FAIL {rule} is claimed by {", ".join(files)}')
    print(f'{len(owners)}/{len(catalogue)} catalogue errors are assigned to an area')
    if not args.check:
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        for page in pages:
            (out / f'{page.path.stem}.html').write_text(render_page(page, pages))
        print(f'wrote {len(pages)} page(s) to {out}')
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
