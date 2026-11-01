#!/usr/bin/env python3
"""Tests for tools/release/: what may change in a lock file when the
version moves on, and the version the rehearsal moves to.

  python3 tools/release/release_test.py
"""
import json
import tempfile
import unittest
from pathlib import Path

import rehearsal
import versions

CRATES = {'jadpo-cli': '0.3.0', 'jadpo-check': '0.3.0'}
BEFORE = {('jadpo-cli', None): ['0.2.0'], ('jadpo-check', None): ['0.2.0'],
          ('serde', 'registry+https://github.com/rust-lang/crates.io-index'): ['1.0.200']}


class LockProblems(unittest.TestCase):
    def test_the_workspace_crates_moving_on_is_all_that_may_change(self):
        after = {**BEFORE, ('jadpo-cli', None): ['0.3.0'], ('jadpo-check', None): ['0.3.0']}
        self.assertEqual(versions.lock_problems(BEFORE, after, CRATES), [])

    def test_a_dependency_that_moves_fails(self):
        after = {**BEFORE, ('jadpo-cli', None): ['0.3.0'], ('jadpo-check', None): ['0.3.0'],
                 ('serde', 'registry+https://github.com/rust-lang/crates.io-index'): ['1.0.201']}
        problems = versions.lock_problems(BEFORE, after, CRATES)
        self.assertEqual(len(problems), 1, problems)
        self.assertIn('serde', problems[0])

    def test_a_workspace_crate_left_behind_fails(self):
        # What release-please's own Cargo.lock entries did for 0.2.0.
        after = {**BEFORE, ('jadpo-cli', None): ['0.3.0']}
        problems = versions.lock_problems(BEFORE, after, CRATES)
        self.assertEqual(problems, ["Cargo.lock: jadpo-check is ['0.2.0'], not 0.3.0"])


class Extension(unittest.TestCase):
    def test_package_lock_takes_package_json_s_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp) / versions.EXTENSION
            folder.mkdir(parents=True)
            (folder / 'package.json').write_text('{\n  "name": "jadpo",\n  "version": "0.3.0"\n}\n')
            lock = {'name': 'jadpo', 'version': '0.2.0', 'lockfileVersion': 3,
                    'packages': {'': {'name': 'jadpo', 'version': '0.2.0'}}}
            (folder / 'package-lock.json').write_text(json.dumps(lock, indent=2) + '\n')
            self.assertEqual(len(versions.sync_extension(Path(tmp), write=False)), 1)
            self.assertEqual(versions.sync_extension(Path(tmp), write=True), [])
            written = json.loads((folder / 'package-lock.json').read_text())
            self.assertEqual((written['version'], written['packages']['']['version']), ('0.3.0', '0.3.0'))
            self.assertEqual(versions.sync_extension(Path(tmp), write=False), [])


class NextMinor(unittest.TestCase):
    def test_a_feat_moves_the_minor_number_on_before_and_after_1_0(self):
        config = {'bump-minor-pre-major': True}
        self.assertEqual(rehearsal.next_minor('0.2.0', config), '0.3.0')
        self.assertEqual(rehearsal.next_minor('0.2.5', config), '0.3.0')
        self.assertEqual(rehearsal.next_minor('1.4.2', config), '1.5.0')

    def test_the_rehearsal_writes_every_file_release_please_does(self):
        config = json.loads((rehearsal.ROOT / 'release-please-config.json').read_text())
        self.assertEqual(config['packages']['.']['extra-files'], rehearsal.EXTRA_FILES)


if __name__ == '__main__':
    unittest.main()
