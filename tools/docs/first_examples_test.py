#!/usr/bin/env python3
"""Tests for tools/docs/first_examples.py. Each builds a small docs folder and
lets the real compiler judge it, since the compiler's errors are what the check
reads.

  python3 tools/docs/first_examples_test.py
"""

import contextlib
import io
import tempfile
import unittest
from pathlib import Path

import first_examples as check

MANIFEST = """
[[group]]
title = "Things"

[[group.page]]
title = "One"
topics = {topics}
"""

AREA_HEAD = '---\ntitle: Things\narea: 1\n---\n\n# Things\n<!-- topic: things -->\n\n'


def block(attrs, source):
    return f'```jadpo {attrs}\n{source}\n```\n\n'


def topic(name, *blocks):
    return f'## {name}\n<!-- topic: {name.lower().replace(" ", "-")} -->\n\nWords.\n\n' + ''.join(blocks)


COUNT = 'type Count = Int {\n    min: 0\n}\n'
USES_COUNT = '/// Doubles it.\nfunction double(amount: Count) -> Count {\n    return Count(amount * 2)\n}\n'
EMPTY_APP = 'application {}\n'


class FirstExamples(unittest.TestCase):
    def judge(self, area='', topics='["first"]', guide=None, every_topic=False, status=''):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs/language').mkdir(parents=True)
            (root / 'docs/guides').mkdir(parents=True)
            (root / 'docs/reference.toml').write_text(MANIFEST.format(topics=topics) + status)
            (root / 'docs/language/01-things.md').write_text(AREA_HEAD + area)
            if guide:
                (root / 'docs/guides/01-a-job.md').write_text(guide)
            entries, counts = check.judged(root, every_topic)
            return entries, counts

    def one(self, *blocks):
        entries, _ = self.judge(topic('First', *blocks))
        self.assertEqual(len(entries), 1)
        return entries[0]

    def test_everything_on_the_page_stands(self):
        entry = self.one(block('check project=a', COUNT + '\n' + USES_COUNT))
        self.assertEqual(entry['verdict'], check.STANDS)

    def test_a_type_only_in_a_hidden_file_fails_and_names_the_file(self):
        entry = self.one(block('check project=a file=types.jadpo hidden', COUNT),
                         block('check project=a file=main.jadpo', USES_COUNT))
        self.assertEqual(entry['verdict'], check.NEEDS)
        self.assertEqual([(n['file'], n['hidden']) for n in entry['needed']], [('types.jadpo', True)])
        self.assertEqual(entry['fix'], check.SMALL_BLOCK)

    def test_a_hidden_empty_application_is_not_something_it_needs(self):
        entry = self.one(block('check project=a file=app.jadpo hidden', EMPTY_APP),
                         block('check project=a file=domains/d/types/count.jadpo', COUNT))
        self.assertEqual(entry['verdict'], check.STANDS)

    def test_a_hidden_file_it_doesnt_use_is_no_failure(self):
        entry = self.one(block('check project=a file=other.jadpo hidden', 'type Other = Int {}\n'),
                         block('check project=a file=main.jadpo', COUNT))
        self.assertEqual(entry['verdict'], check.STANDS)

    def test_hidden_configuration_it_needs_fails(self):
        app = 'application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n' \
              '            url: config.database_url\n        }\n    }\n}\n'
        config = 'config {\n    database_url: PostgresUrl\n}\n'
        entity = '/// A thing.\nentity Thing {\n    id: Uuid\n\n    policy: {\n        read: [Access.public]\n    }\n}\n'
        entry = self.one(block('check project=a file=app.jadpo', app),
                         block('check project=a file=config.jadpo hidden', config),
                         block('check project=a file=domains/d/entities/thing.jadpo', entity))
        self.assertEqual(entry['verdict'], check.NEEDS)
        self.assertIn('config.jadpo', [n['file'] for n in entry['needed']])

    def test_a_type_shown_only_on_another_topic_fails(self):
        area = (topic('First', block('check project=a file=main.jadpo', USES_COUNT))
                + topic('Second', block('check project=a file=types.jadpo', COUNT)))
        entries, _ = self.judge(area, '["first", "second"]')
        self.assertEqual(entries[0]['verdict'], check.NEEDS)
        self.assertEqual([(n['file'], n['hidden']) for n in entries[0]['needed']], [('types.jadpo', False)])

    def test_a_type_shown_further_down_the_page_stands(self):
        entry = self.one(block('check project=a file=main.jadpo', USES_COUNT),
                         block('check project=a file=types.jadpo', COUNT))
        self.assertEqual(entry['verdict'], check.STANDS)

    def test_a_first_block_that_only_sets_up_doesnt_hide_the_example_after_it(self):
        # `Thing` compiles alone. The function is what the example is for.
        entry = self.one(block('check project=a file=setup.jadpo', 'type Thing = Int {}\n'),
                         block('check project=a file=types.jadpo hidden', COUNT),
                         block('check project=a file=main.jadpo', USES_COUNT))
        self.assertEqual(entry['example'], ['setup.jadpo', 'main.jadpo'])
        self.assertEqual(entry['verdict'], check.NEEDS)

    def test_a_later_step_may_lean_on_a_hidden_file(self):
        # The function ends the first example, so what the later one needs is
        # no failure of the first.
        entry = self.one(block('check project=a file=first.jadpo', COUNT + '\n' + USES_COUNT),
                         block('check project=a file=extra.jadpo hidden', 'type Extra = Int {}\n'),
                         block('check project=a file=later.jadpo',
                               '/// Keeps it.\nfunction keep(amount: Extra) -> Extra {\n    return amount\n}\n'))
        self.assertEqual(entry['verdict'], check.STANDS)

    def test_a_later_copy_of_a_file_is_a_later_step(self):
        entry = self.one(block('check project=a file=main.jadpo', COUNT + '\n' + USES_COUNT),
                         block('check project=a file=main.jadpo', 'type Broken = Missing {}\n'))
        self.assertEqual(entry['verdict'], check.STANDS)

    def test_a_fragment_fails_and_one_that_compiles_is_a_one_line_fix(self):
        entry = self.one(block('fragment', COUNT))
        self.assertEqual((entry['verdict'], entry['fix']), (check.FRAGMENT, check.ONE_LINE))
        entry = self.one(block('fragment', USES_COUNT))
        self.assertEqual((entry['verdict'], entry['fix']), (check.FRAGMENT, check.SMALL_BLOCK))

    def test_a_fragment_on_a_topic_that_isnt_built_is_its_design(self):
        built = '## What is built\n<!-- topic: things-built -->\n\n| Feature | Status |\n|---|---|\n| One | Not built yet. |\n'
        entries, _ = self.judge(topic('First', block('fragment', USES_COUNT)) + built, status='status = ["01-things: One"]\n')
        self.assertEqual((entries[0]['verdict'], entries[0]['fix']), (check.PLANNED, None))

    def test_an_example_of_an_error_isnt_a_working_use(self):
        entry = self.one(block('check project=a expect=type.invalid_literal', 'type A = Nope {}\n'))
        self.assertEqual(entry['verdict'], check.FAILURE)

    def test_the_first_topic_with_an_example_is_the_pages(self):
        area = topic('First') + topic('Second', block('check project=a', COUNT))
        entries, counts = self.judge(area, '["first", "second"]')
        self.assertEqual([entry['topic'] for entry in entries], ['second'])
        self.assertEqual(counts, {'Things': 1})

    def test_a_page_with_no_example_is_counted_and_not_judged(self):
        entries, counts = self.judge(topic('First'))
        self.assertEqual((entries, counts), ([], {'Things': 1}))

    def test_a_page_with_every_topic_judges_each(self):
        area = (topic('First', block('check project=a', COUNT))
                + topic('Second', block('check project=b', USES_COUNT)))
        entries, _ = self.judge(area, '["first", "second"]', every_topic=True)
        self.assertEqual([entry['verdict'] for entry in entries], [check.STANDS, check.NEEDS])

    def test_a_guide_is_judged(self):
        guide = ('# A job\n<!-- topic: guide-a-job -->\n\nWords.\n\n'
                 + block('check project=g file=app.jadpo hidden', EMPTY_APP)
                 + block('check project=g file=types.jadpo hidden', COUNT)
                 + block('check project=g file=main.jadpo', USES_COUNT))
        entries, counts = self.judge(topic('First'), guide=guide)
        self.assertEqual([(entry['group'], entry['verdict']) for entry in entries], [(check.GUIDES, check.NEEDS)])
        self.assertEqual(counts[check.GUIDES], 1)

    def test_it_reports_without_failing_until_strict(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs/language').mkdir(parents=True)
            (root / 'docs/guides').mkdir(parents=True)
            (root / 'docs/reference.toml').write_text(MANIFEST.format(topics='["first"]'))
            (root / 'docs/language/01-things.md').write_text(AREA_HEAD + topic('First', block('fragment', USES_COUNT)))
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(check.main(['--markdown', str(root / 'list.md')], root), 0)
                self.assertEqual(check.main(['--strict'], root), 1)
            self.assertIn('first example fails: Things › One', out.getvalue())
            self.assertIn('first examples: 1 of 1 fail', out.getvalue())
            self.assertIn('## Fail (1)', (root / 'list.md').read_text())

    def test_a_fix_is_sized_by_the_lines_it_shows(self):
        self.assertEqual(check.fix_size(1), check.ONE_LINE)
        self.assertEqual(check.fix_size(check.SMALL_BLOCK_LINES), check.SMALL_BLOCK)
        self.assertEqual(check.fix_size(check.SMALL_BLOCK_LINES + 1), check.REWRITE)


if __name__ == '__main__':
    unittest.main()
