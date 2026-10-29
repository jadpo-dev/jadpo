#!/usr/bin/env python3
"""Tests for tools/commit_type.py, each in a git repository of its own.

  python3 tools/commit_type_test.py
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

import commit_type

PAGE = '''# A page

```jadpo check project=greeting expect=semantic.unknown_name
function greet() -> Text {
    return nobody
}
```

```text output
{output}
```
'''

def page(output):
    return PAGE.replace('{output}', output)


LIB = '''rules! {{
    SEMANTIC_UNKNOWN_NAME: "semantic.unknown_name", area 1, Error, Agent,
        "Name is not declared";
{extra}}}
'''


class Repo:
    def __init__(self, folder):
        self.root = Path(folder)
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.email', 'test@example.com')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'commit.gpgsign', 'false')

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.root, check=True, capture_output=True,
                              text=True).stdout.strip()

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def commit(self, message):
        self.git('add', '-A')
        self.git('commit', '-q', '--allow-empty', '-m', message)
        return self.git('rev-parse', 'HEAD')


class CommitTypes(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.repo = Repo(self.folder.name)
        self.saved = commit_type.ROOT
        commit_type.ROOT = self.repo.root
        self.repo.write('docs/language/01-page.md', page('error: `nobody` is not declared'))
        self.repo.write(commit_type.CATALOGUE, LIB.format(extra=''))
        self.base = self.repo.commit('docs: the first page')
        self.repo.git('checkout', '-q', '-b', 'topic')

    def tearDown(self):
        commit_type.ROOT = self.saved
        self.folder.cleanup()

    def check(self, title):
        head = self.repo.git('rev-parse', 'HEAD')
        return [summary for summary, _, _ in commit_type.problems(self.base, head, title)]

    def change_the_output(self, message):
        self.repo.write('docs/language/01-page.md', page('error: `nobody` is unknown'))
        self.repo.commit(message)

    def test_a_changed_result_under_fix_fails(self):
        self.change_the_output('fix: say unknown')
        self.assertEqual(self.check('fix: say unknown'),
                         ['The language changes under a type that says it doesn\'t'])

    def test_a_changed_result_under_feat_passes(self):
        self.change_the_output('feat: say unknown')
        self.assertEqual(self.check('feat: say unknown'), [])

    def test_a_changed_result_under_a_breaking_feat_passes(self):
        self.change_the_output('feat!: say unknown')
        self.assertEqual(self.check('feat!: say unknown'), [])

    def test_a_changed_example_and_its_result_is_not_a_language_change(self):
        text = page('error: `someone` is not declared').replace('return nobody', 'return someone')
        self.repo.write('docs/language/01-page.md', text)
        self.repo.commit('docs: a clearer example')
        self.assertEqual(self.check('docs: a clearer example'), [])

    def test_an_example_moved_to_another_page_is_not_a_language_change(self):
        self.repo.write('docs/language/01-page.md', '# A page\n')
        self.repo.write('docs/errors/semantic.unknown_name.md',
                        page('error: `nobody` is not declared'))
        self.repo.commit('docs: the example on its error page')
        self.assertEqual(self.check('docs: the example on its error page'), [])

    def test_a_changed_result_in_the_readme_counts(self):
        self.repo.write('README.md', page('error: one'))
        self.base = self.repo.commit('docs: a readme')
        self.repo.write('README.md', page('error: two'))
        self.repo.commit('docs: another output')
        self.assertEqual(self.check('docs: another output'),
                         ['The language changes under a type that says it doesn\'t'])

    def test_a_rule_entering_the_catalogue_needs_feat(self):
        extra = '    SEMANTIC_NEW: "semantic.new", area 1, Error, Agent,\n        "Something new";\n'
        self.repo.write(commit_type.CATALOGUE, LIB.format(extra=extra))
        self.repo.commit('refactor: a new rule')
        self.assertEqual(self.check('refactor: a new rule'),
                         ['The language changes under a type that says it doesn\'t'])

    def test_a_rule_leaving_the_catalogue_needs_feat(self):
        self.repo.write(commit_type.CATALOGUE, 'rules! {\n}\n')
        self.repo.commit('feat!: no rules')
        self.assertEqual(self.check('feat!: no rules'), [])
        self.assertIn('rule semantic.unknown_name left the catalogue',
                      commit_type.language_changes(self.base, self.repo.git('rev-parse', 'HEAD')))

    def test_a_title_that_is_not_a_conventional_commit_fails(self):
        self.repo.commit('docs: a word')
        self.assertEqual(self.check('Fix the thing'), ['The title `Fix the thing` isn\'t a Conventional Commit'])
        self.assertEqual(self.check('feature: a thing'),
                         ['The title `feature: a thing` isn\'t a Conventional Commit'])

    def test_the_branch_commits_are_not_compared_with_the_title(self):
        self.repo.commit('docs: a word')
        self.assertEqual(self.check('fix: a word'), [])
        self.repo.commit('feat: a thing')
        self.assertEqual(self.check('docs: a thing'), [])

    def test_a_breaking_change_footer_is_breaking(self):
        self.change_the_output('feat: say unknown\n\nBREAKING CHANGE: the message changed')
        self.assertEqual(self.check('feat!: say unknown'), [])

    def test_a_scope_is_allowed(self):
        self.repo.commit('fix(cli): a word')
        self.assertEqual(self.check('fix(cli): a word'), [])

    def test_the_release_pull_request_passes(self):
        self.repo.commit('chore(main): release 0.2.0')
        self.assertEqual(self.check('chore(main): release 0.2.0'), [])


if __name__ == '__main__':
    unittest.main()
