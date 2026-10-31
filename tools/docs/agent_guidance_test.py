#!/usr/bin/env python3
"""Tests for tools/docs/agent_guidance.py: what an agent reads first.

  python3 tools/docs/agent_guidance_test.py
"""

import tempfile
import unittest
from pathlib import Path

import agent_guidance as guidance

AREA = """---
title: Identity
area: 8
---

# Identity
<!-- topic: identity -->

Text.

## What is built
<!-- topic: identity-built -->

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| `sign_in:`'s `providers:` ([Integrations](11-integrations.md)) | Not built yet. |
| Scope declarations, in [Policy](09-policy.md) | Partly built: the rest. |

| Reported by | Mistake |
|---|---|
| `jadpo check` | Not built yet, a second table |

## Errors
"""

EXAMPLES = """# Jadpo by example

1. [01-first-project/](01-first-project/README.md): the project root
   and a domain.
2. [02-named-types/](02-named-types/README.md): every built-in
   representation.

## The closest example for each feature

| Feature | Example |
|---|---|
| Project layout | [01-first-project](01-first-project/README.md) |
| Named types | [02-named-types](02-named-types/README.md) |

## How the comments work
"""


def project(area=AREA, examples=EXAMPLES, folders=('01-first-project', '02-named-types')):
    root = Path(tempfile.mkdtemp())
    (root / 'docs/language').mkdir(parents=True)
    (root / 'docs/language/08-identity.md').write_text(area)
    (root / 'examples').mkdir()
    (root / 'examples/README.md').write_text(examples)
    for folder in folders:
        (root / 'examples' / folder).mkdir()
        (root / 'examples' / folder / 'app.jadpo').write_text('application {}\n')
    return root


class WhatIsBuilt(unittest.TestCase):
    def test_each_area_lists_what_is_not_built_from_its_first_table(self):
        rows = guidance.not_built(project())
        self.assertEqual(rows, [('Identity', 'identity-built', [
            "`sign_in:`'s `providers:`",
            'Scope declarations, in Policy (partly)',
        ])])

    def test_an_area_with_everything_built_says_so(self):
        area = AREA.replace("| `sign_in:`'s `providers:` ([Integrations](11-integrations.md)) | Not built yet. |\n", '')
        area = area.replace('| Scope declarations, in [Policy](09-policy.md) | Partly built: the rest. |\n', '')
        table = guidance.built_table(guidance.not_built(project(area=area)))
        self.assertIn('| [Identity](https://jadpo.dev/docs/topics/identity-built.md) | All of it |', table)


class Examples(unittest.TestCase):
    def test_every_example_is_listed_in_reading_order(self):
        listed = guidance.examples(project())
        self.assertEqual([folder for folder, _ in listed], ['01-first-project', '02-named-types'])
        self.assertEqual(listed[0][1], 'the project root and a domain.')

    def test_an_example_missing_from_the_reading_order_is_a_problem(self):
        root = project(folders=('01-first-project', '02-named-types', '03-objects'))
        self.assertIn('examples/03-objects is in no line of examples/README.md\'s reading order',
                      guidance.problems(root))

    def test_the_feature_table_links_each_example_where_it_is_read(self):
        features = guidance.closest_examples(project())
        self.assertEqual(features, [('Project layout', ['01-first-project']), ('Named types', ['02-named-types'])])
        self.assertIn('[01-first-project](https://github.com/jadpo-dev/jadpo/tree/main/examples/01-first-project)',
                      guidance.feature_table(features, guidance.GITHUB_TREE))


class InStep(unittest.TestCase):
    def test_the_written_files_are_what_the_sources_give(self):
        self.assertEqual(guidance.problems(guidance.ROOT), [])

    def test_llms_txt_lists_every_teaching_example(self):
        text = (guidance.ROOT / 'llms.txt').read_text()
        for folder in sorted(p.name for p in (guidance.ROOT / 'examples').iterdir() if (p / 'app.jadpo').is_file()):
            self.assertIn(f'/examples/{folder}/README.md', text, folder)

    def test_the_agents_template_keeps_its_working_method(self):
        text = guidance.TEMPLATE.read_text()
        for phrase in ['`jadpo check` after each file', '`jadpo help <rule>`', '`jadpo acceptance new',
                       'one topic', 'Not built yet']:
            self.assertIn(phrase, text)


if __name__ == '__main__':
    unittest.main()
