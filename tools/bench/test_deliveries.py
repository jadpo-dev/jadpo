"""Delivery comparison guards: missing, duplicate or stale work cannot win."""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from deliveries import digest, validate_build, validate_backlog


class DeliveryGuards(unittest.TestCase):
    def test_every_durable_obligation_is_required(self):
        state = {key: 10000 for key in ('receipts', 'unique', 'succeeded', 'handled', 'events', 'attempts')}
        state.update(pending=0, unexpected=0, missing=0, invalid_id=0)
        validate_backlog(state, 10000)
        for key, value in state.items():
            corrupt = {**state, key: value + 1}
            with self.subTest(field=key), self.assertRaises(ValueError):
                validate_backlog(corrupt, 10000)
            with self.subTest(missing=key), self.assertRaises(ValueError):
                validate_backlog({name: number for name, number in state.items() if name != key}, 10000)

    def test_optimized_python_refuses_unchecked_campaigns_before_artifact_access(self):
        runner = Path(__file__).with_name('deliveries.py')
        cases = [([sys.executable, '-O'], {**os.environ, 'PYTHONOPTIMIZE': '0'}),
                 ([sys.executable], {**os.environ, 'PYTHONOPTIMIZE': '1'})]
        for command, environment in cases:
            with self.subTest(command=command, optimize=environment['PYTHONOPTIMIZE']):
                answer = subprocess.run([*command, str(runner), 'measure', '--label', 'missing-artifact'],
                    env=environment, capture_output=True, text=True, timeout=10)
                self.assertEqual(answer.returncode, 2, answer.stderr)
                self.assertIn('assertions must remain enabled', answer.stderr)
                self.assertNotIn('FileNotFoundError', answer.stderr)

    def test_source_and_artifacts_must_match(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / 'tick'
            artifact.write_text('baseline')
            stamp = {'sources': {'runner': 'serial'}, 'artifacts': {str(artifact): digest(artifact)}}
            validate_build(stamp, {'runner': 'serial'})
            with self.assertRaises(ValueError):
                validate_build(stamp, {'runner': 'bounded'})
            artifact.write_text('changed')
            with self.assertRaises(ValueError):
                validate_build(stamp, {'runner': 'serial'})
            with self.assertRaises(ValueError):
                validate_build({'sources': {}, 'artifacts': {}}, {})


if __name__ == '__main__':
    unittest.main()
