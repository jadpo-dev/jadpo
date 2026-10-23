"""Timing-independent guards shared by the golden application comparison."""
import hashlib
import datetime as dt
from pathlib import Path

TARGETS = ('native', 'workers', 'bun', 'node')


def validate_conformance(reports, case_ids):
    expected = {(target, case) for target in TARGETS for case in case_ids}
    observed = [(row.get('target'), row.get('id')) for row in reports]
    if (len(case_ids) != 44 or len(observed) != len(expected)
            or set(observed) != expected
            or any(row.get('status') != 'passed' for row in reports)):
        raise ValueError('All 44 current-source contract cases must pass on all four targets')


def validate_build(stamp, inputs):
    artifacts = stamp.get('artifacts', {})
    if stamp.get('inputs') != inputs or not artifacts:
        raise ValueError('Build inputs changed or artifact identity is missing; rebuild required')
    for name, expected in artifacts.items():
        path = Path(name)
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError('A built artifact changed; rebuild required')


def validate_state(before, after, successes, owners):
    original = {row['id']: row for row in before}
    actual = {row['id']: row for row in after}
    if len(actual) != len(after) or any(actual.get(key) != row for key, row in original.items()):
        raise ValueError('The workload changed or removed an existing stored row')
    created = [row for key, row in actual.items() if key not in original]
    if len(created) != successes:
        raise ValueError('Persisted writes differ from every successful response including warmup and drain')
    for row in created:
        if (row['owner_id'] not in owners or row['title'] != 'bench-write'
                or row['status'] != 'open' or row['due_at'] is not None
                or row['deleted_at'] is not None or row['reminder_revision'] != 1
                or row['reminder_sent_at'] is not None or row['reminder_queued_at'] is not None
                or not row['created_at'] or row['updated_at'] != row['created_at']):
            raise ValueError('A persisted write has incorrect owner, fields or lifecycle state')


def validate_receipts(receipts, after, successes):
    fields = {'id', 'owner_id', 'title', 'status', 'due_at', 'created_at', 'updated_at'}
    def normalized(row):
        result = {key: row[key] for key in fields}
        for key in ('created_at', 'updated_at'):
            instant = dt.datetime.fromisoformat(result[key].replace('Z', '+00:00'))
            if instant.tzinfo is None:
                raise ValueError('Write receipt has no timestamp timezone')
            result[key] = instant.astimezone(dt.timezone.utc)
        return result
    stored = {row['id']: normalized(row) for row in after}
    if len(receipts) != successes or len({row['id'] for row in receipts}) != successes:
        raise ValueError('Every successful write needs one unique complete response receipt')
    for receipt in receipts:
        if set(receipt) != fields or stored.get(receipt['id']) != normalized(receipt):
            raise ValueError('A write response differs from its persisted row or signed owner')
