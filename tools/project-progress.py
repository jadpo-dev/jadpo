#!/usr/bin/env python3
"""Create a daily roadmap snapshot and a short report; never infer completion from deletion."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import tempfile
from zoneinfo import ZoneInfo

ROOT = Path(__file__).resolve().parents[1]
STATES = {'ready', 'queued', 'decision', 'external', 'conditional', 'in progress', 'blocked', 'needs review'}
PLANNING = {'planned', 'needs planning', 'needs input', 'unassessed'}


def cells(line):
    return [s.strip() for s in re.split(r'(?<!\\)\|', line.strip().strip('|'))]


def estimate(value):
    match = re.fullmatch(r'(\d+(?:\.\d+)?)\s*[–-]\s*(\d+(?:\.\d+)?)h\s*\([SMLN]\)', value)
    if match:
        lo, hi = map(float, match.groups())
        if not 0 < lo <= hi:
            raise ValueError(f'Invalid effort range: {value}')
        return [lo, hi]
    if (value.startswith('Unmeasured')
            or re.fullmatch(r'Unestimated \(needs planning(?:;[^)]*)?\)', value)
            or 'per decision' in value):
        return None
    raise ValueError(f'Unrecognised estimate; update parser rather than silently omit: {value}')


def collect(roadmap, history, plans):
    tasks, epics, header = {}, {}, []
    epic = None
    for line in roadmap.splitlines():
        match = re.match(r'## (E\d+) [—–-] (.+)', line)
        if match:
            epic, title = match.groups()
            epics[epic] = title
        if line.startswith('| ID |'):
            header = cells(line)
        if not re.match(r'^\| RM-\d+\s*\|', line):
            continue
        values = cells(line)
        if not epic or len(values) != len(header):
            raise ValueError(f'Malformed task row: {line}')
        row = dict(zip(header, values))
        task_id = row['ID']
        if task_id in tasks:
            raise ValueError(f'Duplicate task ID: {task_id}')
        state = row['State'].lower()
        if state not in STATES:
            raise ValueError(f'Unrecognised state for {task_id}: {state}')
        tasks[task_id] = dict(id=task_id, epic=epic, description=row['Remaining task and completion condition'],
                              state=state, depends_on=row['Depends on'], estimate=row['Effort'],
                              hours=estimate(row['Effort']), planning='unassessed')
    if not tasks:
        raise ValueError('No open task rows found; inspect roadmap format')
    completed, heading, section = {}, '', ''
    for line in history.splitlines():
        if line.startswith('#'):
            heading = line.lstrip('# ').strip()
        match = re.fullmatch(r'\*\*Completed task:\*\* (RM-\d+)', line)
        if match:
            task_id = match.group(1)
            if task_id in completed:
                raise ValueError(f'Duplicate completion marker: {task_id}')
            completed[task_id] = {'title': heading, 'date': heading[:10]}
    overlap = sorted(set(tasks) & set(completed))
    # The active roadmap wins if a task reopens; the old completion remains in history.
    completed = {key: value for key, value in completed.items() if key not in tasks}
    plan_records = {}
    for name, content in plans.items():
        header = []
        for line in content.splitlines():
            if line.startswith('| Task ID |'):
                header = cells(line)
            if not re.match(r'^\| RM-\d+\s*\|', line) or 'Planning state' not in header:
                continue
            values = cells(line)
            if len(values) != len(header):
                raise ValueError(f'Malformed planning record in {name}')
            row = dict(zip(header, values))
            task_id, state = row['Task ID'], row['Planning state'].lower()
            if state not in PLANNING:
                raise ValueError(f'Unrecognised planning state in {name}: {state}')
            if task_id in plan_records and plan_records[task_id] != state:
                raise ValueError(f'Conflicting planning states for {task_id}; reconcile the plans')
            plan_records[task_id] = state
            if task_id in tasks:
                tasks[task_id]['planning'] = state
    return {'tasks': tasks, 'completed': completed, 'epics': epics, 'reopened_or_conflicting_ids': overlap}


def delta(current, previous):
    if previous is None:
        return None
    old_open, new_open = set(previous['tasks']), set(current['tasks'])
    old_done, new_done = set(previous['completed']), set(current['completed'])
    return {'completed': sorted(new_done - old_done),
            'added_or_reopened': sorted(new_open - old_open),
            'removed_without_completion': sorted(old_open - new_open - new_done)}


def effort(tasks):
    known = [t['hours'] for t in tasks if t['hours'] is not None]
    lo = sum(pair[0] for pair in known)
    hi = sum(pair[1] for pair in known)
    unknown = sum(t['hours'] is None for t in tasks)
    if not tasks:
        return '—'
    result = f'{lo:g}–{hi:g}h' if known else 'Unestimated'
    if known and unknown:
        result += f' + {unknown} unestimated'
    return result


def render(snapshot, notes):
    tasks = list(snapshot['tasks'].values())
    states = Counter(t['state'] for t in tasks)
    planning = Counter(t['planning'] for t in tasks)
    conditional = states['conditional']
    lines = [f"# Jadpo daily progress — {snapshot['date']}", '',
             f"Updated {snapshot['captured_at']} · figures reflect the recorded roadmap.", '',
             notes['summary'], '', '## At a glance', '',
             '| Measure | Recorded count |', '|---|---:|',
             f"| Completed since task-ID tracking began | {len(snapshot['completed'])} |",
             f'| Remaining open tasks | {len(tasks)} |',
             f'| Of those, conditional / not activated | {conditional} |',
             f"| In progress | {states['in progress']} |",
             f"| Marked ready for work | {states['ready']} |",
             f"| Queued behind dependencies | {states['queued']} |",
             f"| Decision / blocked / external / review | {sum(states[s] for s in ['decision', 'blocked', 'external', 'needs review'])} |", '',
             'Completed counts cover the reorganised roadmap, not all earlier development. '
             'Older capabilities are in the history; there is no reliable whole-project percentage.', '']
    if notes['delivered']:
        lines += ['## Delivered', '']
        lines += ['- ' + item for item in notes['delivered']]
    changes = snapshot['changes']
    lines += ['', '## Since the previous report', '']
    if changes is None:
        lines += ['First snapshot: this establishes the baseline for daily changes. Earlier work is not counted as delivered today.']
    else:
        lines += [f"Compared with {snapshot['previous_date']}: {len(changes['completed'])} newly recorded complete; "
                  f"{len(changes['added_or_reopened'])} added or reopened; "
                  f"{len(changes['removed_without_completion'])} removed/reclassified without completion evidence."]
        if changes['completed']:
            lines += ['', 'Completed IDs: ' + ', '.join(changes['completed']) + '.']
    needs_planning_verb = 'is' if planning['needs planning'] == 1 else 'are'
    lines += ['', '## Planning and implementation', '',
              f"- **{planning['planned']}** open tasks have an explicitly recorded plan; "
              f"**{planning['needs planning']}** {needs_planning_verb} recorded as needing planning and "
              f"**{planning['needs input']}** as needing input for their plan.",
              f"- **{planning['unassessed']}** still need their planning status assessed. Existing design documents may already cover some of them.",
              f'- **{len(tasks) - conditional}** non-conditional tasks remain to deliver; **{conditional}** more are conditional. '
              'This includes coding, decisions, testing and external work—not just implementation.',
              '- A saved plan does not make a task execution-ready; its dependencies still apply.', '',
              '## Remaining work by epic', '',
              '| Epic | Open tasks | Conditional | Estimated effort for non-conditional tasks |',
              '|---|---:|---:|---|']
    for epic, title in snapshot['epics'].items():
        group = [t for t in tasks if t['epic'] == epic]
        active = [t for t in group if t['state'] != 'conditional']
        lines.append(f'| {epic} — {title} | {len(group)} | {len(group) - len(active)} | {effort(active)} |')
    lines += ['', 'Estimates are provisional agent session hours for open tasks, not days or a delivery date. '
              'They are task forecasts, not measured remaining time on partially completed work. '
              'Each epic excludes other epics’ prerequisites; conditional estimates and external waiting are excluded. '
              'Unestimated work remains additional.', '',
              '## Next milestone', '', notes['next'], '', '## Input needed', '', notes['input_needed'], '',
              '[Roadmap](../implementation-roadmap.md) · [Completed evidence](../implementation-history.md) · '
              '[Estimate basis](../task-timing/README.md) · [Reporting rules](README.md)', '']
    if snapshot['reopened_or_conflicting_ids']:
        lines += ['Data note: active/history overlap treated as open: ' + ', '.join(snapshot['reopened_or_conflicting_ids']) + '.', '']
    return '\n'.join(lines)


def atomic_write(path, content):
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as handle:
        handle.write(content)
        temporary = Path(handle.name)
    temporary.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT, help='Repository root; override for fixture tests')
    parser.add_argument('--notes', type=Path, required=True, help='Freshly reviewed JSON: summary, delivered[], next, input_needed')
    args = parser.parse_args()
    root = args.root.resolve()
    notes = json.loads(args.notes.read_text())
    for key in ['summary', 'next', 'input_needed']:
        if not isinstance(notes.get(key), str) or not notes[key].strip():
            parser.error(f'Missing narrative field: {key}')
    if not isinstance(notes.get('delivered'), list) or not all(isinstance(x, str) for x in notes['delivered']):
        parser.error('delivered must be a list of plain-language bullets')
    plan_paths = sorted(p for p in (root / 'docs/work-plans').glob('*.md') if p.name != 'TEMPLATE.md')
    paths = [root / 'docs/implementation-roadmap.md', root / 'docs/implementation-history.md', *plan_paths]
    inputs = {p: p.read_text() for p in paths}
    snapshot = collect(inputs[paths[0]], inputs[paths[1]], {str(p.relative_to(root)): inputs[p] for p in plan_paths})
    now = datetime.now(ZoneInfo('Europe/London'))
    day = now.date().isoformat()
    output = root / 'docs/progress'
    output.mkdir(parents=True, exist_ok=True)
    old_paths = sorted(p for p in output.glob('????-??-??.json') if p.stem < day)
    previous = json.loads(old_paths[-1].read_text()) if old_paths else None
    snapshot.update(schema_version=1, date=day, captured_at=now.isoformat(timespec='seconds'),
                    sources={str(p.relative_to(root)): hashlib.sha256(text.encode()).hexdigest() for p, text in inputs.items()},
                    previous_date=previous['date'] if previous else None, notes=notes)
    snapshot['changes'] = delta(snapshot, previous)
    report = render(snapshot, notes)
    if plan_paths != sorted(p for p in (root / 'docs/work-plans').glob('*.md') if p.name != 'TEMPLATE.md') or any(p.read_text() != text for p, text in inputs.items()):
        parser.error('Source changed during reporting; retry from current records')
    atomic_write(output / f'{day}.json', json.dumps(snapshot, indent=2) + '\n')
    atomic_write(output / f'{day}.md', report)
    atomic_write(output / 'latest.md', report)
    print(output / f'{day}.md')
    print(f"{len(snapshot['completed'])} completed; {len(snapshot['tasks'])} open")


if __name__ == '__main__':
    main()
