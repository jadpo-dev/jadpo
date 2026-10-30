#!/usr/bin/env python3
"""Tests for the docs renderer in tools/docs/language_docs.py.

The gate's `language_docs.py --check` run renders every page too; these
need no `jadpo` and render the Markdown alone, each under the renderer's
time limit, so a parser that stops moving fails rather than hangs.

  python3 tools/docs/language_docs_test.py
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

    def test_a_renamed_topic_keeps_its_old_ids(self):
        index, files, _ = self.build(two=AREA_TWO.replace('<!-- topic: twos-part -->',
                                                          '<!-- topic: the-part (was twos-part, two-part) -->'))
        by_id = {e['id']: e for e in index['pages']}
        self.assertEqual(by_id['the-part']['aliases'], ['twos-part', 'two-part'])
        self.assertNotIn('aliases', by_id['two'])
        self.assertNotIn('twos-part', by_id)
        self.assertIn('topics/the-part.md', files)
        self.assertNotIn('topic:', files['topics/the-part.md'])

    def test_a_topic_is_a_page_of_its_own(self):
        _, files, _ = self.build()
        shown = files['topics/shown.md']
        self.assertTrue(shown.startswith('# Shown\n\nSee '), shown)
        self.assertIn('[two\'s part](twos-part.md#the-part)', shown)
        self.assertIn('[the rule](../errors/a.b.md)', shown)
        self.assertIn('[the notes](https://github.com/jadpo-dev/jadpo/blob/main/docs/language/notes/01-one.md)', shown)
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

    def test_every_command_page_says_what_json_gives(self):
        commands = [dict(COMMANDS[0]), dict(COMMANDS[0], name='fmt', usage='jadpo fmt')]
        flags = [{'name': '--json', 'description': 'Print data for tools instead of text'},
                 {'name': '--color', 'value': 'when', 'description': 'Choose when to use colour'}]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root)
            docs.Bundle(pages, errors, {'rules': list(catalogue.values()), 'commands': commands,
                                        'flags': flags}, root).write(root / 'bundle')
            for name in ('check', 'fmt'):
                page = (root / f'bundle/commands/jadpo-{name}.md').read_text()
                self.assertIn('## On every command\n\n- `--json`: Print data for tools instead of text\n'
                              '- `--color <when>`: Choose when to use colour\n', page)
                self.assertIn(f'`jadpo {name} --json` writes one JSON document to standard output', page)

    def test_the_agent_skill_goes_in_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root)
            (root / docs.SKILL).parent.mkdir(parents=True)
            (root / docs.SKILL).write_bytes(b'---\nname: jadpo\ndescription: x\n---\n\n# Skill  \n')
            docs.Bundle(pages, errors, {'rules': list(catalogue.values()), 'commands': []}, root).write(root / 'bundle')
            self.assertEqual((root / 'bundle' / docs.SKILL_IN_BUNDLE).read_bytes(), (root / docs.SKILL).read_bytes())
            self.assertEqual(docs.check_skill(root), [])
            (root / docs.SKILL).write_text('---\nname: other\n---\n')
            self.assertEqual(len(docs.check_skill(root)), 2)
            (root / docs.SKILL).unlink()
            self.assertIn('is missing', docs.check_skill(root)[0])

    def test_a_code_fence_keeps_only_its_language(self):
        one = AREA_ONE.replace('### Later', '```text output\nchecked\n```\n\n````markdown\n```jadpo check project=q\n```\n````\n\n### Later')
        _, files, _ = self.build(one=one)
        shown = files['topics/shown.md']
        self.assertIn('```jadpo\ntype Shown = Int\n```', shown)
        self.assertIn('```text\nchecked\n```', shown)
        # What a fence holds is left as it is, a fence written inside it too.
        self.assertIn('````markdown\n```jadpo check project=q\n```\n````', shown)
        self.assertNotIn('project=p', shown)
        self.assertNotIn('text output', shown)


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

    def test_a_help_line_is_a_rules_page_on_the_site(self):
        shown = '```text\nerror: no\n  rule: a.b\n  help: {}\n```\n'
        good = AREA_TWO + shown.format('https://jadpo.dev/docs/errors/a.b')
        self.assertEqual(self.problems(two=good), ['c.d (area 2) has no page: add docs/errors/c.d.md'])
        found = ' '.join(self.problems(two=AREA_TWO + shown.format('docs/errors/a.b.md')))
        self.assertIn('help line names docs/errors/a.b.md, not a rule\'s page on the site', found)
        found = ' '.join(self.problems(two=AREA_TWO + shown.format('https://jadpo.dev/docs/errors/x.y')))
        self.assertIn('help line names https://jadpo.dev/docs/errors/x.y, a rule the catalogue doesn\'t have', found)

    def test_a_topic_needs_an_id(self):
        problems = self.problems(two=AREA_TWO.replace("<!-- topic: twos-part -->\n", ''))
        self.assertIn('declares no topic ID', ' '.join(problems))

    def test_an_id_is_kebab_case_and_used_once(self):
        found = ' '.join(self.problems(two=AREA_TWO.replace('twos-part', 'shown').replace('<!-- topic: two -->',
                                                                                         '<!-- topic: Two -->')))
        self.assertIn('topic ID "shown" is used by 01-one.md "Shown", 02-two.md "Two\'s part"', found)
        self.assertIn('topic ID "Two" is not lowercase kebab case', found)

    def test_an_old_id_is_kebab_case_and_used_once(self):
        found = ' '.join(self.problems(two=AREA_TWO.replace('<!-- topic: twos-part -->',
                                                            '<!-- topic: the-part (was shown, Old) -->')))
        self.assertIn('topic ID "Old" is not lowercase kebab case', found)
        found = ' '.join(self.problems(two=AREA_TWO.replace('<!-- topic: twos-part -->',
                                                            '<!-- topic: the-part (was shown) -->')))
        self.assertIn('topic ID "shown" is used by 01-one.md "Shown", 02-two.md "Two\'s part" (its old ID)', found)

    def test_an_area_page_names_no_area_or_question_by_number(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, pages, _ = tree(Path(tmp), two=AREA_TWO + '\nAs area 9\'s policy says (question 2), and '
                                                          '`area 3` in code.\n\n```text\narea 4\n```\n')
        self.assertEqual(docs.numbered_references(pages[0]), [])
        self.assertEqual([text for _, text in docs.numbered_references(pages[1])], ['area 9', 'question 2'])

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



LEARN_INDEX = """# Learn
<!-- topic: learn -->

Start with [the first page](01-first.md).
"""

LEARN_ONE = """# The first page
<!-- topic: learn-first -->

What the first page teaches. See [one](../language/01-one.md).

`app.jadpo`:

```jadpo check project=learn-app file=app.jadpo
application {}
```

`policy.jadpo`:

```jadpo check project=learn-app file=policy.jadpo
policy {}
```
"""

LEARN_TWO = """---
removes: policy.jadpo
---

# The second page
<!-- topic: learn-second -->

It builds on the first.

`main.jadpo`:

```jadpo check project=learn-app file=main.jadpo
type Count = Int {}
```

## Not built yet

Some of it isn't built yet.
"""


def learn_tree(root, one=LEARN_ONE, two=LEARN_TWO):
    learn = root / 'docs/learn'
    learn.mkdir(parents=True)
    (learn / 'README.md').write_text(LEARN_INDEX)
    (learn / '01-first.md').write_text(one)
    (learn / '02-second.md').write_text(two)
    return docs.load_learn({}, root)


class Learn(unittest.TestCase):
    def test_a_page_builds_on_the_pages_before_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            pages = learn_tree(Path(tmp))
            for page in pages:
                page.collect_blocks()
            index, first, second = pages
            self.assertEqual(index.blocks, [])
            self.assertEqual(sorted(b['attrs']['file'] for b in first.blocks), ['app.jadpo', 'policy.jadpo'])
            # The second page's project is the first's, less what it removes,
            # with its own files; only its own are shown.
            self.assertEqual(sorted(b['attrs']['file'] for b in second.blocks), ['app.jadpo', 'main.jadpo'])
            self.assertEqual([b['attrs']['file'] for b in second.blocks if not b['attrs'].get('inherited')],
                             ['main.jadpo'])
            self.assertEqual({b['attrs']['project'] for b in second.blocks}, {'learn-app'})

    def test_a_record_is_never_inherited(self):
        one = LEARN_ONE + '\n`api/main.jadpo_record`:\n\n```jadpo check project=learn-app file=api/main.jadpo_record\nrecord api 1\n```\n'
        with tempfile.TemporaryDirectory() as tmp:
            pages = learn_tree(Path(tmp), one=one)
            pages[2].collect_blocks()
            self.assertNotIn('api/main.jadpo_record', [b['attrs']['file'] for b in pages[2].blocks])

    def test_each_page_is_one_topic(self):
        with tempfile.TemporaryDirectory() as tmp:
            pages = learn_tree(Path(tmp))
            self.assertEqual([[(t['id'], t['title']) for t in p.topics()] for p in pages],
                             [[('learn', 'Learn')], [('learn-first', 'The first page')],
                              [('learn-second', 'The second page')]])
            self.assertEqual([p.check_topics() for p in pages], [[], [], []])

    def test_the_bundle_holds_the_path_in_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root)
            learn = learn_tree(root)
            for page in learn:
                page.collect_blocks()
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': []}, root, learn=learn)
            bundle.write(root / 'bundle')
            index = json.loads((root / 'bundle/index.json').read_text())
            entries = [(e['kind'], e['id'], e['area']) for e in index['pages'] if e['kind'] == 'learn']
            first = (root / 'bundle/learn/learn-first.md').read_text()
            start = (root / 'bundle/learn/learn.md').read_text()
            second = (root / 'bundle/learn/learn-second.md').read_text()
        self.assertEqual(entries, [('learn', 'learn', None), ('learn', 'learn-first', None),
                                   ('learn', 'learn-second', None)])
        self.assertEqual([e['kind'] for e in index['pages']][:3], ['learn'] * 3)
        self.assertIn('[the first page](learn-first.md)', start)
        self.assertIn('[one](../topics/one.md)', first)
        self.assertNotIn('topic:', first)
        self.assertNotIn('removes:', second)
        self.assertTrue(second.startswith('# The second page\n'), second)

    def user_problems(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs/language').mkdir(parents=True)
            (root / 'docs/language/01-one.md').write_text('# One\n')
            (root / 'examples').mkdir()
            (root / 'examples/README.md').write_text('# Examples\n')
            path = root / 'docs/learn/01-page.md'
            path.parent.mkdir(parents=True)
            path.write_text(text)
            page = docs.LearnPage(path, {}, root=root)
            page.collect_blocks()
            return ' '.join(docs.user_page_problems(page, root))

    def test_a_user_page_follows_the_writing_guide(self):
        clean = '# A page\n<!-- topic: a-page -->\n\nSee [one](../language/01-one.md).\n\n## Not built yet\n\nIt isn\'t built yet.\n'
        self.assertEqual(self.user_problems(clean), '')
        found = self.user_problems(
            '# A page\n<!-- topic: a-page -->\n\nAs area 8 says, and question 11.\n\n'
            'This is not built yet.\n\nDecided. It was carried to area 9.\n\n'
            '[examples](../../examples/README.md) and '
            '[code](https://github.com/jadpo-dev/jadpo/blob/main/README.md).\n\n'
            '```jadpo fragment\nx\n```\n\n## Not built yet\n\nNone.\n\n## After\n\nMore.\n')
        for expected in ('"area 8"', '"question 11"', 'build status', 'spec vocabulary',
                         'links a file in this repository', 'fragment', '`## Not built yet` is the last section'):
            self.assertIn(expected, found)
        self.assertEqual(self.user_problems('# A\n<!-- topic: a -->\n\n`area 8` in code is fine.\n'), '')

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
                '- [A](https://raw.githubusercontent.com/jadpo-dev/jadpo/main/docs/a.md)\n'
                '- [B](https://raw.githubusercontent.com/jadpo-dev/jadpo/main/docs/b.md)\n'
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
