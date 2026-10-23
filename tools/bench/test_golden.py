"""Fault controls for the application comparison; no timing thresholds."""
import copy
import tempfile
import subprocess
import sys
import unittest
from pathlib import Path

from golden_contracts import validate_conformance, validate_build, validate_state, validate_receipts
from golden import load_output


class GoldenControls(unittest.TestCase):
    def test_all_four_targets_need_every_case_from_this_source(self):
        ids = {f'CASE-{i:03}' for i in range(44)}
        reports = [{'target': target, 'id': case, 'status': 'passed'}
                   for target in ('native', 'workers', 'bun', 'node') for case in ids]
        validate_conformance(reports, ids)
        for corrupt in (reports[:-1], reports + [reports[0]],
                        [{**row, 'status': 'unsupported'} if i == 0 else row
                         for i, row in enumerate(reports)],
                        [{**row, 'target': 'preview'} if i == 0 else row
                         for i, row in enumerate(reports)]):
            with self.assertRaises(ValueError):
                validate_conformance(corrupt, ids)

    def test_build_rejects_stale_input_or_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / 'server'
            artifact.write_bytes(b'production')
            stamp = {'inputs': 'current', 'artifacts': {str(artifact):
                     __import__('hashlib').sha256(artifact.read_bytes()).hexdigest()}}
            validate_build(stamp, 'current')
            with self.assertRaises(ValueError):
                validate_build(stamp, 'changed')
            artifact.write_bytes(b'stale')
            with self.assertRaises(ValueError):
                validate_build(stamp, 'current')
            with self.assertRaises(ValueError):
                validate_build({'inputs': 'current', 'artifacts': {}}, 'current')

    def test_write_audit_requires_exact_fields_and_signed_owners(self):
        seeded = [{'id': 'seed', 'owner_id': 'alice', 'title': 'original', 'status': 'open'}]
        created = {'id': 'new', 'owner_id': 'alice', 'title': 'bench-write',
                   'status': 'open', 'due_at': None, 'deleted_at': None,
                   'reminder_revision': 1, 'reminder_sent_at': None,
                   'reminder_queued_at': None, 'created_at': 'now', 'updated_at': 'now'}
        validate_state(seeded, seeded + [created], 1, {'alice'})
        for wrong in (seeded, seeded + [{**created, 'owner_id': 'bob'}],
                      seeded + [{**created, 'title': 'corrupt'}],
                      seeded + [{**created, 'deleted_at': 'now'}],
                      [{**seeded[0], 'title': 'mutated'}, created]):
            with self.assertRaises(ValueError):
                validate_state(seeded, wrong, 1, {'alice'})

    def test_read_cannot_change_state_or_be_a_noop_write(self):
        seeded = [{'id': 'seed', 'owner_id': 'alice'}]
        validate_state(seeded, copy.deepcopy(seeded), 0, {'alice'})
        with self.assertRaises(ValueError):
            validate_state(seeded, [], 0, {'alice'})
        with self.assertRaises(ValueError):
            validate_state(seeded, seeded, 1, {'alice'})

    def test_each_write_response_must_match_its_saved_row_and_owner(self):
        row = {'id': 'new', 'owner_id': 'alice', 'title': 'bench-write',
               'status': 'open', 'due_at': None, 'created_at': '2026-10-23T12:00:00Z',
               'updated_at': '2026-10-23T12:00:00Z'}
        validate_receipts([row], [row], 1)
        for receipts in ([], [row, row], [{**row, 'id': 'made-up'}],
                         [{**row, 'owner_id': 'bob'}], [{**row, 'created_at': ''}],
                         [{**row, 'created_at': '2026-10-23T12:00:00'}]):
            with self.assertRaises(ValueError):
                validate_receipts(receipts, [row], 1)

    def test_unsupported_targets_and_empty_cells_are_refused_before_building(self):
        runner = Path(__file__).with_name('golden.py')
        for arguments in (['--targets', 'interpreted'], ['--targets', 'native,'],
                          ['--connections', ''], ['--duration', 'nan'],
                          ['--routes', 'no-op'], ['--rounds', '0']):
            answer = subprocess.run([sys.executable, '-B', str(runner), *arguments],
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(answer.returncode, 2, answer.stderr)
            self.assertIn('error:', answer.stderr)

    def test_nonzero_load_and_timeout_keep_both_raw_streams(self):
        for failure in (True, False):
            script = ("import sys,time; print('partial stdout',flush=True); "
                      "print('partial stderr',file=sys.stderr,flush=True); "
                      + ('sys.exit(7)' if failure else 'time.sleep(30)'))
            process = subprocess.Popen([sys.executable, '-c', script], stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, text=True)
            row = {}
            try:
                with self.assertRaises(ValueError):
                    load_output(process, 5 if failure else 1, row)
                self.assertIn('partial stdout', row['load_stdout'])
                self.assertIn('partial stderr', row['load_stderr'])
                self.assertIsNotNone(process.poll())
                self.assertEqual(row.get('load_timeout', False), not failure)
                if failure:
                    self.assertEqual(row['load_exit_code'], 7)
            finally:
                if process.poll() is None:
                    process.kill(); process.communicate(timeout=5)


if __name__ == '__main__':
    unittest.main()
