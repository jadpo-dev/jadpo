"""Timing reports retain late reconciliation without inventing task effort."""
import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('task_time', ROOT / 'tools/task-time.py')
TIMING = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TIMING)


class PostCloseTiming(unittest.TestCase):
    def test_retained_late_annotation_does_not_extend_actual_work(self):
        path = ROOT / 'docs/task-timing/runs/RM-306-1c8f71d09f7d.jsonl'
        original = path.read_bytes()
        events = TIMING.read_events(path)
        result = TIMING.summarize(events)
        self.assertEqual(result['finished_at'], '2026-10-05T20:44:35+00:00')
        self.assertEqual(result['active_minutes'], 4.02)
        self.assertEqual(result['elapsed_minutes'], 4.02)
        self.assertEqual(result['post_close_annotation_minutes'], 0.83)
        self.assertIn('late_close_reconciliation', result['evidence'])
        self.assertEqual(path.read_bytes(), original)
        reopened = copy.deepcopy(events)
        reopened[-1]['phase'] = 'implementation'
        with self.assertRaisesRegex(ValueError, 'closed run cannot resume'):
            TIMING.summarize(reopened)
        changed = copy.deepcopy(events)
        changed[-1]['outcome'] = 'complete'
        with self.assertRaisesRegex(ValueError, 'change outcome'):
            TIMING.summarize(changed)


if __name__ == '__main__':
    unittest.main()
