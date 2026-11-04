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

    def test_every_topic_has_its_place_in_the_manifest(self):
        catalogue, pages, _, errors = real_docs()
        bundle = docs.Bundle(pages, errors, {'rules': list(catalogue.values()), 'commands': []})
        self.assertEqual(bundle.reference_problems(bundle.build()), [])


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


REFERENCE = """
[[group]]
title = "Things"

[[group.page]]
id = "one"
title = "One"
topics = ["shown"]

[[group.page]]
title = "Two"
topics = ["two"]
details = ["twos-part"]
commands = true
"""


def tree(root, one=AREA_ONE, two=AREA_TWO, reference=REFERENCE):
    language = root / 'docs/language'
    (language / 'notes').mkdir(parents=True)
    if reference is not None:
        (root / 'docs/reference.toml').write_text(reference)
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
            self.nav = json.loads(files.pop('nav.json')) if 'nav.json' in files else None
            return json.loads(files.pop('index.json')), files, bundle

    def test_the_bundle_has_its_shape(self):
        index, files, _ = self.build()
        self.assertEqual(index['version'], '0.1.0')
        self.assertEqual([(e['kind'], e['id']) for e in index['pages']],
                         [('topic', 'one'), ('topic', 'shown'), ('topic', 'two'), ('topic', 'twos-part'),
                          ('error', 'a.b'), ('command', 'jadpo-check')])
        paths = {'topic': 'topics/{}.md', 'error': 'errors/{}.md', 'command': 'commands/{}.md'}
        self.assertEqual(sorted(files), sorted(paths[e['kind']].format(e['id']) for e in index['pages']))
        placed = {'one': ('Things', 'One'), 'shown': ('Things', 'One'), 'two': ('Things', 'Two'),
                  'twos-part': ('Things', 'Two'), 'jadpo-check': ('Things', 'Two')}
        for entry in index['pages']:
            place = ['group', 'page', 'status'] if entry['id'] in placed else []
            self.assertEqual(sorted(entry), sorted(['id', 'kind', 'title', 'summary', 'tokens', 'hash', 'area', 'links'] + place))
            if place:
                self.assertEqual((entry['group'], entry['page']), placed[entry['id']])
            text = files[paths[entry['kind']].format(entry['id'])]
            self.assertEqual(entry['hash'], 'sha256:' + hashlib.sha256(text.encode()).hexdigest())
            self.assertEqual(entry['tokens'], (len(text) + 3) // 4)
        by_id = {e['id']: e for e in index['pages']}
        self.assertEqual(by_id['one']['summary'], 'What one is.')
        self.assertEqual(by_id['one']['area'], 1)
        self.assertEqual(by_id['shown']['links'], ['a.b', 'twos-part'])
        self.assertEqual(by_id['a.b']['summary'], 'A is b')
        self.assertEqual(by_id['jadpo-check']['links'], ['a.b'])

    def test_nav_json_is_the_tree_with_its_addresses(self):
        self.build()
        self.assertEqual(self.nav['version'], '0.1.0')
        group = self.nav['groups'][0]
        self.assertEqual(group['title'], 'Things')
        self.assertEqual(group['href'], '/docs/one')
        one, two = group['children']
        self.assertEqual((one['id'], one['href'], one['summary']), ('one', '/docs/one', 'What one is.'))
        self.assertEqual([t['href'] for t in two['topics']], ['/docs/two', '/docs/jadpo-check'])
        self.assertEqual([t['kind'] for t in two['topics']], ['topic', 'command'])
        self.assertEqual([t['id'] for t in two['details']], ['twos-part'])

    def test_no_manifest_writes_no_nav_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root, reference=None)
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': COMMANDS}, root)
            bundle.write(root / 'bundle')
            self.assertFalse((root / 'bundle/nav.json').exists())
            self.assertTrue((root / 'bundle/index.json').exists())

    def test_a_standard_types_group_has_its_compiler_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root, reference=REFERENCE + '''
[[group]]
title = "Types"
id = "types"
summary = "Every type."

[[group.page]]
title = "Money"
id = "types-money"
summary = "Money and what goes with it."
types = "Money"
''')
            types = [{'name': 'Money', 'group': 'Money', 'summary': 'A sum'}, {'name': 'Iban', 'group': 'Money', 'summary': 'An account'}]
            type_pages = {'index': '# Standard types\n\nEvery type.\n',
                          'types': [{'id': 'type-money', 'name': 'Money', 'page': '# Money\n\nA sum.\n'},
                                    {'id': 'type-iban', 'name': 'Iban', 'page': '# Iban\n\nAn account.\n'}]}
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()), 'commands': COMMANDS,
                                                 'types': types, 'type_pages': type_pages}, root)
            built = bundle.write(root / 'bundle')
            nav = json.loads((root / 'bundle/nav.json').read_text())
            index = {e['id']: e for e in json.loads((root / 'bundle/index.json').read_text())['pages']}
            self.assertEqual(bundle.reference_problems(built), [])
        money = nav['groups'][1]['children'][0]
        self.assertEqual(nav['groups'][1]['href'], '/docs/types')
        self.assertEqual([(t['id'], t['href']) for t in money['topics']],
                         [('type-money', '/docs/types/type-money'), ('type-iban', '/docs/types/type-iban')])
        self.assertEqual((index['type-iban']['group'], index['type-iban']['page']), ('Types', 'Money'))

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


class BundleLinks(unittest.TestCase):
    """Every link on a built page reaches a page in the bundle, and its
    anchor a heading of that page (docs/documentation.md, "The bundle")."""

    AREA = """---
title: One
area: 1
---

# One
<!-- topic: one -->

See [the wrapped
part](02-two.md#the-part), [Later](#later), [the first Example](#example) and
[the second Example](#example-1).

## Shown
<!-- topic: shown -->

### Example

### Later

## Other
<!-- topic: other -->

### Example

## Errors

### Rules for one

- [`a.b`](../errors/a.b.md)
"""

    def build(self, one=None, two=AREA_TWO):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root, one=one or self.AREA, two=two)
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': COMMANDS}, root)
            return bundle, bundle.build()

    def page(self, built, path):
        return next(text for _, _, at, _, _, text, _ in built if at == path)

    def test_a_link_wrapped_over_two_lines_is_written_by_id(self):
        _, built = self.build()
        self.assertIn('[the wrapped\npart](twos-part.md#the-part)', self.page(built, 'topics/one.md'))

    def test_a_wrapped_link_is_found_at_its_line(self):
        self.assertEqual(list(docs.links_in('one\n\ntwo [a\nb](c.md) `[d\ne](f.md)`\n')), [(3, 'c.md')])

    def test_an_anchor_is_the_one_the_topics_own_page_gives(self):
        # The second "Example" on the area page is `example-1`; on its own
        # topic's page it is the first.
        _, built = self.build()
        one = self.page(built, 'topics/one.md')
        self.assertIn('[Later](shown.md#later)', one)
        self.assertIn('[the first Example](shown.md#example)', one)
        self.assertIn('[the second Example](other.md#example)', one)
        self.assertEqual(docs.bundle_link_problems(built), [])

    def test_a_link_to_a_section_no_page_keeps_goes_to_the_page(self):
        # `## Errors` is left out of every topic's page, and what is under it.
        for anchor in ('errors', 'rules-for-one'):
            area = self.AREA.replace('## Other', f'See [the errors](#{anchor}).\n\n## Other')
            _, built = self.build(one=area)
            self.assertIn('[the errors](one.md)', self.page(built, 'topics/shown.md'))
            self.assertEqual(docs.bundle_link_problems(built), [])

    def test_a_link_to_a_page_outside_the_bundle_is_a_problem(self):
        built = [('topic', 'one', 'topics/one.md', 'One', 1, '# One\n\n[gone](gone.md) [far](https://example.com/x.md)\n', set())]
        problems = docs.bundle_link_problems(built)
        self.assertEqual(len(problems), 1)
        self.assertIn('topics/one.md:3', problems[0])
        self.assertIn('gone.md', problems[0])

    def test_a_link_to_a_heading_its_page_lacks_is_a_problem(self):
        built = [('topic', 'one', 'topics/one.md', 'One', 1, '# One\n\n[a](two.md#here) [b](two.md#gone)\n', set()),
                 ('topic', 'two', 'topics/two.md', 'Two', 1, '# Two\n\n## Here\n', set())]
        problems = docs.bundle_link_problems(built)
        self.assertEqual(len(problems), 1)
        self.assertIn('two.md#gone', problems[0])

    def test_a_link_between_folders_is_followed(self):
        built = [('error', 'a.b', 'errors/a.b.md', 'a.b', 1, '[one](../topics/one.md) [p](../primer.md)\n', set()),
                 ('topic', 'one', 'topics/one.md', 'One', 1, '# One\n', set()),
                 ('primer', 'primer', 'primer.md', 'Primer', None, '# Primer\n', set())]
        self.assertEqual(docs.bundle_link_problems(built), [])

    def test_a_link_in_code_is_not_a_link(self):
        built = [('topic', 'one', 'topics/one.md', 'One', 1, '`[a](gone.md)`\n\n```text\n[b](gone.md)\n```\n', set())]
        self.assertEqual(docs.bundle_link_problems(built), [])


class WhatTheSiteServes(unittest.TestCase):
    """index.json's kinds, each at a path docs/documentation.md's "What the
    site serves" gives: the site builds its addresses from that table."""

    KINDS = {'primer', 'learn', 'guide', 'topic', 'type', 'error', 'command'}

    def test_every_kind_has_its_path_and_the_table_says_so(self):
        self.assertEqual(docs.site_path_problems(), [])

    def test_the_kinds_are_the_ones_the_bundle_writes(self):
        self.assertEqual(set(docs.SITE_PATHS), self.KINDS)
        documented = (docs.ROOT / 'docs/documentation.md').read_text()
        self.assertIn('(`primer`, `learn`, `guide`, `topic`, `type`, `error`, `command`)', documented)

    def test_a_built_kind_without_a_path_is_a_problem(self):
        problems = docs.site_path_problems(kinds={'topic', 'recipe'})
        self.assertEqual(len(problems), 1)
        self.assertIn('recipe', problems[0])

    def test_a_kind_the_table_doesnt_name_is_a_problem(self):
        table = '## What the site serves\n\n| Path | What |\n|---|---|\n| `/docs/<id>` | A topic |\n\n## Next\n'
        problems = docs.site_path_problems(kinds={'topic', 'command'}, documented=table)
        self.assertEqual(len(problems), 1)
        self.assertIn('command', problems[0])

    def test_a_path_the_table_doesnt_give_is_a_problem(self):
        table = '## What the site serves\n\n| Path | What |\n|---|---|\n| `/guides/<id>` | A guide |\n\n## Next\n'
        problems = docs.site_path_problems(kinds={'topic'}, documented=table)
        self.assertEqual(len(problems), 1)
        self.assertIn('/docs/<id>', problems[0])

    def test_a_page_the_table_gives_that_no_kind_claims_is_a_problem(self):
        table = ('## What the site serves\n\n| Path | What |\n|---|---|\n| `/docs/<id>` | A topic |\n'
                 '| `/recipes/<id>`, `.md` | A recipe |\n\n## Next\n')
        problems = docs.site_path_problems(kinds={'topic'}, documented=table)
        self.assertEqual(len(problems), 1)
        self.assertIn('/recipes/<id>', problems[0])


class WhatAnAgentIsToldToRead(unittest.TestCase):
    """llms.txt, the AGENTS.md `jadpo new` writes and the agent skill point
    at the site's split pages, never at a whole area page."""

    def write(self, root, llms='', agents='', skill=''):
        (root / 'crates/jadpo-cli/scaffold').mkdir(parents=True)
        (root / 'docs/agent-skill').mkdir(parents=True)
        (root / 'llms.txt').write_text(llms)
        (root / 'crates/jadpo-cli/scaffold/AGENTS.md.tmpl').write_text(agents)
        (root / 'docs/agent-skill/SKILL.md').write_text(skill)

    def problems(self, **files):
        built = [('topic', 'one', 'topics/one.md', 'One', 1, '# One\n', set()),
                 ('type', 'types', 'types/types.md', 'Types', None, '# Types\n', set()),
                 ('type', 'type-money', 'types/type-money.md', 'Money', 19, '# Money\n', set()),
                 ('error', 'a.b', 'errors/a.b.md', 'a.b', 1, '# a.b\n', set())]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write(root, **files)
            return docs.agent_reading_problems(built, root)

    def test_the_repositorys_own_files_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in docs.AGENT_READS:
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text((docs.ROOT / name).read_text())
            found = docs.agent_reading_problems([], root)
        # With no pages built only the addresses are left to fail.
        self.assertEqual([p for p in found if 'whole area page' in p], [])

    def test_split_pages_pass(self):
        self.assertEqual(self.problems(llms='[One](https://jadpo.dev/docs/one.md) https://jadpo.dev/docs/types/type-money.md',
                                       agents='https://jadpo.dev/docs/errors/a.b https://jadpo.dev/docs/types.md',
                                       skill='https://jadpo.dev/docs/primer.md and https://jadpo.dev/docs/index.json'), [])

    def test_a_whole_area_page_is_a_problem_however_it_is_written(self):
        for url in ('https://raw.githubusercontent.com/jadpo-dev/jadpo/main/docs/language/19-standard-library.md',
                    'https://github.com/jadpo-dev/jadpo/blob/main/docs/language/19-standard-library.md#money',
                    'docs/language/19-standard-library.md'):
            problems = self.problems(llms=f'Read [the library]({url}).\n')
            self.assertEqual(len(problems), 1, url)
            self.assertIn('llms.txt', problems[0])
            self.assertIn('whole area page', problems[0])

    def test_a_design_note_isnt_an_area_page(self):
        self.assertEqual(self.problems(llms='docs/language/notes/19-standard-library.md\n'), [])

    def test_a_page_the_site_doesnt_serve_is_a_problem(self):
        problems = self.problems(agents='https://jadpo.dev/docs/gone.md\n')
        self.assertEqual(len(problems), 1)
        self.assertIn('gone', problems[0])

    def test_the_old_topics_folder_is_a_problem(self):
        # /docs/topics/<id> only redirects: the table's path is /docs/<id>.
        problems = self.problems(agents='https://jadpo.dev/docs/topics/one.md\n')
        self.assertEqual(len(problems), 1)
        self.assertIn('topics/one', problems[0])

    def test_a_pattern_in_an_address_is_not_a_page(self):
        self.assertEqual(self.problems(skill='https://jadpo.dev/docs/errors/<rule>.md, https://jadpo.dev/v/<version>/docs/index.json\n'), [])


class WhatAnAgentReadsOnce(unittest.TestCase):
    """The primer as the bundle gives it, and the repository's llms.txt, stay
    inside the budget (agent_guidance.READ_ONCE_TOKENS)."""

    def problems(self, primer, llms):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'llms.txt').write_text(llms)
            return docs.read_once_problems([('primer', 'primer', 'primer.md', 'Primer', None, primer, set())], root)

    def test_inside_the_budget_passes(self):
        self.assertEqual(self.problems('p' * 100, 'l' * 100), [])

    def test_over_it_fails_on_the_page_as_the_bundle_gives_it(self):
        found = self.problems('p' * 40_000, 'l' * 100)
        self.assertEqual(len(found), 1)
        self.assertIn('read once', found[0])


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

    def test_a_new_topic_needs_its_place_in_the_manifest(self):
        problems = self.problems(two=AREA_TWO + '\n## Newer\n<!-- topic: newer -->\n\nNewer still.\n')
        self.assertIn('topic "newer" has no place', ' '.join(problems))

    def test_a_renamed_topic_names_its_new_id(self):
        problems = self.problems(two=AREA_TWO.replace('<!-- topic: twos-part -->',
                                                      '<!-- topic: the-part (was twos-part) -->'))
        self.assertIn('"twos-part" is now "the-part"', ' '.join(problems))

    def test_a_tree_with_no_manifest_says_so(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root, reference=None)
            problems = docs.check_structure(pages, errors, catalogue, COMMANDS, root)
        self.assertIn('docs/reference.toml is missing', ' '.join(problems))

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

GUIDE_INDEX = """# Guides
<!-- topic: guides -->

[Count things](01-count.md).
"""

GUIDE_ONE = """# Count things
<!-- topic: guide-count -->

How to count. See [one](../language/01-one.md) and [the path](../learn/README.md).

`app.jadpo`:

```jadpo check project=guide-count file=app.jadpo
application {}
```
"""

GUIDE_TWO = """# Name things
<!-- topic: guide-name -->

`main.jadpo`:

```jadpo check project=guide-name file=main.jadpo
type Name = String {}
```
"""


def guide_tree(root):
    guides = root / 'docs/guides'
    guides.mkdir(parents=True)
    (guides / 'README.md').write_text(GUIDE_INDEX)
    (guides / '01-count.md').write_text(GUIDE_ONE)
    (guides / '02-name.md').write_text(GUIDE_TWO)
    return docs.load_guides({}, root)


class Guides(unittest.TestCase):
    def test_each_guide_is_a_project_of_its_own(self):
        with tempfile.TemporaryDirectory() as tmp:
            guides = guide_tree(Path(tmp))
            for page in guides:
                page.collect_blocks()
            index, one, two = guides
            self.assertEqual(index.blocks, [])
            self.assertEqual([b['attrs']['file'] for b in one.blocks], ['app.jadpo'])
            # Nothing carries over from the guide before it.
            self.assertEqual([b['attrs']['file'] for b in two.blocks], ['main.jadpo'])
            self.assertEqual([[(t['id'], t['title']) for t in p.topics()] for p in guides],
                             [[('guides', 'Guides')], [('guide-count', 'Count things')],
                              [('guide-name', 'Name things')]])

    def test_the_bundle_holds_the_guides_after_the_learn_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root)
            learn = learn_tree(root)
            guides = guide_tree(root)
            for page in learn + guides:
                page.collect_blocks()
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': []}, root, learn=learn, guides=guides)
            bundle.write(root / 'bundle')
            index = json.loads((root / 'bundle/index.json').read_text())
            kinds = [e['kind'] for e in index['pages']]
            guide_entries = [(e['id'], e['area']) for e in index['pages'] if e['kind'] == 'guide']
            start = (root / 'bundle/guides/guides.md').read_text()
            one = (root / 'bundle/guides/guide-count.md').read_text()
        self.assertEqual(guide_entries, [('guides', None), ('guide-count', None), ('guide-name', None)])
        self.assertEqual(kinds[:6], ['learn'] * 3 + ['guide'] * 3)
        self.assertIn('[Count things](guide-count.md)', start)
        self.assertIn('[one](../topics/one.md)', one)
        self.assertIn('[the path](../learn/learn.md)', one)
        self.assertNotIn('topic:', one)

    def test_a_guide_may_link_another_guide(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            guides = guide_tree(root)
            guides[0].collect_blocks()
            self.assertEqual(docs.user_page_problems(guides[0], root), [])


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
            # Output copied from a run no check can make: its project's
            # check blocks, which all pass, are compiled all the same.
            path.write_text('# `cli.x`\n\n```jadpo check project=p file=app.jadpo hidden\napplication {}\n```\n\n'
                            '```sh\njadpo go\n```\n\n```text\nerror: no\n  rule: cli.x\n```\n')
            page = docs.ErrorPage(path, catalogue)
            page.collect_blocks()
            page.check_rule()
            self.assertEqual(page.problems, [])
            # One that fails must be the rule's.
            path.write_text('# `cli.x`\n\n```jadpo check project=p expect=a.b\nnope\n```\n')
            page = docs.ErrorPage(path, catalogue)
            page.collect_blocks()
            page.check_rule()
            self.assertEqual(page.problems, ['no example for cli.x: add a ```sh run block failing with it'])


FORMATTED = '''# `a.b`

A setting holds something other than `true` or `false`.

## Why Jadpo refuses it

It has two answers.

## What it looks like

```jadpo check project=e expect=a.b
x: "no"
```

```text output
error: no
  next: Write `false` (agent)
  rule: a.b
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it should be off**, write `false`:

```jadpo fragment
    x: false
```

```jadpo check project=e-off hidden
y {
    x: false
}
```

It is off.

## Related

- [Settings](../language/01-a.md#settings)
'''


class ErrorFormat(unittest.TestCase):
    catalogue = {'a.b': {'id': 'a.b', 'area': 3, 'decides': 'agent',
                         'summary': 'A setting holds something other than `true` or `false`'}}

    def problems(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'a.b.md'
            path.write_text(text)
            page = docs.ErrorPage(path, self.catalogue)
            page.collect_blocks()
            return docs.error_format_problems(page)

    def test_a_page_with_every_part_passes(self):
        self.assertEqual(self.problems(FORMATTED), [])

    def test_the_sections_come_in_order(self):
        text = FORMATTED.replace('## Related', '## See also')
        self.assertIn('an error page has', self.problems(text)[0])

    def test_what_happened_is_the_catalogue_summary(self):
        text = FORMATTED.replace('A setting holds', 'The setting holds')
        self.assertIn('catalogue summary', self.problems(text)[0])

    def test_what_happened_is_one_sentence(self):
        text = FORMATTED.replace('`false`.\n', '`false`. It is wrong.\n', 1)
        self.assertIn('one sentence', self.problems(text)[0])

    def test_the_marker_is_explained_in_the_standard_wording(self):
        text = FORMATTED.replace('can make\nthe fix itself', 'may fix it')
        self.assertIn('`(agent)`', self.problems(text)[0])
        human = FORMATTED.replace('(agent)\n', '(human)\n')
        self.assertIn('`(human)`', self.problems(human)[0])

    def test_a_fragment_is_compiled_by_a_hidden_block(self):
        text = FORMATTED.replace('    x: false\n}', '    x: true\n}')
        self.assertIn('hidden check block', self.problems(text)[0])

    def test_each_fix_says_what_it_changes(self):
        text = FORMATTED.replace('It is off.\n', '')
        self.assertIn('what it changes', self.problems(text)[0])

    def test_related_links_topics(self):
        text = FORMATTED.replace('- [Settings](../language/01-a.md#settings)', 'Settings.')
        self.assertIn('links the topics', self.problems(text)[0])

    def test_a_command_error_shows_its_run_block(self):
        text = FORMATTED.replace('```jadpo check project=e expect=a.b\nx: "no"\n```',
                                 '```sh run project=e expect=a.b\njadpo go\n```')
        self.assertEqual(self.problems(text), [])
        text = text.replace('```sh run project=e expect=a.b', '```sh run project=e expect=a.c')
        self.assertIn('expect=a.b', self.problems(text)[0])

    def test_the_scope_is_what_the_user_layer_and_people_reach(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for folder in ('docs/learn', 'docs/guides', 'docs/errors', 'examples/one'):
                (root / folder).mkdir(parents=True)
            (root / 'docs/learn/01-a.md').write_text('See [it](../errors/a.b.md).\n')
            (root / 'examples/one/README.md').write_text('Shows `c.d`.\n')
            command = docs.COMMAND_AREA
            catalogue = {'a.b': {'area': 3, 'decides': 'agent'}, 'c.d': {'area': 3, 'decides': 'agent'},
                         'e.f': {'area': 3, 'decides': 'human'}, 'g.h': {'area': 3, 'decides': 'agent'},
                         'cli.y': {'area': command, 'decides': 'agent'},
                         'seed.failed': {'area': command, 'decides': 'agent'},
                         'acceptance.edited': {'area': command, 'decides': 'agent'},
                         'deployable.empty': {'area': command, 'decides': 'agent'},
                         'install.in_use': {'area': command, 'decides': 'agent'}}
            commands = [{'name': 'check', 'rules': ['acceptance.edited']},
                        {'name': 'seed', 'rules': ['seed.failed']},
                        {'name': 'package', 'rules': ['install.in_use']}]
            # The command line's own rules, and every other rule only a
            # command raises, are in scope, but for the ones waiting for
            # D13b3c; a rule `jadpo check` raises, or no command lists,
            # waits for D13c.
            self.assertEqual(docs.error_format_scope(catalogue, root, commands),
                             {'a.b', 'c.d', 'e.f', 'cli.y', 'seed.failed'})

    def test_the_bundle_gives_the_summary_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'a.b.md'
            path.write_text(FORMATTED)
            page = docs.ErrorPage(path, self.catalogue)
            page.collect_blocks()
            bundle = docs.Bundle([], [page], {'rules': list(self.catalogue.values()), 'commands': []}, Path(tmp))
            text = bundle.error_markdown(page, set())
        self.assertEqual(text.count('something other than'), 1)
        self.assertNotIn('project=e-off', text)


FAKE_JADPO = """#!/bin/sh
case "$1" in
  --version) echo 'jadpo 0.2.0';;
  fail) printf 'error: no\\n  next: Fix it (agent)\\n  rule: cli.x\\n' >&2; exit 1;;
  where) pwd; echo "$HOME"; echo "checked: 1 file in 0.3 s";;
  database) case "$DATABASE_URL" in postgresql://*) echo ok;; *) echo none;; esac;;
  ask) printf '? Enter X  hidden  '; stty -echo; read line; stty echo; printf '\\n'; echo "got $line";;
  *) echo ok;;
esac
"""


class Commands(unittest.TestCase):
    """A ```sh run block: its commands run in a copy of its project, and
    what they print is compared with the output block after it."""
    catalogue = {'cli.x': {'id': 'cli.x', 'area': docs.COMMAND_AREA, 'decides': 'agent', 'summary': 'X'}}

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        fake = Path(self.dir.name) / 'jadpo'
        fake.write_text(FAKE_JADPO)
        fake.chmod(0o755)
        self.saved = docs.JADPO, docs._jadpo_built
        docs.JADPO, docs._jadpo_built = fake, True

    def tearDown(self):
        docs.JADPO, docs._jadpo_built = self.saved
        self.dir.cleanup()

    def page(self, text, bless=False):
        path = Path(self.dir.name) / 'cli.x.md'
        path.write_text(text)
        page = docs.ErrorPage(path, self.catalogue)
        page.collect_blocks()
        page.check_runs(bless)
        return page

    def test_what_it_prints_is_compared(self):
        page = self.page('# `cli.x`\n\n```sh run project=p expect=cli.x\njadpo fail\n```\n\n'
                         '```text output\nerror: no\n  next: Fix it (agent)\n  rule: cli.x\n```\n')
        self.assertEqual(page.problems, [])
        self.assertEqual(page.covered_rules(), {'cli.x'})
        page = self.page('# `cli.x`\n\n```sh run project=p expect=cli.x\njadpo fail\n```\n\n'
                         '```text output\nerror: yes\n```\n')
        self.assertIn('differs', page.problems[0])

    def test_bless_writes_the_output(self):
        page = self.page('# `cli.x`\n\n```sh run project=p\njadpo go\n```\n', bless=True)
        self.assertEqual(page.problems, [])
        page.bless()
        self.assertIn('```sh run project=p\njadpo go\n```\n\n```text output\nok\n```\n', page.path.read_text())

    def test_it_must_fail_with_its_rule_or_pass(self):
        page = self.page('# `cli.x`\n\n```sh run project=p expect=cli.y\njadpo fail\n```\n\n'
                         '```text output\nerror: no\n  next: Fix it (agent)\n  rule: cli.x\n```\n')
        self.assertIn('expected exactly one cli.y', page.problems[0])
        page = self.page('# `cli.x`\n\n```sh run project=p\njadpo fail\n```\n\n'
                         '```text output\nerror: no\n  next: Fix it (agent)\n  rule: cli.x\n```\n')
        self.assertIn('expected to succeed', page.problems[0])

    def test_paths_and_run_times_are_the_projects_own(self):
        page = self.page('# `cli.x`\n\n```sh run project=p\njadpo where\n```\n\n'
                         '```text output\n.\n~\nchecked: 1 file\n```\n')
        self.assertEqual(page.problems, [])

    def test_the_clock_time_a_command_started_is_left_out(self):
        """`jadpo dev` heads its output with the time it started, which no
        two runs share; like a duration, it is left out."""
        self.assertEqual(docs.without_run_time('jadpo dev, started 21:13:00\nerror: x\nchecked: 1 file in 0.1 s\n'),
                         'jadpo dev\nerror: x\nchecked: 1 file\n')

    def test_the_project_is_its_check_blocks(self):
        page = self.page('# `cli.x`\n\n```jadpo check project=p file=app.jadpo hidden\napplication {}\n```\n\n'
                         '```sh run project=p\ncat app.jadpo\n```\n\n```text output\napplication {}\n```\n')
        self.assertEqual(page.problems, [])

    def test_what_is_typed_at_a_terminal(self):
        page = self.page('# `cli.x`\n\n```sh run project=p tty typed=abc\njadpo ask\n```\n\n'
                         '```text output\n? Enter X  hidden\ngot abc\n```\n')
        self.assertEqual(page.problems, [])

    def test_a_database_block_gets_a_database_of_its_own(self):
        """`database` gives the commands DATABASE_URL, a new database on
        the run's server; under JADPO_SKIP_POSTGRES the block is skipped."""
        asked = []

        class Fake:
            def create(self, stem):
                asked.append(stem)
                return f'postgresql://jadpo@/docs_{stem}_{len(asked)}?host=/tmp'

        saved = docs.DATABASES, docs.SKIP_POSTGRES
        try:
            docs.DATABASES, docs.SKIP_POSTGRES = Fake(), False
            page = self.page('# `cli.x`\n\n```sh run project=p database\njadpo database\n```\n\n'
                             '```text output\nok\n```\n')
            self.assertEqual(page.problems, [])
            self.assertEqual(asked, ['p'])
            # Without `database`, there is none.
            page = self.page('# `cli.x`\n\n```sh run project=q\njadpo database\n```\n\n'
                             '```text output\nnone\n```\n')
            self.assertEqual(page.problems, [])
            self.assertEqual(asked, ['p'])
            docs.SKIP_POSTGRES = True
            page = self.page('# `cli.x`\n\n```sh run project=p database\njadpo database\n```\n\n'
                             '```text output\nok\n```\n')
            self.assertEqual(page.problems, [])
            self.assertEqual(page.skipped_runs, ['p'])
            self.assertEqual(asked, ['p'])
        finally:
            docs.DATABASES, docs.SKIP_POSTGRES = saved

    def test_root_refuses_blocks_that_take_permissions_away(self):
        """Root reads and writes past any permission, so a block whose
        error comes from `chmod` would pass wrongly: it isn't run."""
        text = ('# `cli.x`\n\n```sh run project=p expect=cli.x\nchmod a-w .\njadpo fail\n```\n\n'
                '```text output\nerror: no\n  next: Fix it (agent)\n  rule: cli.x\n```\n')
        saved = docs.os.geteuid
        try:
            docs.os.geteuid = lambda: 0
            page = self.page(text)
            self.assertEqual(len(page.problems), 1)
            self.assertIn('as root', page.problems[0])
            self.assertIn('ordinary user', page.problems[0])
            # A block without `chmod` still runs as root.
            page = self.page('# `cli.x`\n\n```sh run project=p\njadpo go\n```\n\n```text output\nok\n```\n')
            self.assertEqual(page.problems, [])
        finally:
            docs.os.geteuid = saved
        self.assertEqual(self.page(text).problems, [])


MONEY = {'name': 'Money', 'group': 'Money and banking', 'summary': 'An amount of money (area 19).'}
ADDRESS = {'name': 'PostalAddress', 'group': 'People and contact', 'summary': 'A postal address (area 19).'}
TYPE_PAGES = {
    'index': '# Standard types\n\nEvery one.\n\n| Type | Group | What it is |\n|---|---|---|\n'
             '| [`Money`](type-money.md) | Money and banking | An amount of money. |\n'
             '| [`PostalAddress`](type-postal-address.md) | People and contact | A postal address. |\n',
    'types': [
        {'name': 'Money', 'id': 'type-money', 'group': 'Money and banking', 'meaning': 'An amount of money.',
         'example': 'money.from_minor(1050, Currency.gbp)',
         'page': '# `Money`\n\nAn amount of money.\n\nSee [`PostalAddress`](type-postal-address.md).\n'},
        {'name': 'PostalAddress', 'id': 'type-postal-address', 'group': 'People and contact',
         'meaning': 'A postal address.', 'example': 'PostalAddress { }',
         'page': '# `PostalAddress`\n\nA postal address.\n'},
    ],
}


class TypeReference(unittest.TestCase):
    def build(self, type_pages=TYPE_PAGES):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalogue, pages, errors = tree(root)
            bundle = docs.Bundle(pages, errors, {'version': '0.1.0', 'rules': list(catalogue.values()),
                                                 'commands': COMMANDS, 'types': [MONEY, ADDRESS], 'operations': [],
                                                 'type_pages': type_pages}, root)
            bundle.write(root / 'bundle')
            files = {str(p.relative_to(root / 'bundle')): p.read_text()
                     for p in (root / 'bundle').rglob('*') if p.is_file()}
            self.nav = json.loads(files.pop('nav.json')) if 'nav.json' in files else None
            return json.loads(files.pop('index.json')), files, bundle

    def test_a_type_id_is_its_name_in_kebab_case(self):
        self.assertEqual(docs.type_id('UkSortCode'), 'type-uk-sort-code')
        self.assertEqual(docs.type_id('H3Cell'), 'type-h3-cell')
        self.assertEqual(docs.type_id('Ean13'), 'type-ean13')
        self.assertEqual(docs.type_id('Money'), 'type-money')

    def test_the_types_follow_the_topics_index_first_then_a_to_z(self):
        index, files, bundle = self.build()
        kinds = [(e['kind'], e['id']) for e in index['pages']]
        self.assertEqual(kinds[kinds.index(('type', 'types')) - 1][0], 'topic')
        self.assertEqual([k for k in kinds if k[0] == 'type'],
                         [('type', 'types'), ('type', 'type-money'), ('type', 'type-postal-address')])
        self.assertEqual(kinds[kinds.index(('type', 'type-postal-address')) + 1][0], 'error')
        self.assertIn('types/type-money.md', files)
        by_id = {e['id']: e for e in index['pages']}
        self.assertEqual(by_id['type-money']['title'], 'Money')
        self.assertEqual(by_id['type-money']['area'], 19)
        self.assertEqual(by_id['type-money']['summary'], 'An amount of money.')
        self.assertEqual(by_id['type-money']['links'], ['type-postal-address'])
        self.assertEqual(by_id['types']['links'], ['type-money', 'type-postal-address'])
        self.assertEqual(bundle.problems, [])

    def test_a_page_with_another_id_than_ours_is_a_problem(self):
        pages = {'index': '# Types\n', 'types': [dict(TYPE_PAGES['types'][0], id='type-cash')]}
        _, _, bundle = self.build(pages)
        self.assertTrue(any('type-cash' in p for p in bundle.problems), bundle.problems)

    def test_a_type_name_in_code_links_its_page(self):
        linked = set()
        names = {'Money', 'Range'}
        text = ('Use `Money` and `Range<T>`, not `Other`.\n\n## `Money` stays a heading\n\n'
                '```jadpo\n`Money`\n```\n\nAlready [`Money`](x.md).\n')
        out = docs.link_type_names(text, names, linked)
        self.assertIn('Use [`Money`](../types/type-money.md) and [`Range<T>`](../types/type-range.md), not `Other`.', out)
        self.assertIn('## `Money` stays a heading', out)
        self.assertIn('```jadpo\n`Money`\n```', out)
        self.assertIn('Already [`Money`](x.md).', out)
        self.assertEqual(linked, {'type-money', 'type-range'})
        self.assertIn('[`Money`](types/type-money.md)',
                      docs.link_type_names('`Money`', names, set(), 'types/'))

    def test_a_page_keeps_to_plain_words(self):
        fine = {'name': 'A', 'page': 'See [this](../topics/one-contract.md), `LawfulBasis.contract`.'}
        self.assertEqual(docs.type_page_word_problems([fine]), [])
        bad = {'name': 'B', 'page': 'It is not ordered, under the baseline contract.'}
        self.assertEqual(len(docs.type_page_word_problems([bad])), 3)

    def test_an_example_is_a_value_of_its_type_or_its_own_statements(self):
        self.assertEqual(docs.type_example_test('Email', 'Email("a@example.com")'),
                         'test "Email" {\n    var example: Email = Email("a@example.com")\n'
                         '    assert example == example\n}\n')
        self.assertEqual(docs.type_example_test('Range', 'var a: Range<Year> = b\nassert a == a'),
                         'test "Range" {\n    var a: Range<Year> = b\n    assert a == a\n}\n')
        self.assertEqual(docs.split_type_example('type A = Object {\n    b: Text\n}\n\nvar c: A = d'),
                         (['type A = Object {\n    b: Text\n}'], 'var c: A = d'))

    def test_the_group_tables_must_match_the_compiler(self):
        def page(area, count=2):
            return SimpleNamespace(area=lambda: area, path=Path(f'{area}.md'), body=(
                f'There are {count} standard types, listed here.\n\n'
                '| Group | Types | Extras in |\n|---|---|---|\n'
                '| People and contact (1) | **`PostalAddress`** | `web` |\n'
                '| Money and banking (1) | **`Money`** (enum) | `Money` |\n'))
        self.assertEqual(docs.type_table_problems([page(19)], [ADDRESS, MONEY]), [])
        self.assertEqual(docs.type_table_problems([page(2)], [ADDRESS, MONEY]), [])
        wrong_order = docs.type_table_problems([page(19)], [MONEY, ADDRESS])
        self.assertEqual(len(wrong_order), 1, wrong_order)
        moved = docs.type_table_problems([page(19)], [ADDRESS, dict(MONEY, group='People and contact')])
        self.assertTrue(any('`Money` is listed under' in p for p in moved), moved)
        count = docs.type_table_problems([page(19, count=247)], [ADDRESS, MONEY])
        self.assertTrue(any('There are 247' in p and '2' in p for p in count), count)
        left_out = docs.type_table_problems([page(19)], [ADDRESS, MONEY, dict(MONEY, name='Iban')])
        self.assertTrue(any('leaves out `Iban`' in p for p in left_out), left_out)
        self.assertTrue(any('should say how many' in p for p in left_out), left_out)
        extra = docs.type_table_problems([page(19)], [ADDRESS])
        self.assertTrue(any('`Money`, which isn\'t a standard type' in p for p in extra), extra)


class ReleaseLine(unittest.TestCase):
    """No page names the running release (docs/writing.md, "The release
    line"), so moving the version on breaks none."""

    def setUp(self):
        self.running, docs._running = docs._running, ('0.3', '0.3.0')

    def tearDown(self):
        docs._running = self.running

    def test_output_shows_the_running_line_and_release_as_placeholders(self):
        out = 'Created todo-app\n    jadpo  0.3\n    route  POST /hello\njadpo 0.3.0\n2 |     jadpo: "0.3"\n'
        self.assertEqual(docs.normalised(out),
                         'Created todo-app\n    jadpo  X.Y\n    route  POST /hello\njadpo X.Y.Z\n'
                         '2 |     jadpo: "X.Y"\n')

    def test_other_lines_and_numbers_are_left_as_they_are(self):
        out = '    jadpo  0.2\nThis project needs Jadpo 99.0\n    jadpo: "0.0"\ntotal * 0.3\n'
        self.assertEqual(docs.normalised(out), out)

    def test_a_check_block_compiles_the_placeholder_as_the_running_line(self):
        self.assertEqual(docs.with_release_line('application {\n    jadpo: "X.Y"\n}\n'),
                         'application {\n    jadpo: "0.3"\n}\n')
        self.assertEqual(docs.with_release_line('application {}\n'), 'application {}\n')

    def test_a_page_naming_the_running_line_or_release_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            page = root / 'page.md'
            page.write_text('as `application { jadpo: "0.3" }`\n```text\n    jadpo  0.3\njadpo 0.3.0\n```\n'
                            'Fetching Jadpo 0.3.2, `jadpo: "X.Y"`, `jadpo: "99.0"`\n')
            problems = docs.check_release_line([page], '0.3', '0.3.0', root)
        self.assertEqual([p.split(': ')[0] for p in problems], ['page.md:1', 'page.md:3', 'page.md:4'], problems)
        self.assertIn('write `as `application { jadpo: "X.Y" }``', problems[0])


if __name__ == '__main__':
    unittest.main()
