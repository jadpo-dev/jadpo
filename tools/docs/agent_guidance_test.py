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
        self.assertIn('| [Identity](https://jadpo.dev/docs/identity-built.md) | All of it |', table)


class StatusWords(unittest.TestCase):
    def row(self, feature, status):
        area = AREA.replace('| Everything else on this page | Built. |', f'| {feature} | {status} |')
        return guidance.status_word_problems(project(area=area))

    def test_a_built_row_saying_part_of_it_is_not_built_is_a_problem(self):
        for status in ('Built: the list. Not built yet: `jadpo impact`.',
                       "Built. Comparing them in a query isn't built yet.",
                       'Built: `share`, against a fake (HQ itself comes later).',
                       "Built: its driver, which comes with caches.",
                       "Built: the binding, whose windows aren't yet checked.",
                       'Built. Until the timelines are built, each step is a line.'):
            problems = self.row('Flags', status)
            self.assertEqual(len(problems), 1, status)
            self.assertIn('"Flags"', problems[0])
            self.assertIn('Partly built', problems[0])

    def test_the_problem_reaches_the_gate(self):
        area = AREA.replace('| Everything else on this page | Built. |',
                            '| Flags | Built. Not built yet: tenants. |')
        self.assertTrue(any('"Flags"' in problem for problem in guidance.problems(project(area=area))))

    def test_a_row_saying_partly_built_may_say_what_is_missing(self):
        self.assertEqual(self.row('Flags', 'Partly built: the list. Not built yet: tenants.'), [])

    def test_what_a_built_row_may_say(self):
        for status in ('Built.', 'Built, with the checks under [Errors](#errors).',
                       'Built: `toolchain.not_built_yet` where it is written.',
                       'Built. It is [listed](other.md#wording-not-yet-accepted).',
                       'Built. Not deployed yet: a maintainer switches it on.',
                       'Built: a route waits for the work its action set off.',
                       "Built: the kind comes from what a test calls."):
            self.assertEqual(self.row('Flags', status), [], status)

    def test_a_status_starts_with_one_of_the_three_words(self):
        problems = self.row('Flags', 'Mostly there.')
        self.assertEqual(len(problems), 1)
        self.assertIn('"Mostly there."', problems[0])

    def test_only_the_first_table_holds_statuses(self):
        self.assertIn('Not built yet, a second table', AREA)
        self.assertEqual(guidance.status_word_problems(project()), [])

    def test_the_real_tables_are_honest(self):
        self.assertEqual(guidance.status_word_problems(), [])


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
        self.assertIn('[01-first-project](https://github.com/jadpo-dev/jadpo/tree/v{release}/examples/01-first-project)',
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
        for phrase in ['`jadpo check` after each connected change', '`layout.unused_declaration`',
                       '`jadpo help <rule>`', '`jadpo acceptance new', 'one topic', 'Not built yet']:
            self.assertIn(phrase, text)

    def test_the_agents_template_links_examples_at_the_release_that_wrote_it(self):
        text = guidance.TEMPLATE.read_text()
        self.assertIn('/tree/v{release}/examples/', text)
        self.assertNotIn('/tree/main/', text)


CARGO = """[workspace.package]
repository = "https://github.com/acme/tool"
"""

CLI = """[package.metadata.dist]
formula = "tool"
npm-package = "tool"
"""

DIST = """[dist]
installers = ["shell", "powershell", "homebrew", "npm"]
npm-scope = "@acme"
tap = "acme/homebrew-tap"
"""


def released(root, dist=DIST):
    (root / 'Cargo.toml').write_text(CARGO)
    (root / 'crates/jadpo-cli').mkdir(parents=True)
    (root / 'crates/jadpo-cli/Cargo.toml').write_text(CLI)
    (root / 'dist-workspace.toml').write_text(dist)
    return root


class ReleaseFacts(unittest.TestCase):
    def test_the_install_commands_are_what_dist_ships(self):
        commands = dict(guidance.install_commands(released(project())))
        self.assertEqual(commands, {
            'On macOS or Linux:': "curl --proto '=https' --tlsv1.2 -LsSf "
                                  'https://github.com/acme/tool/releases/latest/download/jadpo-cli-installer.sh | sh',
            'On Windows, in PowerShell:': 'powershell -c "irm '
                                          'https://github.com/acme/tool/releases/latest/download/jadpo-cli-installer.ps1 | iex"',
            'With Homebrew:': 'brew install acme/tap/tool',
            'With npm:': 'npm install -g @acme/tool',
        })

    def test_an_installer_dist_drops_leaves_the_page(self):
        dist = DIST.replace('"shell", "powershell", "homebrew", "npm"', '"homebrew"')
        section = guidance.learn_install(released(project(), dist))
        self.assertIn('brew install acme/tap/tool', section)
        self.assertNotIn('curl', section)
        self.assertNotIn('npm', section)

    def test_an_installer_the_page_has_no_words_for_is_a_problem(self):
        dist = DIST.replace('"npm"', '"msi"')
        self.assertEqual(guidance.install_problems(released(project(), dist)),
                         ['dist-workspace.toml ships the installer "msi", which tools/docs/agent_guidance.py has no command for'])

    def test_the_first_learn_page_installs_with_what_the_release_ships(self):
        page = (guidance.ROOT / 'docs/learn/01-setting-up.md').read_text()
        for command in ('jadpo-cli-installer.sh | sh', 'jadpo-cli-installer.ps1 | iex',
                        'brew install jadpo-dev/tap/jadpo', 'npm install -g @jadpo/jadpo'):
            self.assertIn(command, page)
        self.assertNotIn('cargo build', page)
        self.assertNotIn('## Not built yet', page)

    def test_a_first_page_that_says_to_build_from_source_is_a_problem(self):
        root = project()
        (root / 'docs/learn').mkdir(parents=True)
        (root / 'docs/primer.md').write_text('# Primer\n\nThere is no installer yet.\n')
        (root / 'docs/learn/02-next.md').write_text('# Next\n\nRun cargo build first.\n')
        self.assertEqual(guidance.install_claim_problems(root), [
            'docs/learn/02-next.md:3: says how to get `jadpo` by hand (`cargo build`); '
            "the first Learn page's `## Install it` is generated from what a release ships, so link it",
            'docs/primer.md:3: says how to get `jadpo` by hand (`no installer`); '
            "the first Learn page's `## Install it` is generated from what a release ships, so link it",
        ])


PICKS = (('08-identity', '`Driver.github`', 'GitHub sign-in'),
         ('08-identity', 'OAuth grants', 'OAuth grants'),
         ('08-identity', 'Scope declarations', 'token scopes'),
         ('08-identity', 'Passkeys', 'passkeys'))

IDENTITY = """# Identity
<!-- topic: identity -->

## What is built
<!-- topic: identity-built -->

| Feature | Status |
|---|---|
| Passkeys | Built. |
| `Driver.github` and `oauth_provider:` | Not built yet. |
| Scope declarations, in [Policy](09-policy.md) | Partly built: the rest. |
| OAuth grants: `OAuthGrant` credentials | Not built yet. |
"""


class PrimerNotBuilt(unittest.TestCase):
    def test_it_lists_a_feature_only_while_its_area_says_it_is_not_built(self):
        section = guidance.primer_not_built(project(area=IDENTITY), PICKS)
        self.assertIn('GitHub sign-in; OAuth grants; and token scopes.', ' '.join(section.split()))
        self.assertNotIn('passkeys', section)

    def test_a_feature_built_since_leaves_the_list(self):
        area = IDENTITY.replace('| OAuth grants: `OAuthGrant` credentials | Not built yet. |',
                                '| OAuth grants: `OAuthGrant` credentials | Built. |')
        section = guidance.primer_not_built(project(area=area), PICKS)
        self.assertNotIn('OAuth grants', section)

    def test_one_left_reads_as_a_sentence_and_none_left_leaves_the_rule(self):
        built = IDENTITY.replace('Not built yet.', 'Built.').replace('Partly built: the rest.', 'Built.')
        none = guidance.primer_not_built(project(area=built), PICKS)
        self.assertIn('toolchain.not_built_yet', none)
        self.assertNotIn('among others', none)
        one = built.replace('| Passkeys | Built. |', '| Passkeys | Not built yet. |')
        self.assertIn('among others: passkeys. Each', ' '.join(guidance.primer_not_built(project(area=one), PICKS).split()))

    def test_a_row_its_area_no_longer_has_is_a_problem(self):
        area = IDENTITY.replace('| Passkeys | Built. |\n', '')
        self.assertEqual(guidance.primer_pick_problems(project(area=area), PICKS), [
            'the primer\'s list reads "Passkeys" in docs/language/08-identity.md, whose "What is built" table '
            'has no such row: change PRIMER_NOT_BUILT in tools/docs/agent_guidance.py'])

    def test_the_real_primer_lists_neither_seeds_nor_installers(self):
        page = (guidance.ROOT / 'docs/primer.md').read_text()
        built = page[page.index('\n## Not built yet\n'):]
        for word in ('seed', 'install', 'cargo', 'flags'):
            self.assertNotIn(word, built)

    def test_the_real_primer_links_how_to_install_it(self):
        self.assertIn('(learn/01-setting-up.md#install-it)', (guidance.ROOT / 'docs/primer.md').read_text())


if __name__ == '__main__':
    unittest.main()
