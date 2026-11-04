#!/usr/bin/env python3
"""Tests for the reference's manifest in tools/docs/reference_manifest.py.

The tree under a real checkout is held by language_docs_test.py; these are
the check's failures and the shape of what the manifest gives.

  python3 tools/docs/reference_manifest_test.py
"""

import tempfile
import tomllib
import unittest
from pathlib import Path

import reference_manifest as manifest

SMALL = """
[[group]]
id = "language"
title = "Language"
summary = "How a program is written."

[[group.page]]
title = "Logic"
topics = ["logic", "loops"]
details = ["recursion"]
built = ["logic-built"]

[[group.page.section]]
id = "types"
title = "Standard types"
summary = "Every standard type."

[[group.page.section.section]]
id = "types-money"
title = "Money"
summary = "What money is."
types = "Money"
topics = ["money-and-payments"]

[[group]]
id = "services"
title = "Services"
topics = ["how-services-work"]

[[group.page]]
title = "Email"
status = ["16-services: Email", "16-services: Mail packages"]
topics = ["email", "email-sending"]

[[group.page]]
title = "Files"
status = ["16-services: Files"]
missing = "uploads and downloads"
topics = ["files", "reading-gzip"]

[[group.page]]
title = "Documents"
status = ["16-services: Documents"]
topics = ["documents", "document-privacy"]

[[group]]
id = "tools"
title = "Tools"
summary = "The command."

[[group.page]]
id = "commands"
title = "Commands"
summary = "Every command."
commands = ["check", "test"]
topics = ["the-command"]

[[group]]
id = "errors"
title = "Errors"
summary = "What each error means."
href = "/docs/errors"
topics = ["error-messages"]

[topic_status]
logic = ["03-logic: Logging"]
"""

ROWS = {
    '03-logic': [('Logging', 'Not built yet.')],
    '16-services': [('Email', 'Built: all of it.'), ('Mail packages', 'Built.'),
                    ('Files', 'Partly built. Built: `put`.'), ('Documents', 'Not built yet.')],
}

TOPICS = ['logic', 'loops', 'recursion', 'logic-built', 'money-and-payments', 'how-services-work', 'email',
          'email-sending', 'files', 'reading-gzip', 'documents', 'document-privacy', 'the-command', 'error-messages',
          'services']

TYPE_GROUPS = [('Money', ['type-money', 'type-iban'])]


def page(page_id, kind='topic', title=None):
    default = 'What is built: Logic' if page_id == 'logic-built' else page_id.replace('-', ' ').capitalize()
    return {'id': page_id, 'kind': kind, 'title': title or default, 'summary': f'About {page_id}.'}


def pages(topics=TOPICS):
    every = [page(t) for t in topics]
    every += [page('types', 'type', 'Standard types'), page('type-money', 'type', 'Money'),
              page('type-iban', 'type', 'Iban'), page('jadpo-check', 'command', 'jadpo check'),
              page('jadpo-test', 'command', 'jadpo test'), page('a.b', 'error', 'a.b')]
    return {entry['id']: entry for entry in every}


def href(kind, page_id):
    return {'topic': f'/docs/{page_id}', 'command': f'/docs/{page_id}',
            'type': '/docs/types' if page_id == 'types' else f'/docs/types/{page_id}'}[kind]


def reference(text=SMALL, rows=ROWS):
    return manifest.Reference(tomllib.loads(text), rows)


def problems(text=SMALL, rows=ROWS, every=None, aliases=None, type_groups=TYPE_GROUPS):
    return reference(text, rows).problems(pages() if every is None else every, aliases or {}, type_groups)


class TheCheck(unittest.TestCase):
    def test_a_manifest_in_shape_has_no_problems(self):
        self.assertEqual(problems(), [])

    def test_a_topic_in_no_place_is_a_problem(self):
        found = problems(every=pages(TOPICS + ['a-new-topic']))
        self.assertEqual(len(found), 1, found)
        self.assertIn('"a-new-topic"', found[0])
        self.assertIn('no place', found[0])

    def test_a_topic_in_two_places_is_a_problem_naming_both(self):
        found = problems(SMALL.replace('topics = ["logic", "loops"]', 'topics = ["logic", "loops", "email"]'))
        self.assertEqual(len(found), 1, found)
        self.assertIn('"email"', found[0])
        self.assertIn('Language › Logic', found[0])
        self.assertIn('Services › Email', found[0])

    def test_a_topic_in_details_and_topics_is_in_two_places(self):
        found = problems(SMALL.replace('details = ["recursion"]', 'details = ["recursion", "loops"]'))
        self.assertIn('"loops"', ' '.join(found))

    def test_an_id_no_topic_has_is_a_problem(self):
        found = problems(SMALL.replace('"loops"', '"loops", "no-such-topic"'))
        self.assertEqual(len(found), 1, found)
        self.assertIn('"no-such-topic"', found[0])

    def test_a_renamed_topics_old_id_says_the_new_one(self):
        found = problems(SMALL.replace('"loops"', '"old-loops"'), aliases={'old-loops': 'loops'},
                         every=pages())
        self.assertEqual(len(found), 1, found)
        self.assertIn('"old-loops"', found[0])
        self.assertIn('now "loops"', found[0])

    def test_a_page_of_another_kind_cant_stand_as_a_topic(self):
        found = problems(SMALL.replace('"loops"', '"loops", "type-money"'))
        self.assertTrue(any('"type-money"' in p for p in found), found)

    def test_a_status_row_thats_gone_is_a_problem(self):
        found = problems(SMALL.replace('16-services: Files', '16-services: Uploads'))
        self.assertEqual(len(found), 1, found)
        self.assertIn('"16-services: Uploads"', found[0])
        self.assertIn('no row', found[0])

    def test_a_status_row_in_an_area_with_no_table_is_a_problem(self):
        found = problems(SMALL.replace('16-services: Files', '99-nowhere: Files'))
        self.assertIn('"99-nowhere: Files"', ' '.join(found))

    def test_a_status_row_needs_an_area_and_a_feature(self):
        found = problems(SMALL.replace('16-services: Files', 'Files'))
        self.assertIn('"Files"', ' '.join(found))

    def test_two_rows_starting_the_same_are_a_problem(self):
        rows = dict(ROWS, **{'16-services': ROWS['16-services'] + [('Files on disk', 'Built.'), ('Files in S3', 'Built.')]})
        self.assertEqual(problems(rows=rows), [])
        found = problems(SMALL.replace('16-services: Files', '16-services: File'), rows=rows)
        self.assertIn('more than one row', ' '.join(found))

    def test_a_row_with_status_words_nobody_reads_is_a_problem(self):
        rows = dict(ROWS, **{'16-services': [('Email', 'Mostly.')] + ROWS['16-services'][1:]})
        found = problems(rows=rows)
        self.assertEqual(len(found), 1, found)
        self.assertIn('"Mostly."', found[0])

    def test_a_partly_built_page_says_whats_missing(self):
        found = problems(SMALL.replace('missing = "uploads and downloads"\n', ''))
        self.assertEqual(len(found), 1, found)
        self.assertIn('Services › Files', found[0])
        self.assertIn('missing', found[0])

    def test_a_row_given_a_topic_needs_a_placed_topic(self):
        found = problems(SMALL + 'no-such-topic = ["03-logic: Logging"]\n')
        self.assertEqual(len(found), 1, found)
        self.assertIn('"no-such-topic"', found[0])

    def test_a_row_given_a_topic_is_checked_too(self):
        found = problems(SMALL.replace('03-logic: Logging', '03-logic: Printing'))
        self.assertIn('"03-logic: Printing"', ' '.join(found))

    def test_a_compiler_group_with_no_place_is_a_problem(self):
        found = problems(type_groups=TYPE_GROUPS + [('Health', ['type-nhs-number'])])
        self.assertEqual(len(found), 1, found)
        self.assertIn('"Health"', found[0])

    def test_a_group_the_compiler_doesnt_have_is_a_problem(self):
        found = problems(type_groups=[])
        self.assertIn('"Money"', ' '.join(found))

    def test_a_compiler_group_placed_twice_is_a_problem(self):
        found = problems(SMALL.replace('title = "Standard types"',
                                       'title = "Standard types"\ntypes = "Money"'))
        self.assertIn('"Money"', ' '.join(found))

    def test_with_no_compiler_data_type_groups_arent_compared(self):
        self.assertEqual(problems(type_groups=None), [])

    def test_every_command_has_one_place(self):
        found = problems(SMALL.replace('commands = ["check", "test"]\n', 'commands = ["check"]\n'))
        self.assertEqual(len(found), 1, found)
        self.assertIn('`jadpo test` has no place', found[0])
        twice = problems(SMALL.replace('title = "Logic"', 'title = "Logic"\ncommands = ["check"]'))
        self.assertTrue(any('`jadpo check` is in more than one place' in p and 'Language › Logic' in p for p in twice), twice)

    def test_a_command_there_is_no_page_for_is_a_problem(self):
        found = problems(SMALL.replace('commands = ["check", "test"]', 'commands = ["check", "test", "nope"]'))
        self.assertEqual(len(found), 1, found)
        self.assertIn('no command `jadpo nope`', found[0])

    def test_commands_are_listed_by_name(self):
        found = problems(SMALL.replace('commands = ["check", "test"]', 'commands = true'))
        self.assertTrue(any('lists commands by name' in p for p in found), found)

    def test_a_landing_cannot_take_an_old_id(self):
        found = problems(SMALL.replace('id = "commands"', 'id = "old-commands"'), aliases={'old-commands': 'the-command'})
        self.assertTrue(any('old ID of "the-command"' in p for p in found), found)

    def test_a_topic_summary_names_a_placed_topic(self):
        found = problems(SMALL + '\n[topic_summary]\nnope = "A line."\n')
        self.assertEqual(len(found), 1, found)
        self.assertIn('"nope" is no topic placed here', found[0])

    def test_a_landing_to_be_made_says_what_it_holds(self):
        found = problems(SMALL.replace('summary = "How a program is written."\n', ''))
        self.assertEqual(len(found), 1, found)
        self.assertIn('Language', found[0])
        self.assertIn('summary', found[0])

    def test_a_landing_that_is_a_topic_needs_no_summary(self):
        self.assertNotIn('summary', SMALL.split('id = "services"')[1].split('[[group.page]]')[0])
        self.assertEqual(problems(), [])

    def test_a_landing_is_a_topic_or_a_new_page_not_another_kinds(self):
        found = problems(SMALL.replace('id = "tools"', 'id = "jadpo-check"'))
        self.assertIn('"jadpo-check"', ' '.join(found))

    def test_two_nodes_with_one_id_are_a_problem(self):
        found = problems(SMALL.replace('id = "tools"', 'id = "language"'))
        self.assertIn('"language"', ' '.join(found))

    def test_an_id_is_kebab_case(self):
        found = problems(SMALL.replace('id = "tools"', 'id = "Tools"'))
        self.assertIn('"Tools"', ' '.join(found))

    def test_a_key_nothing_reads_is_a_problem(self):
        found = problems(SMALL.replace('title = "Files"', 'title = "Files"\ndetail = ["x"]'))
        self.assertEqual(len(found), 1, found)
        self.assertIn('"detail"', found[0])
        self.assertIn('Services › Files', found[0])

    def test_a_node_with_no_title_is_a_problem(self):
        found = problems(SMALL.replace('title = "Files"\n', ''))
        self.assertIn('title', ' '.join(found))

    def test_a_node_that_places_nothing_is_a_problem(self):
        found = problems(SMALL + '\n[[group]]\ntitle = "Empty"\n')
        found = [p for p in found if 'Empty' in p]
        self.assertEqual(len(found), 1, found)

    def test_only_a_page_the_site_writes_itself_takes_an_href(self):
        found = problems(SMALL.replace('href = "/docs/errors"', 'href = "/docs/elsewhere"'))
        self.assertIn('/docs/elsewhere', ' '.join(found))

    def test_what_is_built_stays_out_of_the_sidebar(self):
        moved = SMALL.replace('topics = ["logic", "loops"]', 'topics = ["logic", "loops", "logic-built"]')
        moved = moved.replace('built = ["logic-built"]\n', '')
        found = problems(moved)
        self.assertEqual(len(found), 1, found)
        self.assertIn('"logic-built"', found[0])
        self.assertIn('built', found[0])
        wrong = problems(SMALL.replace('built = ["logic-built"]', 'built = ["logic-built", "loops"]')
                         .replace('topics = ["logic", "loops"]', 'topics = ["logic"]'))
        self.assertIn('"loops"', ' '.join(wrong))


class ATopicWithNoId(unittest.TestCase):
    """The docs check reports a topic with no ID; a node with no `id` isn't it."""

    def setUp(self):
        self.every = pages() | {None: page('no-id') | {'id': None}}

    def test_it_is_left_to_the_check_on_ids(self):
        self.assertEqual(problems(every=self.every), [])

    def test_a_node_without_an_id_doesnt_open_it(self):
        self.assertEqual(reference().places(self.every, TYPE_GROUPS)['email']['status'], 'built')
        logic = reference().nav(self.every, TYPE_GROUPS, href, '0.3.0')['groups'][0]['children'][0]
        self.assertEqual(logic['href'], '/docs/logic')


class ThePlaces(unittest.TestCase):
    """What index.json takes: `group`, `page`, `status` for each placed page."""

    def setUp(self):
        self.places = reference().places(pages(), TYPE_GROUPS)

    def test_a_topic_has_its_group_and_page(self):
        self.assertEqual(self.places['loops'], {'group': 'Language', 'page': 'Logic', 'status': None})

    def test_a_topic_directly_under_a_group_has_no_page(self):
        self.assertEqual(self.places['how-services-work'], {'group': 'Services', 'page': None, 'status': None})
        self.assertEqual(self.places['services'], {'group': 'Services', 'page': None, 'status': None})

    def test_a_section_is_under_its_page(self):
        self.assertEqual(self.places['money-and-payments']['page'], 'Logic')

    def test_a_topics_own_row_is_its_status(self):
        self.assertEqual(self.places['logic']['status'], 'not built')

    def test_a_pages_first_topic_stands_for_the_page(self):
        self.assertEqual(self.places['email']['status'], 'built')
        self.assertEqual(self.places['files']['status'], 'partly built')

    def test_the_rest_of_a_partly_built_page_says_nothing(self):
        self.assertEqual(self.places['email-sending']['status'], None)
        self.assertEqual(self.places['reading-gzip']['status'], None)

    def test_nothing_on_a_page_not_built_is_built(self):
        self.assertEqual(self.places['documents']['status'], 'not built')
        self.assertEqual(self.places['document-privacy']['status'], 'not built')

    def test_generated_pages_have_their_place(self):
        self.assertEqual(self.places['type-iban'], {'group': 'Language', 'page': 'Logic', 'status': None})
        self.assertEqual(self.places['types'], {'group': 'Language', 'page': 'Logic', 'status': None})
        self.assertEqual(self.places['jadpo-test'], {'group': 'Tools', 'page': 'Commands', 'status': None})

    def test_a_page_not_wholly_built_says_which_area_says_so(self):
        unfinished = reference().unfinished(pages(), TYPE_GROUPS)
        self.assertEqual(unfinished['files'], {'status': 'partly built', 'missing': 'uploads and downloads',
                                               'area': '16-services'})
        self.assertEqual(unfinished['documents'], {'status': 'not built', 'missing': None, 'area': '16-services'})
        self.assertEqual(unfinished['document-privacy']['status'], 'not built')
        self.assertEqual(unfinished['logic'], {'status': 'not built', 'missing': None, 'area': '03-logic'})
        self.assertNotIn('email', unfinished)
        self.assertNotIn('reading-gzip', unfinished)

    def test_what_is_outside_the_reference_has_none(self):
        self.assertNotIn('a.b', self.places)
        self.assertNotIn('the-page-nobody-has', self.places)


class TheTree(unittest.TestCase):
    """What nav.json holds."""

    def setUp(self):
        self.nav = reference().nav(pages(), TYPE_GROUPS, href, '0.3.0')
        self.groups = {group['title']: group for group in self.nav['groups']}

    def page_named(self, group, title):
        return next(p for p in self.groups[group]['children'] if p['title'] == title)

    def test_the_groups_are_in_the_manifests_order(self):
        self.assertEqual(self.nav['version'], '0.3.0')
        self.assertEqual([g['title'] for g in self.nav['groups']], ['Language', 'Services', 'Tools', 'Errors'])

    def test_every_node_has_the_same_keys(self):
        keys = ['id', 'title', 'summary', 'href', 'status', 'missing', 'status_column', 'topics', 'details', 'built',
                'children']

        def walk(node):
            self.assertEqual(sorted(node), sorted(keys), node['title'])
            for child in node['children']:
                walk(child)
        for group in self.nav['groups']:
            walk(group)

    def test_an_entry_says_what_it_is_and_where(self):
        logic = self.page_named('Language', 'Logic')
        self.assertEqual(logic['topics'][1], {'id': 'loops', 'kind': 'topic', 'title': 'Loops',
                                              'summary': 'About loops.', 'href': '/docs/loops', 'status': None})

    def test_details_and_what_is_built_are_kept_apart(self):
        logic = self.page_named('Language', 'Logic')
        self.assertEqual([t['id'] for t in logic['topics']], ['logic', 'loops'])
        self.assertEqual([t['id'] for t in logic['details']], ['recursion'])
        self.assertEqual([t['id'] for t in logic['built']], ['logic-built'])

    def test_a_node_without_its_landing_yet_opens_its_first_topic(self):
        self.assertEqual(self.groups['Language']['id'], 'language')
        self.assertEqual(self.groups['Language']['href'], '/docs/logic')
        self.assertEqual(self.page_named('Tools', 'Commands')['href'], '/docs/the-command')
        self.assertEqual(self.groups['Tools']['href'], '/docs/the-command')

    def test_a_node_whose_id_is_a_page_opens_it(self):
        self.assertEqual(self.groups['Services']['href'], '/docs/services')
        self.assertEqual(self.page_named('Language', 'Logic')['children'][0]['href'], '/docs/types')

    def test_a_landing_that_is_a_topic_is_not_listed_again(self):
        self.assertEqual([t['id'] for t in self.groups['Services']['topics']], ['how-services-work'])

    def test_a_page_with_no_id_opens_its_first_topic(self):
        page = self.page_named('Services', 'Email')
        self.assertIsNone(page['id'])
        self.assertEqual(page['href'], '/docs/email')

    def test_a_page_the_site_writes_itself_is_its_address(self):
        self.assertEqual(self.groups['Errors']['href'], '/docs/errors')

    def test_a_summary_is_the_manifests_or_the_first_topics(self):
        self.assertEqual(self.groups['Language']['summary'], 'How a program is written.')
        self.assertEqual(self.groups['Services']['summary'], 'About services.')
        self.assertEqual(self.page_named('Services', 'Email')['summary'], 'About email.')

    def test_a_status_and_whats_missing(self):
        email = self.page_named('Services', 'Email')
        self.assertEqual((email['status'], email['missing']), ('built', None))
        files = self.page_named('Services', 'Files')
        self.assertEqual((files['status'], files['missing']), ('partly built', 'uploads and downloads'))
        documents = self.page_named('Services', 'Documents')
        self.assertEqual((documents['status'], documents['missing']), ('not built', None))
        self.assertIsNone(self.groups['Language']['status'])

    def test_a_row_mixing_built_and_not_is_partly_built(self):
        rows = dict(ROWS, **{'16-services': [('Email', 'Built.'), ('Mail packages', 'Not built yet.')] + ROWS['16-services'][2:]})
        nav = reference(rows=rows).nav(pages(), TYPE_GROUPS, href, '0.3.0')
        email = next(p for p in nav['groups'][1]['children'] if p['title'] == 'Email')
        self.assertEqual(email['status'], 'partly built')

    def test_whats_missing_goes_once_the_page_is_built(self):
        rows = dict(ROWS, **{'16-services': [r if r[0] != 'Files' else ('Files', 'Built.') for r in ROWS['16-services']]})
        nav = reference(rows=rows).nav(pages(), TYPE_GROUPS, href, '0.3.0')
        files = next(p for p in nav['groups'][1]['children'] if p['title'] == 'Files')
        self.assertEqual((files['status'], files['missing']), ('built', None))

    def test_a_type_group_lists_its_topics_then_its_types(self):
        money = self.page_named('Language', 'Logic')['children'][0]['children'][0]
        self.assertEqual([(t['id'], t['kind'], t['href']) for t in money['topics']],
                         [('money-and-payments', 'topic', '/docs/money-and-payments'),
                          ('type-money', 'type', '/docs/types/type-money'),
                          ('type-iban', 'type', '/docs/types/type-iban')])

    def test_a_type_group_with_no_topics_opens_its_first_type(self):
        text = SMALL.replace('topics = ["money-and-payments"]\n', '').replace('"logic", "loops"',
                                                                              '"logic", "loops", "money-and-payments"')
        nav = reference(text).nav(pages(), TYPE_GROUPS, href, '0.3.0')
        money = nav['groups'][0]['children'][0]['children'][0]['children'][0]
        self.assertEqual(money['href'], '/docs/types/type-money')

    def test_commands_follow_the_topics(self):
        commands = self.page_named('Tools', 'Commands')
        self.assertEqual([(t['id'], t['kind']) for t in commands['topics']],
                         [('the-command', 'topic'), ('jadpo-check', 'command'), ('jadpo-test', 'command')])

    def test_a_broken_manifest_still_gives_a_tree(self):
        text = SMALL.replace('"loops"', '"loops", "no-such-topic"').replace('16-services: Files', '16-services: Uploads')
        nav = reference(text).nav(pages(), TYPE_GROUPS, href, '0.3.0')
        logic = nav['groups'][0]['children'][0]
        self.assertEqual([t['id'] for t in logic['topics']], ['logic', 'loops'])


class Loading(unittest.TestCase):
    def test_no_manifest_is_none(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertIsNone(manifest.load(Path(tmp)))

    def test_the_rows_come_from_each_areas_table(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs/language').mkdir(parents=True)
            (root / 'docs/language/16-services.md').write_text(
                '# Services\n\n## What is built\n<!-- topic: services-built -->\n\n'
                '| Feature | Status |\n|---|---|\n| [Email](#email) | Built: all of it. |\n'
                '| `Files` ([Files](#files)) | Partly built. |\n\n## Errors\n')
            self.assertEqual(manifest.status_rows(root),
                             {'16-services': [('Email', 'Built: all of it.'), ('`Files`', 'Partly built.')]})

    def test_a_manifest_that_isnt_toml_is_a_problem_not_a_crash(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs').mkdir()
            (root / manifest.MANIFEST).write_text('[[group\n')
            loaded = manifest.load(root)
            found = loaded.problems({}, {}, None)
            self.assertEqual(len(found), 1, found)
            self.assertIn(str(manifest.MANIFEST), found[0])
            self.assertEqual(loaded.places({}, None), {})
            self.assertEqual(loaded.nav({}, None, href, '0.3.0')['groups'], [])


if __name__ == '__main__':
    unittest.main()
