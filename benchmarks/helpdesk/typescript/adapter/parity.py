#!/usr/bin/env python3
"""The twin answers as the Jadpo app does: drives this app through the
helpdesk fixtures and every step of benchmarks/agent-changes/desk_parity.py,
then compares with test/parity-reference.json, what the Jadpo app answered.
Starts and stops a PostgreSQL of its own unless JADPO_TEST_DATABASE_URL
names one. usage: python3 adapter/parity.py [--write recorded.json]"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import sys
import tempfile

import run as runner

CHANGES = runner.ROOT / 'benchmarks/agent-changes'
sys.path.insert(0, str(CHANGES))
import desk  # noqa: E402
import desk_parity  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', type=Path, help='also write what the twin answered')
    args = parser.parse_args()
    reference = json.loads((runner.PROJECT / 'test/parity-reference.json').read_text())
    (runner.PROJECT / 'build').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='parity-', dir=runner.PROJECT / 'build') as folder:
        folder = Path(folder)
        shared = os.environ.get('JADPO_TEST_DATABASE_URL', '').strip()
        server = runner.SharedPostgres(shared) if shared else runner.Postgres(folder)
        controller = None
        try:
            name = 'desk_parity_' + secrets.token_hex(5)
            server.prepare(name)
            environment = {**os.environ, 'GOLDEN_PSQL': runner.tool('psql'),
                           'SESSION_SIGNING_KEY': secrets.token_urlsafe(32),
                           'BROWSER_ORIGIN': 'https://desk.example.test',
                           'CONTROL_DATABASE_URL': server.url('postgres'), 'DATABASE_URL': server.url(name)}
            controller = runner.Controller('node', environment, folder)
            recorded = desk_parity.record(desk.Desk(controller))
        finally:
            if controller:
                controller.stop()
            server.stop()
    if args.write:
        args.write.write_text(json.dumps(recorded, indent=1))
    differences = desk_parity.compare(reference, recorded)
    print(json.dumps(differences, indent=1))
    print(f'{len(recorded)} steps, {len(differences)} differences', file=sys.stderr)
    return 1 if differences else 0


if __name__ == '__main__':
    raise SystemExit(main())
