#!/usr/bin/env python3
"""Tests for the docs renderer in tools/language_docs.py.

The gate's `language_docs.py --check` run renders every page too; these
need no `jadpo` and render the Markdown alone, each under the renderer's
time limit, so a parser that stops moving fails rather than hangs.

  python3 tools/language_docs_test.py
"""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

import language_docs as docs

time_limit = docs.time_limit


def render(markdown):
    page = SimpleNamespace(body=markdown, blocks=[], results={}, catalogue={},
                           claimed_rules=lambda: [], covered_rules=lambda: set())
    with time_limit(repr(markdown)):
        return docs.Renderer(page).render()


class Lists(unittest.TestCase):
    def test_an_indented_item_on_its_own_renders(self):
        # The smallest Markdown that looped: an item no list had started.
        self.assertEqual(render('  - a\n'), '<ul><li>a</li></ul>\n')

    def test_a_sub_list_nests_under_its_item(self):
        self.assertEqual(render('- a\n  - b\n  - c\n- d\n'),
                         '<ul><li>a<ul><li>b</li><li>c</li></ul></li><li>d</li></ul>\n')

    def test_continuation_lines_join_their_item(self):
        self.assertEqual(render('- one\n  two\n- three\n'),
                         '<ul><li>one two</li><li>three</li></ul>\n')

    def test_a_sub_list_under_a_numbered_item(self):
        self.assertEqual(render('1. a\n   more\n   - b\n2. c\n'),
                         '<ol><li>a more<ul><li>b</li></ul></li><li>c</li></ol>\n')

    def test_text_after_a_sub_list_continues_its_item(self):
        self.assertEqual(render('- a\n  - b\n  after\n'),
                         '<ul><li>a<ul><li>b</li></ul> after</li></ul>\n')

    def test_a_list_stops_at_a_blank_line(self):
        self.assertEqual(render('- a\n\ntext\n'), '<ul><li>a</li></ul>\n<p>text</p>\n')

    def test_a_list_stops_at_the_end_it_is_given(self):
        page = SimpleNamespace(body='- a\n- b\n', blocks=[], results={}, catalogue={},
                               claimed_rules=lambda: [], covered_rules=lambda: set())
        with time_limit('a bounded list'):
            self.assertEqual(docs.Renderer(page).render(0, 1), '<ul><li>a</li></ul>\n')

    def test_an_open_question_keeps_its_sub_list_in_its_text(self):
        renderer = docs.Renderer(SimpleNamespace(body='', blocks=[], results={}, catalogue={}))
        renderer.question = lambda number, text: f'Q{number}: {text}'
        renderer.in_questions = True
        renderer.page.body = '1. **Title** body\n   - **Point.** more\n2. **Next** x\n'
        with time_limit('an open question'):
            self.assertEqual(renderer.render(),
                             'Q1: **Title** body - **Point.** more\nQ2: **Next** x\n</ol>')


def real_docs():
    """The repository's pages, notes and error pages, read with a catalogue
    made from the error pages (no `jadpo` needed)."""
    catalogue = {p.stem: {'id': p.stem, 'area': 1, 'summary': 'A rule', 'declinable': False, 'decides': 'agent'}
                 for p in docs.ERRORS.glob('*.md')}
    pages, notes, errors = docs.load_pages(catalogue)
    for page in pages + list(notes.values()) + errors:
        page.collect_blocks()
    return catalogue, pages, notes, errors


class Pages(unittest.TestCase):
    def test_every_language_page_renders(self):
        _, pages, notes, errors = real_docs()
        self.assertTrue(pages and notes and errors)
        with tempfile.TemporaryDirectory() as out:
            docs.write_site(pages, Path(out), notes, errors)
            self.assertEqual(len(list(Path(out).glob('*.html'))), len(pages) + 1)
            self.assertEqual(len(list(Path(out).glob('errors/*.html'))), len(errors))

    def test_every_topic_has_one_id_and_fits(self):
        catalogue, pages, _, errors = real_docs()
        ids = {}
        for page in pages:
            self.assertEqual(page.check_topics(), [], page.path.name)
            for topic in page.topics():
                ids.setdefault(topic['id'], []).append(page.path.name)
        self.assertEqual({i: p for i, p in ids.items() if len(p) > 1}, {})
        bundle = docs.Bundle(pages, errors, {'rules': list(catalogue.values()), 'commands': []})
        sizes = {page_id: docs.words(text) for kind, page_id, _, _, _, text, _ in bundle.build() if kind == 'topic'}
        self.assertEqual(len(sizes), len(ids))
        self.assertEqual({i: n for i, n in sizes.items() if n > docs.TOPIC_WORDS}, {})
        self.assertEqual(bundle.problems, [])


AREA_ONE = """---
title: One
area: 1
---

# One
<!-- topic: one -->

What one is. More about it.

## Shown
<!-- topic: shown -->

See [two's part](02-two.md#the-part), [the rule](../errors/a.b.md),
[the notes](notes/01-one.md) and [later](#later).

```jadpo check project=p file=types.jadpo hidden
type Hidden = Int
```

```jadpo check project=p
type Shown = Int
```

### Later

`[not a link](nowhere.md)`

## Errors

- [`a.b`](../errors/a.b.md)
"""

AREA_TWO = """---
title: Two
area: 2
---

# Two
<!-- topic: two -->

Two's intro.

## Two's part
<!-- topic: twos-part -->

### The part

Back to [one](01-one.md).
"""


def tree(root, one=AREA_ONE, two=AREA_TWO):
    language = root / 'docs/language'
    (language / 'notes').mkdir(parents=True)
    (root / 'docs/errors').mkdir()
    (language / '01-one.md').write_text(one)
    (language / '02-two.md').write_text(two)
    (language / 'notes/01-one.md').write_text('---\ntitle: One, design notes\narea: 1\n---\n\n# One: design notes\n')
    (root / 'docs/errors/a.b.md').write_text('# `a.b`\n\nIn [Shown](../language/01-one.md#shown).\n')
    catalogue = {'a.b': {'id': 'a.b', 'area': 1, 'summary': 'A is b', 'declinable': True, 'decides': 'agent'}}
    pages, _, errors = docs.load_pages(catalogue, root)
    for page in pages + errors:
        page.collect_blocks()
    return catalogue, pages, errors


COMMANDS = [{'name': 'check', 'summary': 'Check the project', 'usage': 'jadpo check [path]',
             'arguments': [{'name': 'path', 'required': False, 'description': 'What to check'}],
             'flags': [], 'examples': [{'description': 'Check it', 'command': 'jadpo check'}],
             'rules': ['a.b']}]


class Bundle(unittest.TestCase):
    def build(self, **pages):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root, **pages)
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': COMMANDS}, root)
            bundle.write(root / 'bundle')
            files = {str(p.relative_to(root / 'bundle')): p.read_text()
                     for p in (root / 'bundle').rglob('*') if p.is_file()}
            return json.loads(files.pop('index.json')), files, bundle

    def test_the_bundle_has_its_shape(self):
        index, files, _ = self.build()
        self.assertEqual(index['version'], '0.1.0')
        self.assertEqual([(e['kind'], e['id']) for e in index['pages']],
                         [('topic', 'one'), ('topic', 'shown'), ('topic', 'two'), ('topic', 'twos-part'),
                          ('error', 'a.b'), ('command', 'jadpo-check')])
        paths = {'topic': 'topics/{}.md', 'error': 'errors/{}.md', 'command': 'commands/{}.md'}
        self.assertEqual(sorted(files), sorted(paths[e['kind']].format(e['id']) for e in index['pages']))
        for entry in index['pages']:
            self.assertEqual(sorted(entry), sorted(['id', 'kind', 'title', 'summary', 'tokens', 'hash', 'area', 'links']))
            text = files[paths[entry['kind']].format(entry['id'])]
            self.assertEqual(entry['hash'], 'sha256:' + hashlib.sha256(text.encode()).hexdigest())
            self.assertEqual(entry['tokens'], (len(text) + 3) // 4)
        by_id = {e['id']: e for e in index['pages']}
        self.assertEqual(by_id['one']['summary'], 'What one is.')
        self.assertEqual(by_id['one']['area'], 1)
        self.assertEqual(by_id['shown']['links'], ['a.b', 'twos-part'])
        self.assertEqual(by_id['a.b']['summary'], 'A is b')
        self.assertEqual(by_id['jadpo-check']['links'], ['a.b'])

    def test_a_topic_is_a_page_of_its_own(self):
        _, files, _ = self.build()
        shown = files['topics/shown.md']
        self.assertTrue(shown.startswith('# Shown\n\nSee '), shown)
        self.assertIn('[two\'s part](twos-part.md#the-part)', shown)
        self.assertIn('[the rule](../errors/a.b.md)', shown)
        self.assertIn('[the notes](https://github.com/richardwillars/jadpo/blob/main/docs/language/notes/01-one.md)', shown)
        self.assertIn('[later](shown.md#later)', shown)
        self.assertIn('`[not a link](nowhere.md)`', shown)
        self.assertIn('## Later', shown)
        self.assertNotIn('Hidden', shown)
        self.assertNotIn('topic:', shown)
        self.assertIn('type Shown = Int', shown)
        self.assertNotIn('Errors', files['topics/shown.md'])
        self.assertIn('[one](one.md)', files['topics/twos-part.md'])
        error = files['errors/a.b.md']
        self.assertTrue(error.startswith('# `a.b`\n\nA is b.\n\nArea 1, [One](../topics/one.md); declinable'), error)
        self.assertIn('[Shown](../topics/shown.md)', error)
        self.assertIn('- [`a.b`](../errors/a.b.md): A is b', files['commands/jadpo-check.md'])


PRIMER = """# Primer

Read this first. See [one](language/01-one.md) and [the rule](errors/a.b.md).

```jadpo check project=p
type Shown = Int {}
```

```jadpo check project=p file=support.jadpo hidden
type Support = Int {}
```

The end.
"""


class Primer(unittest.TestCase):
    def test_the_bundle_leads_with_the_primer(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root)
            (root / 'docs/primer.md').write_text(PRIMER)
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': []}, root, primer=root / 'docs/primer.md')
            bundle.write(root / 'bundle')
            index = json.loads((root / 'bundle/index.json').read_text())
            primer = (root / 'bundle/primer.md').read_text()
        first = index['pages'][0]
        self.assertEqual((first['kind'], first['id'], first['area']), ('primer', 'primer', None))
        self.assertEqual(first['links'], ['a.b', 'one'])
        self.assertIn('[one](topics/one.md)', primer)
        self.assertIn('[the rule](errors/a.b.md)', primer)
        self.assertIn('type Shown', primer)
        self.assertNotIn('Support', primer)
        self.assertIn('```\n\nThe end.', primer)

    def problems(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'primer.md'
            path.write_text(text)
            page = docs.Primer(path, {})
            page.collect_blocks()
            page.check_primer()
            return ' '.join(page.problems)

    def test_its_size_is_checked_without_its_hidden_blocks(self):
        filler = ' '.join(['word'] * 3000) + '\n'
        self.assertEqual(self.problems(PRIMER + filler), '')
        self.assertIn('outside 3,000 to 5,000', self.problems(PRIMER))
        self.assertIn('outside 3,000 to 5,000', self.problems(PRIMER + filler * 2))

    def test_every_example_compiles_and_it_declares_no_topic(self):
        filler = ' '.join(['word'] * 3000) + '\n'
        found = self.problems(PRIMER.replace('check project=p\n', 'fragment\n') + filler
                              + '\n## Later\n<!-- topic: later -->\n')
        self.assertIn('every example in the primer is compiled', found)
        self.assertIn("the primer's topic ID is `primer`", found)


class Structure(unittest.TestCase):
    def problems(self, **pages):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root, **pages)
            catalogue['c.d'] = {'id': 'c.d', 'area': 2, 'summary': 'C is d'}
            return docs.check_structure(pages, errors, catalogue, COMMANDS, root)

    def test_a_tree_in_shape_has_one_problem_the_missing_page(self):
        self.assertEqual(self.problems(), ['c.d (area 2) has no page: add docs/errors/c.d.md'])

    def test_a_topic_needs_an_id(self):
        problems = self.problems(two=AREA_TWO.replace("<!-- topic: twos-part -->\n", ''))
        self.assertIn('declares no topic ID', ' '.join(problems))

    def test_an_id_is_kebab_case_and_used_once(self):
        found = ' '.join(self.problems(two=AREA_TWO.replace('twos-part', 'shown').replace('<!-- topic: two -->',
                                                                                         '<!-- topic: Two -->')))
        self.assertIn('topic ID "shown" is used by 01-one.md "Shown", 02-two.md "Two\'s part"', found)
        self.assertIn('topic ID "Two" is not lowercase kebab case', found)

    def test_an_id_stands_after_a_heading(self):
        found = ' '.join(self.problems(two=AREA_TWO + '\n<!-- topic: stray -->\n'))
        self.assertIn('a topic ID belongs on the line after', found)

    def test_a_topic_over_the_limit_is_split(self):
        long = AREA_TWO + ' '.join(['word'] * docs.TOPIC_WORDS) + '\n'
        self.assertIn('topic "twos-part" is', ' '.join(self.problems(two=long)))

    def test_a_link_resolves_to_a_file_and_a_heading(self):
        found = ' '.join(self.problems(two=AREA_TWO + '\n[gone](03-gone.md) and [lost](01-one.md#lost)\n'))
        self.assertIn('links to 03-gone.md, which isn\'t there', found)
        self.assertIn('links to 01-one.md#lost, but 01-one.md has no heading with that anchor', found)


class Links(unittest.TestCase):
    def test_github_anchors(self):
        seen = {}
        self.assertEqual(docs.github_slug('Saying what can fail: `fails`', seen), 'saying-what-can-fail-fails')
        self.assertEqual(docs.github_slug('`failure.kind_required`', seen), 'failurekind_required')
        self.assertEqual(docs.github_slug('Saying what can fail: `fails`', seen), 'saying-what-can-fail-fails-1')

    def test_llms_txt_links_name_the_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs').mkdir()
            (root / 'docs/a.md').write_text('# A\n')
            (root / 'llms.txt').write_text(
                '- [A](https://raw.githubusercontent.com/richardwillars/jadpo/main/docs/a.md)\n'
                '- [B](https://raw.githubusercontent.com/richardwillars/jadpo/main/docs/b.md)\n'
                '- [Elsewhere](https://example.com/c.md)\n')
            problems = docs.check_links([root / 'llms.txt'], root)
            self.assertEqual(len(problems), 1)
            self.assertIn('docs/b.md', problems[0])

    def test_error_pages_need_their_example(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'a.b.md'
            catalogue = {'a.b': {'id': 'a.b', 'area': 3}, 'cli.x': {'id': 'cli.x', 'area': docs.COMMAND_AREA}}
            path.write_text('# `a.b`\n\nNothing.\n')
            page = docs.ErrorPage(path, catalogue)
            page.collect_blocks()
            page.check_rule()
            self.assertEqual(page.problems, ['no example for a.b: add a check block failing with it'])
            path = Path(tmp) / 'cli.x.md'
            path.write_text('# `cli.x`\n\nA command raises it.\n')
            page = docs.ErrorPage(path, catalogue)
            page.collect_blocks()
            page.check_rule()
            self.assertEqual(page.problems, ['say "No example yet." on a page with no example'])
            path.write_text('# `cli.x`\n\nNo example yet.\n')
            page = docs.ErrorPage(path, catalogue)
            page.collect_blocks()
            page.check_rule()
            self.assertEqual(page.problems, [])


if __name__ == '__main__':
    unittest.main()
