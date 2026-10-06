#!/usr/bin/env python3
"""Record roadmap work sessions; explicit pauses keep waiting out of active time."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import fcntl
import json
from pathlib import Path
import re
import uuid

ROOT = Path(__file__).resolve().parents[1]
DEFAULT = ROOT / 'docs/task-timing/runs'
ACTIVE = {'planning', 'implementation', 'verification', 'review'}
PHASES = sorted(ACTIVE | {'blocked', 'paused'})


def now():
    return datetime.now(timezone.utc).isoformat(timespec='microseconds')


def read_events(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def summarize(events):
    first, last = events[0], events[-1]
    seconds = {phase: 0.0 for phase in PHASES}
    unknown_seconds = 0.0
    closed_event = next((event for event in events if event['phase'] == 'finished'), None)
    post_close_seconds = 0.0
    for previous, current in zip(events, events[1:]):
        delta = (datetime.fromisoformat(current['at']) -
                 datetime.fromisoformat(previous['at'])).total_seconds()
        if delta < 0:
            raise ValueError('Clock moved backwards; retain and investigate this record')
        if previous['phase'] == 'finished':
            # Retained historical records may append a late closeout annotation.
            # It cannot reopen work, change its outcome or add active time.
            if current['phase'] != 'finished' or current.get('outcome') != closed_event.get('outcome'):
                raise ValueError('A closed run cannot resume or change outcome; retain and investigate this record')
            post_close_seconds += delta
            continue
        if current.get('unobserved_gap'):
            unknown_seconds += delta
        else:
            seconds[previous['phase']] += delta
    closed = last['phase'] == 'finished'
    return {
        'run_id': first['run_id'], 'task_id': first['task_id'],
        'actor': first['actor'], 'actor_kind': first['actor_kind'],
        'category': first['category'], 'estimate_hours': first['estimate_hours'],
        'model': first.get('model'), 'reasoning': first.get('reasoning'),
        'started_at': first['at'], 'finished_at': closed_event['at'] if closed else None,
        'outcome': last.get('outcome', 'open'),
        'active_minutes': round(sum(seconds[p] for p in ACTIVE) / 60, 2),
        'verification_minutes': round(seconds['verification'] / 60, 2),
        'blocked_minutes': round(seconds['blocked'] / 60, 2),
        'paused_minutes': round(seconds['paused'] / 60, 2),
        'unknown_minutes': round(unknown_seconds / 60, 2),
        'elapsed_minutes': round((sum(seconds.values()) + unknown_seconds) / 60, 2) if closed else None,
        'unclosed_phase': None if closed else last['phase'],
        'unclosed_since': None if closed else last['at'],
        'evidence': last.get('evidence'),
        'post_close_annotation_minutes': round(post_close_seconds / 60, 2),
    }


def start(directory, task_id, actor, actor_kind, category, estimate, thread=None,
          model=None, reasoning=None):
    if not re.fullmatch(r'(?:RM-\d+|META-[A-Z0-9-]+)', task_id):
        raise ValueError('Use a roadmap ID (RM-101) or a named META- task')
    if not actor.strip():
        raise ValueError('Actor must identify the person or agent session')
    if estimate and not (0 < estimate[0] <= estimate[1]):
        raise ValueError('Estimate must be a positive lower/upper range in hours')
    directory.mkdir(parents=True, exist_ok=True)
    for path in directory.glob('*.jsonl'):
        existing = read_events(path)
        if existing[0]['actor'] == actor and existing[-1]['phase'] in ACTIVE:
            raise ValueError(f'Pause or finish active run {path.stem} before switching tasks')
    run_id = f'{task_id}-{uuid.uuid4().hex[:12]}'
    event = {
        'schema_version': 1, 'run_id': run_id, 'task_id': task_id,
        'actor': actor, 'actor_kind': actor_kind, 'category': category,
        'estimate_hours': estimate, 'thread': thread,
        'model': model, 'reasoning': reasoning,
        'at': now(), 'phase': 'planning',
    }
    with (directory / f'{run_id}.jsonl').open('x') as handle:
        handle.write(json.dumps(event) + '\n')
    return run_id


def transition(directory, run_id, phase, note=None, outcome=None, evidence=None, unobserved_gap=False):
    if not re.fullmatch(r'(?:RM-\d+|META-[A-Z0-9-]+)-[a-f0-9]{12}', run_id):
        raise ValueError('Invalid run ID')
    if phase not in PHASES + ['finished']:
        raise ValueError('Unknown phase')
    if phase in {'blocked', 'paused'} and not note:
        raise ValueError('Blocked/paused work requires a reason')
    if phase == 'finished' and (outcome not in {'complete', 'partial', 'abandoned'} or not evidence):
        raise ValueError('Finish requires an outcome and evidence/disposition')
    path = directory / f'{run_id}.jsonl'
    with path.open('r+') as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        events = [json.loads(line) for line in handle if line.strip()]
        if events[-1]['phase'] == 'finished':
            raise ValueError('Finished records are immutable; start a new run for rework')
        if phase in ACTIVE:
            for other in directory.glob('*.jsonl'):
                if other == path:
                    continue
                record = read_events(other)
                if record[0]['actor'] == events[0]['actor'] and record[-1]['phase'] in ACTIVE:
                    raise ValueError(f'Pause active run {other.stem} before resuming this one')
        event = {'at': now(), 'phase': phase, 'note': note}
        if unobserved_gap:
            event['unobserved_gap'] = True
        if phase == 'finished':
            event.update(outcome=outcome, evidence=evidence)
        summarize(events + [event])  # Reject reversed clocks before writing.
        handle.write(json.dumps(event) + '\n')
    return summarize(events + [event])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, default=DEFAULT,
                        help='Override only for testing or a separate measurement dataset')
    sub = parser.add_subparsers(dest='command', required=True)
    begin = sub.add_parser('start')
    begin.add_argument('task_id')
    begin.add_argument('--actor', required=True, help='Use the chat ID for an agent')
    begin.add_argument('--actor-kind', choices=['agent', 'human'], default='agent')
    begin.add_argument('--category', required=True,
                       choices=['fix', 'slice', 'cross-cutting', 'subsystem', 'research'])
    begin.add_argument('--estimate', type=float, nargs=2, metavar=('LOW_HOURS', 'HIGH_HOURS'))
    begin.add_argument('--thread', help='Optional chat ID/link; no chat transcript is stored')
    begin.add_argument('--model', help='Actual model ID when known; omit if unknown')
    begin.add_argument('--reasoning', help='Actual reasoning effort when known (for example high or xhigh)')
    phase = sub.add_parser('phase')
    phase.add_argument('run_id')
    phase.add_argument('phase', choices=PHASES)
    phase.add_argument('--note')
    end = sub.add_parser('finish')
    end.add_argument('run_id')
    end.add_argument('--outcome', choices=['complete', 'partial', 'abandoned'], required=True)
    end.add_argument('--evidence', required=True, help='Report/file/commit or explicit partial disposition')
    end.add_argument('--unobserved-gap', action='store_true',
                     help='Exclude the last interval when recovering an interrupted/stale timer')
    report = sub.add_parser('report')
    report.add_argument('--task')
    args = parser.parse_args()
    try:
        if args.command == 'start':
            print(start(args.directory, args.task_id, args.actor, args.actor_kind,
                        args.category, args.estimate, args.thread, args.model, args.reasoning))
        elif args.command == 'phase':
            print(json.dumps(transition(args.directory, args.run_id, args.phase, args.note), indent=2))
        elif args.command == 'finish':
            print(json.dumps(transition(args.directory, args.run_id, 'finished',
                                        outcome=args.outcome, evidence=args.evidence,
                                        unobserved_gap=args.unobserved_gap), indent=2))
        else:
            records = [summarize(read_events(p)) for p in sorted(args.directory.glob('*.jsonl'))]
            print(json.dumps([r for r in records if not args.task or r['task_id'] == args.task], indent=2))
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    main()
