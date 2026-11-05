#!/usr/bin/env python3
"""A page's first example stands alone (docs/writing.md, "A page's first example
stands alone"): this reports each one that doesn't, as far as a machine can tell.

  python3 tools/docs/first_examples.py                    # report the list, and exit 0
  python3 tools/docs/first_examples.py --strict           # exit 1 if the list isn't empty
  python3 tools/docs/first_examples.py --markdown <file>  # also write the list as Markdown
  python3 tools/docs/first_examples.py --every-topic      # judge every topic, not only each page's first example

The docs check compiles an example with its project's hidden blocks, so "every
example compiles" can't see an example that needs something the reader never
sees. This compiles the first example with only what a reader has on that page:
the blocks of its project that are shown between that topic's headings, with an
empty `application {}` where the project's own is out of sight. The compiler
decides, so no list of names here can go stale. `jadpo migrate new` and `jadpo
records update` run as a reader runs them, so a migration or a record is never
counted as missing. A block with `new` starts from what `jadpo new` writes, as a
reader's project does, so that isn't counted as missing either.

A first example is more than its first block when that block only sets things
up: a connection in `app.jadpo` compiles alone, and says nothing of the action
that uses it. So it runs from the first block shown to the first of its project
that does something (DOES: an action, a function, a query, routes, a test), or
to the project's last.

A guide's later steps may lean on files it starts from, so a block after the
first example that has an error is taken out and the rest compiled again, until
the first example has an error or nothing does. What a failure needed is found
by putting the unseen blocks back and taking them out one at a time.

The pages (docs/writing.md says why):

- each guide;
- each group, page and section of the reference's manifest: the first topic in
  its reading order that shows a ```jadpo block, which covers a built-in
  service's home and a landing page with an example. A page's first-listed topic
  often has no code, so stopping there would pass it without reading anything.

Not these: the Learn path, whose pages build on the ones before; error pages,
whose first example is the error; standard type pages, whose examples compile
alone already (type_example_test in language_docs.py); and a command's page and
a generated landing page, which hold no ```jadpo block.

A first example fails when it needs something unseen, when it is a `fragment`, so
nothing proves it compiles, or when it has `expect=`, so it isn't a working use.
A `fragment` on a topic the manifest says isn't built is its design, so it is
listed as planned and doesn't fail.

The compiler can't say whether an example is the simplest one, whether the page
says where each name comes from, or whether it runs on every target. Those stay
with the writer and the reviewer. Nor does this see a name the page defines in a
block further down, which it passes, as the rule's "shown on that page" does.
"""

import argparse
import json
import re
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import language_docs as docs
import reference_manifest
from reference_manifest import NOT_BUILT

STANDS, NEEDS, FRAGMENT, FAILURE, PLANNED = 'stands', 'needs', 'fragment', 'failure', 'planned'
FAILING = (NEEDS, FRAGMENT, FAILURE)
GUIDES = 'Guides'
APP = 'app.jadpo'
EMPTY_APP = 'application {}\n'
# Errors that aren't something missing: what a reader's commands fix, and a name
# nothing uses yet, which is only so while the code that uses it is out of sight.
NOT_A_NEED = ('migration.out_of_date', 'record.', 'layout.unused_declaration', 'semantic.unused_variable')
ONE_LINE, SMALL_BLOCK, REWRITE = 'one line', 'a small block', 'a real rewrite'
SMALL_BLOCK_LINES = 15
# A declaration that does something, which ends a first example.
DOES = re.compile(r'^(?:public\s+)?(?:action|function|query|routes|library|subscriber|schedule|test)\b', re.MULTILINE)


def generated(block):
    """A file a reader's command writes, so it is never theirs to be shown."""
    file = block['attrs']['file']
    return file.endswith('.jadpo_record') or file.startswith('database/migrations/')


def report_of(page, name, blocks, needs_app):
    """What `jadpo check --json` says of a project of `blocks`: its errors, as
    (rule, path, message). Run as a reader runs it, after their commands."""
    with tempfile.TemporaryDirectory() as tmp:
        files = [{**block, 'attrs': {k: v for k, v in block['attrs'].items() if k != 'migrate'}} for block in blocks]
        docs.write_project(Path(tmp), name, files)
        if needs_app and not any(block['attrs']['file'] == APP for block in blocks):
            (Path(tmp) / APP).write_text(EMPTY_APP)
        docs.jadpo(['migrate', 'new', 'initial'], cwd=tmp, env={'SOURCE_DATE_EPOCH': str(docs.MIGRATION_EPOCH)})
        docs.jadpo(['records', 'update'], cwd=tmp)
        code, out = docs.jadpo(['check', '--json', tmp])
    if code == 0:
        return []
    try:
        found = json.loads(out)['diagnostics']
    except (ValueError, KeyError):
        return [('(no report)', None, out.strip()[:200])]
    return [(d['rule'], (d.get('location') or {}).get('path'), d['message']) for d in found
            if not d['rule'].startswith(NOT_A_NEED)]


def stands(page, name, example, others, needs_app):
    """Whether the blocks of `example` compile with as many of `others` as can.
    A block of `others` with an error of its own comes out, and the rest compile
    again: if the example leant on it, the example now has the error. Gives
    (whether it stands, the errors that stopped it)."""
    kept = list(others)
    while True:
        errors = report_of(page, name, [*example, *kept], needs_app)
        if not errors:
            return True, []
        paths = {path for _, path, _ in errors}
        files = {block['attrs']['file'] for block in kept}
        if not paths <= files:
            return False, errors
        kept = [block for block in kept if block['attrs']['file'] not in paths]


def what_it_needed(page, name, example, shown, unseen, needs_app):
    """The fewest unseen blocks the first example stands with, or None when it
    doesn't stand with all of them."""
    needed = list(unseen)
    if not stands(page, name, example, shown + needed, needs_app)[0]:
        return None
    for block in list(needed):
        without = [other for other in needed if other is not block]
        if stands(page, name, example, shown + without, needs_app)[0]:
            needed = without
    return needed


def example_of(blocks):
    """Where a project's shown blocks split: the first example, up to the first
    block that does something, and the rest."""
    for i, block in enumerate(blocks):
        if block['attrs']['file'].endswith('.jadpo') and DOES.search(block['source']):
            return blocks[:i + 1], blocks[i + 1:]
    return blocks, []


def first_occurrences(blocks):
    """A file as the reader first has it: a later copy is a later step."""
    seen, found = set(), []
    for block in blocks:
        if block['attrs']['file'] not in seen:
            seen.add(block['attrs']['file'])
            found.append(block)
    return found


def shown_blocks(page, topic):
    """The ```jadpo blocks a reader sees on a topic, in order."""
    return [block for block in page.blocks
            if topic['start'] < block['line'] < topic['end']
            and not block['attrs'].get('hidden') and not generated(block)]


def lines_of(blocks):
    return sum(len(block['source'].strip().splitlines()) for block in blocks)


def fix_size(lines):
    """Roughly how much writing a failure takes, from the lines to show. It is a
    guess: a fix can be to cut the part that needs them instead."""
    return ONE_LINE if lines <= 1 else SMALL_BLOCK if lines <= SMALL_BLOCK_LINES else REWRITE


def fragment_fix(page, first):
    """A `fragment` that compiles as it stands needs only its fence changed; one
    that doesn't needs what it uses shown too."""
    if not report_of(page, first['attrs']['project'], [first], False):
        return ONE_LINE
    return fix_size(lines_of([first]))


def judge(item):
    """One page's verdict: `item` is what `pages_to_judge` gave it."""
    page, topic, shown = item['page'], item['topic'], shown_blocks(item['page'], item['topic'])
    first = shown[0]
    where = f'{page.path.name}:{first["line"] + page.offset + 1}'
    entry = {'group': item['group'], 'title': item['title'], 'topic': topic['id'], 'where': where,
             'project': first['attrs']['project'], 'errors': [], 'needed': [], 'fix': None}
    if first['attrs'].get('fragment'):
        entry['verdict'] = PLANNED if item['status'] == NOT_BUILT else FRAGMENT
        if entry['verdict'] == FRAGMENT:
            entry['fix'] = fragment_fix(page, first)
    elif 'expect' in first['attrs']:
        entry['verdict'] = FAILURE
        entry['fix'] = REWRITE
        entry['errors'] = [(first['attrs']['expect'], None, 'shows this error rather than a working use')]
    else:
        name = first['attrs']['project']
        project = [block for block in page.blocks if block['attrs'].get('check')
                   and block['attrs']['project'] == name and not generated(block)]
        needs_app = any(block['attrs']['file'] == APP for block in project)
        example, others = example_of(first_occurrences([block for block in shown if block['attrs'].get('check')
                                                        and block['attrs']['project'] == name]))
        taken = {block['attrs']['file'] for block in example + others}
        unseen = first_occurrences([block for block in project if block['attrs']['file'] not in taken])
        ok, errors = stands(page, name, example, others, needs_app)
        entry['verdict'] = STANDS if ok else NEEDS
        entry['errors'] = errors
        entry['example'] = [block['attrs']['file'] for block in example]
        if not ok:
            needed = what_it_needed(page, name, example, others, unseen, needs_app)
            entry['needed'] = [{'file': block['attrs']['file'], 'lines': lines_of([block]),
                                'hidden': bool(block['attrs'].get('hidden'))} for block in needed or []]
            entry['fix'] = fix_size(lines_of(needed)) if needed else REWRITE
    return entry


def pages_to_judge(root=docs.ROOT, every_topic=False):
    """Each page with a first example, and `counts`: {group: pages looked at}."""
    area_pages = [docs.Page(path, {}) for path in sorted((root / 'docs/language').glob('[0-9][0-9]-*.md'))]
    topics = {}
    for page in area_pages:
        page.collect_blocks()
        for topic in page.topics():
            if topic['id']:
                topics[topic['id']] = (page, topic)
    reference = reference_manifest.load(root)
    known = {i: {'id': i, 'kind': 'topic', 'title': i, 'summary': ''} for i in topics}
    unfinished = reference.unfinished(known, None) if reference else {}
    items, counts, seen = [], {}, set()

    def add(group, title, page, topic, status):
        key = (page.path, topic['start'])
        if key not in seen:
            seen.add(key)
            items.append({'group': group, 'title': title, 'page': page, 'topic': topic, 'status': status})

    for node, trail in reference.walk() if reference else ():
        ids = ([node['id']] if node.get('id') in topics else []) + list(node.get('topics', []))
        if every_topic:
            ids += list(node.get('details', []))
        ids = [i for i in ids if i in topics]
        if not ids:
            continue
        counts[trail[0]] = counts.get(trail[0], 0) + 1
        for page_id in ids:
            page, topic = topics[page_id]
            if shown_blocks(page, topic):
                status = (unfinished.get(page_id) or {}).get('status')
                add(trail[0], ' › '.join(trail) if not every_topic else f'{" › ".join(trail)}: {topic["title"]}',
                    page, topic, status)
                if not every_topic:
                    break
    for page in docs.load_guides({}, root):
        page.collect_blocks()
        for topic in page.topics():
            if page.path.name == 'README.md':
                continue
            counts[GUIDES] = counts.get(GUIDES, 0) + 1
            if shown_blocks(page, topic):
                add(GUIDES, topic['title'], page, topic, None)
    return items, counts


def judged(root=docs.ROOT, every_topic=False):
    items, counts = pages_to_judge(root, every_topic)
    docs.ensure_jadpo()
    with ThreadPoolExecutor(docs.WORKERS) as pool:
        return list(pool.map(judge, items)), counts


def summary(entries, counts):
    """Per group: pages, pages with a first example, failures, planned."""
    rows = []
    for group in counts:
        mine = [entry for entry in entries if entry['group'] == group]
        rows.append((group, counts[group], len(mine), sum(e['verdict'] in FAILING for e in mine),
                     sum(e['verdict'] == PLANNED for e in mine)))
    return rows


def why(entry):
    if entry['verdict'] == FRAGMENT:
        return 'its first example is a `fragment`, so nothing proves it compiles'
    if entry['verdict'] == FAILURE:
        return f'its first example shows the error `{entry["errors"][0][0]}`, not a working use'
    if entry['needed']:
        files = ', '.join(f'{n["file"]} ({n["lines"]} lines{"" if n["hidden"] else ", on another topic"})'
                          for n in entry['needed'])
        return f'needs {files}'
    return 'needs ' + '; '.join(f'{rule}: {message}' for rule, _, message in entry['errors'][:2])


def markdown(entries, counts):
    out = ['# First examples', '', '| Group | Pages | With a first example | Fail | Planned |', '|---|---|---|---|---|']
    rows = summary(entries, counts)
    out += [f'| {group} | {pages} | {shown} | {fails} | {planned} |' for group, pages, shown, fails, planned in rows]
    out += [f'| **All** | {sum(r[1] for r in rows)} | {sum(r[2] for r in rows)} | {sum(r[3] for r in rows)} '
            f'| {sum(r[4] for r in rows)} |', '']
    for heading, verdicts in (('Fail', FAILING), ('Planned', (PLANNED,)), ('Stand alone', (STANDS,))):
        chosen = [entry for entry in entries if entry['verdict'] in verdicts]
        out += [f'## {heading} ({len(chosen)})', '']
        for entry in chosen:
            fix = f' Fix: {entry["fix"]}.' if entry['fix'] else ''
            detail = f' {why(entry)}.' if entry['verdict'] in FAILING else ''
            out.append(f'- **{entry["title"]}** (`{entry["topic"]}`, {entry["where"]}):{detail}{fix}')
        out.append('')
    return '\n'.join(out)


def main(argv=None, root=docs.ROOT):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--strict', action='store_true', help='exit 1 if any first example fails')
    parser.add_argument('--markdown', metavar='FILE', help='also write the whole list as Markdown')
    parser.add_argument('--every-topic', action='store_true', help="judge every topic, not only each page's first")
    args = parser.parse_args(argv)
    entries, counts = judged(root, args.every_topic)
    fails = [entry for entry in entries if entry['verdict'] in FAILING]
    for entry in fails:
        print(f'  first example fails: {entry["title"]} ({entry["where"]}): {why(entry)}')
    for group, pages, shown, failed, planned in summary(entries, counts):
        print(f'first examples: {group}: {pages} page(s), {shown} with a first example, {failed} fail, {planned} planned')
    print(f'first examples: {len(fails)} of {len(entries)} fail'
          + ('' if args.strict else ' (report only: --strict makes this fail)'))
    if args.markdown:
        Path(args.markdown).write_text(markdown(entries, counts))
    return 1 if args.strict and fails else 0


if __name__ == '__main__':
    sys.exit(main())
