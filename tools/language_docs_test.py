#!/usr/bin/env python3
"""Tests for the docs renderer in tools/language_docs.py.

The gate's `language_docs.py --check` run renders every page too; these
need no `jadpo` and render the Markdown alone, each under the renderer's
time limit, so a parser that stops moving fails rather than hangs.

  python3 tools/language_docs_test.py
"""

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


class Pages(unittest.TestCase):
    def test_every_language_page_renders(self):
        catalogue = docs.load_catalogue(docs.load_frozen())
        pages = [docs.Page(p, catalogue, docs.load_frozen())
                 for p in sorted(docs.DOCS.glob('[0-9][0-9]-*.md'))]
        self.assertTrue(pages)
        for page in pages:
            page.collect_blocks()
        with tempfile.TemporaryDirectory() as out:
            docs.write_site(pages, Path(out))
            self.assertEqual(len(list(Path(out).glob('*.html'))), len(pages) + 1)


if __name__ == '__main__':
    unittest.main()
