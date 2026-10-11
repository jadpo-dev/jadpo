"""Tests for report facts that must not become misleading progress claims."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('progress', Path(__file__).resolve().parents[2] / 'tools/project-progress.py')
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)

ROADMAP = '''## E01 — Example
| ID | Remaining task and completion condition | Effort | Depends on | State |
|---|---|---|---|---|
| RM-101 | Ship a bounded change | 1–2h (L) | — | Ready |
| RM-102 | Obtain outside evidence | Unmeasured (U) | RM-101 | External |
| RM-103 | Optional extension | 2–8h (N) | RM-101 | Conditional |
'''


class ReportFacts(unittest.TestCase):
    def test_unknown_planning_is_not_claimed_missing_or_complete(self):
        data = m.collect(ROADMAP, '', {})
        self.assertEqual({t['planning'] for t in data['tasks'].values()}, {'unassessed'})
        self.assertFalse(data['completed'])

    def test_explicit_plan_and_conflicts(self):
        plan = '| Task ID | Planning state |\n| RM-101 | planned |\n'
        data = m.collect(ROADMAP, '', {'plan.md': plan})
        self.assertEqual(data['tasks']['RM-101']['planning'], 'planned')
        self.assertEqual(data['tasks']['RM-102']['planning'], 'unassessed')
        with self.assertRaises(ValueError):
            m.collect(ROADMAP, '', {'a.md': plan, 'b.md': plan.replace('planned', 'needs input')})

    def test_completed_marker_and_reopened(self):
        history = '### 2026-10-01 — Delivered\n**Completed task:** RM-100\n**Completed task:** RM-101\n'
        data = m.collect(ROADMAP, history, {})
        self.assertEqual(set(data['completed']), {'RM-100'})
        self.assertEqual(data['reopened_or_conflicting_ids'], ['RM-101'])

    def test_effort_retains_unknown_and_excludes_conditional_in_report(self):
        data = m.collect(ROADMAP, '', {})
        active = [t for t in data['tasks'].values() if t['state'] != 'conditional']
        self.assertEqual(m.effort(active), '1–2h + 1 unestimated')
        self.assertIsNone(m.estimate('0.25–0.75h (S) per decision'))
        self.assertIsNone(m.estimate('Unestimated (needs planning)'))
        self.assertIsNone(m.estimate('Unestimated (needs planning; owner session time separate)'))
        data.update(date='2026-10-01', captured_at='test', changes=None)
        notes = dict(summary='Summary', delivered=['Evidence'], next='Next', input_needed='None')
        self.assertIn('| E01 — Example | 3 | 1 | 1–2h + 1 unestimated |', m.render(data, notes))

    def test_removed_is_not_automatically_completed(self):
        old = {'tasks': {'RM-101': {}, 'RM-102': {}}, 'completed': {}}
        current = {'tasks': {'RM-103': {}}, 'completed': {'RM-101': {}}}
        self.assertEqual(m.delta(current, old), {'completed': ['RM-101'], 'added_or_reopened': ['RM-103'], 'removed_without_completion': ['RM-102']})
        self.assertIsNone(m.delta(current, None))

    def test_malformed_inputs_fail_instead_of_publishing_wrong_totals(self):
        for value in ['8–1h (L)', 'about a week']:
            with self.assertRaises(ValueError):
                m.estimate(value)
        with self.assertRaises(ValueError):
            m.collect(ROADMAP + '| RM-101 | Duplicate | 1–2h (L) | — | Ready |\n', '', {})
        with self.assertRaises(ValueError):
            m.collect(ROADMAP.replace('Ready', 'Finished'), '', {})


if __name__ == '__main__':
    unittest.main()
