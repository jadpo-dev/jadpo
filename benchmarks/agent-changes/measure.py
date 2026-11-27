#!/usr/bin/env python3
"""A final measurement from start to report, for any helpdesk set and any
models: the measured sessions run after run, and the analyses the report is
written from. The method is in README.md, "A final measurement".

  python3 benchmarks/agent-changes/measure.py sessions --set desk-held4 --frozen <revision> --prefix final1015 \\
      --model gpt-6.1-sol --model gpt-6-luna --model claude-opus-5-5 --runs 3
  python3 benchmarks/agent-changes/measure.py analyse --set desk-held4 --label final1015-6.1-sol --label final1015-6-luna
  python3 benchmarks/agent-changes/measure.py tools --set desk-held4 --label final1015-6.1-sol
  python3 benchmarks/agent-changes/measure.py ask --set desk-held4 --label final1015-6.1-sol --out <folder>
  python3 benchmarks/agent-changes/measure.py debrief --label final1015-6.1-sol --session S11 --language jadpo --run 1 \\
      --changes H52,H53 --out <folder> --spend
  python3 benchmarks/agent-changes/measure.py pilot --label pilot1015-ts --out <folder>
  python3 benchmarks/agent-changes/measure.py brief --set desk-held4 --first H51 --worktree <path> --notes <folder>

Results stay under results/, which git ignores; the report goes in the
results pull request.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import harness  # noqa: E402
import scoring  # noqa: E402

LANGUAGES = harness.LANGUAGES
MIN_FREE_GB = 40
# Codex's weekly limit, as a share of the week, a measurement keeps under,
# and what one run of a model's sessions is taken to use before it starts.
CODEX_WEEK_CAP = 60.0
CODEX_RUN_PERCENT = 2.5
# How long to wait before trying a session again after Claude's usage limit.
CLAUDE_LIMIT_WAIT_SECONDS = 1800
LIMIT = re.compile(r"usage limit|hit your limit|limit reached|rate.?limit|resets? (at|in)|out of (extra )?usage", re.I)


def log_folder():
    return harness.RESULTS / 'measure-logs'


def say(label, text):
    folder = log_folder()
    folder.mkdir(parents=True, exist_ok=True)
    line = time.strftime('%Y-%m-%d %H:%M:%S ') + text
    with (folder / f'{label}.log').open('a') as log:
        log.write(line + '\n')
    print(line, flush=True)


# Sets, sessions and labels ---------------------------------------------------------

def sessions_of(name):
    """The helpdesk sessions a set runs as, in order: every session whose
    changes are all in the set."""
    ids = set(harness.SETS[name])
    found = [session for session, changes in harness.desk_set.SESSIONS.items() if changes and set(changes) <= ids]
    if not found:
        raise SystemExit(f'{name} has no helpdesk sessions')
    return sorted(found, key=lambda session: int(re.sub(r'\D', '', session) or 0))


def agent_for(model):
    return 'claude' if model.startswith('claude-') else 'codex'


def label_for(prefix, model):
    return f'{prefix}-{model.removeprefix("gpt-").removeprefix("claude-")}'


def short_name(model):
    """gpt-6.1-sol is Sol, claude-opus-5-5 is Opus 5.5."""
    rest = re.sub(r'(\d)-(\d)', r'\1.\2', re.sub(r'^(gpt-[\d.]+-|claude-)', '', model))
    return ' '.join(word.capitalize() for word in rest.split('-')) or model


def load(label, change, language, run):
    path = harness.RESULTS / label / f'{harness.run_name(change, language, run)}.json'
    return json.loads(path.read_text()) if path.is_file() else None


def model_of(label):
    for path in sorted((harness.RESULTS / label).glob('*-r*.json')):
        return (json.loads(path.read_text()).get('settings') or {}).get('model')
    return None


# The measured sessions ---------------------------------------------------------------

def on_mains():
    if not shutil.which('pmset'):
        return True
    return 'AC Power' in subprocess.run(['pmset', '-g', 'batt'], capture_output=True, text=True).stdout


def free_gb():
    harness.WORK.mkdir(parents=True, exist_ok=True)
    return shutil.disk_usage(harness.WORK).free / 1e9


def segments():
    if not shutil.which('ipcs'):
        return 0
    out = subprocess.run(['ipcs', '-m'], capture_output=True, text=True).stdout
    return sum(1 for line in out.splitlines() if line.startswith('m '))


def sweep_orphans():
    """Shared memory segments PostgreSQL left with nothing attached."""
    if not shutil.which('ipcs'):
        return 0
    out = subprocess.run(['ipcs', '-m', '-o'], capture_output=True, text=True).stdout
    gone = 0
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 7 and parts[0] == 'm' and parts[4] == os.environ.get('USER') and parts[6] == '0':
            gone += subprocess.run(['ipcrm', '-m', parts[1]], capture_output=True).returncode == 0
    return gone


def rate_limits(paths):
    """The latest Codex rate-limit report among session records: the weekly
    window's share used and when it resets."""
    best = None
    for path in paths:
        try:
            lines = path.read_text(errors='replace').splitlines()
        except OSError:
            continue
        for line in reversed(lines):
            if '"rate_limits"' not in line:
                continue
            try:
                event = json.loads(line)
            except ValueError:
                continue
            limits = ((event.get('payload') or {}).get('rate_limits') or {})
            windows = [w for w in (limits.get('primary'), limits.get('secondary')) if w]
            week = max(windows, key=lambda w: w.get('window_minutes') or 0, default=None)
            if week:
                stamp = event.get('timestamp') or ''
                if best is None or stamp > best['at']:
                    best = {'at': stamp, 'used': float(week.get('used_percent') or 0), 'resets_at': week.get('resets_at')}
            break
    return best


def codex_records(days=8):
    """Codex session records from the last week: the owner's own and those
    the measurement saved."""
    since = time.time() - days * 86400
    roots = [Path.home() / '.codex/sessions', harness.RESULTS]
    found = []
    for root in roots:
        if root.is_dir():
            found += [path for path in root.rglob('*.jsonl') if path.name.startswith(('rollout', 'transcript'))
                      and path.stat().st_mtime >= since]
    return sorted(found, key=lambda path: path.stat().st_mtime)[-40:]


def wait_for_room(label, agent, cap, per_run, min_free):
    """Mains power, free disk and, for Codex, room in the week's usage."""
    while not on_mains():
        say(label, 'waiting for mains power')
        time.sleep(60)
    while free_gb() < min_free:
        say(label, f'waiting for {min_free} GB free ({free_gb():.0f} GB)')
        time.sleep(120)
    while agent == 'codex':
        limits = rate_limits(codex_records())
        if limits is None or limits['used'] + per_run <= cap:
            if limits:
                say(label, f'Codex week {limits["used"]:.0f}% used')
            return
        resets = limits.get('resets_at') or (time.time() + 3600)
        say(label, f'pausing: Codex week {limits["used"]:.0f}% used, a run takes about {per_run}% and the cap is {cap}%; '
                   f'resets {time.strftime("%Y-%m-%d %H:%M", time.localtime(resets))}')
        time.sleep(max(600, min(3600, resets - time.time() + 60)))


def limited(results):
    """Whether a session stopped on Claude's usage limit: a step's session
    ended in an error naming the limit."""
    for data in results:
        for session in (data or {}).get('sessions') or []:
            if session.get('is_error') and LIMIT.search(session.get('text') or ''):
                return True
        if data and data.get('harness_failure') and LIMIT.search(data['harness_failure']):
            return True
    return False


def unreached(data):
    return data is None or (bool(data.get('harness_failure')) and not scoring.total(data.get('tokens') or {}))


def set_aside(label, session, language, run, changes):
    """An interrupted session's results, kept whole beside the label and out
    of its scores."""
    root = harness.RESULTS / label
    name = harness.run_name(session, language, run)
    attempt = 1
    while (root / 'interrupted' / f'{name}-{attempt}').exists():
        attempt += 1
    aside = root / 'interrupted' / f'{name}-{attempt}'
    aside.mkdir(parents=True)
    for change in changes:
        step = harness.run_name(change, language, run)
        for path in (root / f'{step}.json', root / step):
            if path.exists():
                shutil.move(str(path), str(aside / path.name))
    if (root / 'sessions' / name).exists():
        shutil.move(str(root / 'sessions' / name), str(aside / 'session'))
    return aside


def command_sessions(args):
    """Every session of the set in both languages, a run at a time and each
    model in turn within a run, at most --at-once sessions together."""
    sessions = sessions_of(args.set)
    models, runs = args.model, list(range(1, args.runs + 1)) if not args.run else args.run
    languages = args.language or list(LANGUAGES)
    if args.label and len(models) != 1:
        raise SystemExit('--label names one model\'s runs; give one --model')
    if any(agent_for(model) == 'claude' for model in models):
        # Before the first run, not mid-measurement: the pinned Claude Code and its login.
        say('claude', f'Claude Code {harness.CLAUDE_CODE_VERSION} at {harness.claude_code()}')
        harness.claude_token()
    caffeinate = subprocess.Popen(['caffeinate', '-i', '-s', '-w', str(os.getpid())]) if shutil.which('caffeinate') else None
    environment = {key: value for key, value in os.environ.items() if key != 'JADPO_TEST_DATABASE_URL'}
    try:
        for run in runs:
            for model in models:
                label = args.label or label_for(args.prefix, model)
                if not run_cells(args, label, model, run, sessions, languages, environment):
                    say(label, f'stopped at r{run}')
                    return 1
                say(label, f'r{run} finished')
    finally:
        if caffeinate:
            caffeinate.terminate()
    return 0


def run_cells(args, label, model, run, sessions, languages, environment):
    agent = agent_for(model)
    queue = [(session, language) for session in sessions for language in languages
             if not (harness.RESULTS / label / 'sessions' / harness.run_name(session, language, run)).exists()]
    if not queue:
        say(label, f'r{run}: every session already run')
        return True
    wait_for_room(label, agent, args.codex_cap, args.codex_run_percent, args.min_free_gb)
    say(label, f'start rev={args.frozen} model={model} agent={agent} r{run} cells={len(queue)}')
    running, retries, ok = {}, {}, True
    logs = log_folder()
    while queue or running:
        for cell, process in list(running.items()):
            if process.poll() is None:
                continue
            del running[cell]
            session, language = cell
            changes = harness.desk_set.SESSIONS[session]
            data = [load(label, change, language, run) for change in changes]
            say(label, f'done {session}:{language}:r{run} exit={process.returncode} '
                       f'correct={[(d or {}).get("outcome", {}).get("correct") for d in data]} '
                       f'kept={[(d or {}).get("kept", {}).get("correct") for d in data]}')
            shutil.rmtree(harness.WORK / label / harness.run_name(session, language, run), ignore_errors=True)
            if agent == 'claude' and limited(data):
                aside = set_aside(label, session, language, run, changes)
                retries[cell] = retries.get(cell, 0) + 1
                say(label, f'{session}:{language}:r{run} stopped on the Claude usage limit; set aside as '
                           f'{aside.relative_to(harness.RESULTS)}; waiting {args.limit_wait // 60} min, then running it again')
                if retries[cell] > args.limit_retries:
                    say(label, f'stop: {session}:{language}:r{run} hit the limit {retries[cell]} times')
                    ok = False
                    continue
                time.sleep(args.limit_wait)
                queue.insert(0, cell)
                continue
            if unreached(data[0]) or process.returncode != 0:
                say(label, f'stop: {session}:{language}:r{run} never reached the model or failed in the harness')
                ok = False
        if not ok:
            queue = []
        while ok and queue and len(running) < args.at_once:
            free, load_now = free_gb(), os.getloadavg()[0]
            if free < args.min_free_gb or load_now > (os.cpu_count() or 4) * 1.5 or segments() > 24:
                say(label, f'wait: free={free:.0f}GB load={load_now:.1f} segments={segments()}')
                break
            swept = sweep_orphans()
            if swept:
                say(label, f'swept {swept} orphaned shared memory segments')
            session, language = queue.pop(0)
            logs.mkdir(parents=True, exist_ok=True)
            out = (logs / f'{label}-{harness.run_name(session, language, run)}.out').open('a')
            running[(session, language)] = subprocess.Popen(
                [sys.executable, str(HERE / 'harness.py'), 'session', '--agent', agent, '--spend', '--model', model,
                 '--frozen', args.frozen, '--session', session, '--language', language, '--run', str(run), '--label', label],
                cwd=harness.ROOT, env=environment, stdout=out, stderr=subprocess.STDOUT)
            say(label, f'launch {session}:{language}:r{run} free={free:.0f}GB load={load_now:.1f}')
        time.sleep(args.poll)
    return ok


# The analyses ----------------------------------------------------------------------

def k(n):
    return '–' if n is None else f'{n / 1000:.0f}k' if n < 1e6 else f'{n / 1e6:.1f}M'


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def gathered(labels, sessions):
    """Every step result under the labels, by (label, change, language)."""
    out = {}
    for label in labels:
        for session in sessions:
            for change in harness.desk_set.SESSIONS[session]:
                for language in LANGUAGES:
                    for run in (1, 2, 3, 4, 5):
                        data = load(label, change, language, run)
                        if data:
                            out.setdefault((label, change, language), []).append(data)
    return out


def command_analyse(args):
    sessions = sessions_of(args.set)
    labels = args.label
    names = {label: short_name(model_of(label) or label) for label in labels}
    out = gathered(labels, sessions)
    print('## By change: correct at its step out of runs (kept on the final app), Jadpo / TypeScript')
    print('| Change | ' + ' | '.join(names[label] for label in labels) + ' |')
    print('|' + '---|' * (len(labels) + 1))
    for session in sessions:
        for change in harness.desk_set.SESSIONS[session]:
            cells = []
            for label in labels:
                parts = []
                for language in LANGUAGES:
                    runs = out.get((label, change, language), [])
                    correct = sum(bool(d['outcome']['correct']) for d in runs)
                    kept = sum(bool((d.get('kept') or {}).get('correct')) for d in runs)
                    parts.append(f'{correct}/{len(runs)} ({kept})')
                cells.append(' / '.join(parts))
            print(f'| {change} | ' + ' | '.join(cells) + ' |')

    print('\n## Totals per model and language')
    every = []
    for label in labels:
        for language in LANGUAGES:
            runs = [d for (l, _, lang), ds in out.items() if l == label and lang == language for d in ds]
            if not runs:
                continue
            every += runs
            tokens = [d['tokens'] for d in runs if d.get('tokens')]
            print(f'{names[label]} {language}: changes {len(runs)}, correct {sum(bool(d["outcome"]["correct"]) for d in runs)}, '
                  f'kept {sum(bool((d.get("kept") or {}).get("correct")) for d in runs)}, '
                  f'asked {sum(bool(d["outcome"]["asked"]) for d in runs)}, '
                  f'unsafe escaped {sum(bool(d["outcome"]["shortcut_escaped"]) for d in runs)}, '
                  f'per change median fresh in+out {k(median(scoring.fresh(t) for t in tokens))}, '
                  f'cache reads {k(median(t["cache_read_input_tokens"] for t in tokens))}; '
                  f'totals fresh in+out {k(sum(scoring.fresh(t) for t in tokens))}, '
                  f'cache reads {k(sum(t["cache_read_input_tokens"] for t in tokens))}; '
                  f'to green median {k(median((d.get("green") or {}).get("tokens") for d in runs))}; '
                  f'check errors {sum(d["mistakes"]["check"] for d in runs)}, test errors {sum(d["mistakes"]["test"] for d in runs)}, '
                  f'contaminated {sum(bool(d.get("contamination")) for d in runs)}, '
                  f'harness failures {sum(bool(d.get("harness_failure")) for d in runs)}, '
                  f'stream cut-offs {sum(d.get("stream_cutoffs", 0) for d in runs)}')

    print('\n## Per session: correct at its step, kept, fresh in+out, wall minutes')
    for label in labels:
        for run in (1, 2, 3, 4, 5):
            for session in sessions:
                for language in LANGUAGES:
                    steps = [load(label, change, language, run) for change in harness.desk_set.SESSIONS[session]]
                    if not all(steps):
                        continue
                    minutes = sum(t['seconds'] for d in steps for t in d['timings']) / 60
                    print(f'{names[label]} r{run} {session} {language}: {sum(bool(d["outcome"]["correct"]) for d in steps)}/'
                          f'{len(steps)} kept {sum(bool((d.get("kept") or {}).get("correct")) for d in steps)}/{len(steps)}, '
                          f'{k(sum(scoring.fresh(d["tokens"]) for d in steps))}, {minutes:.0f} min, '
                          f'dispositions {[d["outcome"]["disposition"][0] for d in steps]}')

    print('\n## Session medians: fresh in+out, fresh in, out; cache reads and in+out over all sessions')
    for label in labels:
        for language in LANGUAGES:
            rows = []
            for session in sessions:
                for run in (1, 2, 3, 4, 5):
                    tokens = [d['tokens'] for c in harness.desk_set.SESSIONS[session]
                              if (d := load(label, c, language, run)) and d.get('tokens')]
                    if tokens:
                        rows.append((sum(scoring.fresh(t) for t in tokens), sum(t['input_tokens'] for t in tokens),
                                     sum(t['output_tokens'] for t in tokens), sum(t['cache_read_input_tokens'] for t in tokens)))
            if rows:
                print(f'{names[label]} {language}: sessions {len(rows)}, in+out {k(median(r[0] for r in rows))}, '
                      f'in {k(median(r[1] for r in rows))}, out {k(median(r[2] for r in rows))}, '
                      f'cache reads all {k(sum(r[3] for r in rows))}, in+out all {k(sum(r[0] for r in rows))}')

    print('\n## Cost per correct change')
    print(scoring.cost_table(scoring.per_correct([d for d in every if not d.get('harness_failure')])))
    interrupted = [path.name for label in labels for path in sorted((harness.RESULTS / label / 'interrupted').glob('*'))]
    if interrupted:
        print('\nSet aside, interrupted and run again: ' + ', '.join(interrupted))


def calls_of(step):
    path = step / 'calls.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()] if path.is_file() else []


def command_tools(args):
    """How often the Jadpo agents ran `jadpo fix`, `jadpo explain` and `jadpo
    help topic`, per session, beside its failed checks and those that
    offered `jadpo fix`."""
    sessions = sessions_of(args.set)
    for label in args.label:
        folder = harness.RESULTS / label
        print(f'## {label}')
        rows = []
        for session in sessions:
            changes = harness.desk_set.SESSIONS[session]
            for run in (1, 2, 3, 4, 5):
                steps = [folder / harness.run_name(c, 'jadpo', run) for c in changes]
                if not all((folder / f'{s.name}.json').is_file() for s in steps):
                    continue
                calls = [c for s in steps for c in calls_of(s)]

                def verb(call):
                    return (call.get('args') or [''])[0]
                fix = [c for c in calls if verb(c) == 'fix']
                explain = [c for c in calls if verb(c) == 'explain']
                topic = [c for c in calls if verb(c) == 'help' and (c.get('args') or [''])[1:2] == ['topic']]
                checks = [c for c in calls if verb(c) == 'check']
                failed = [c for c in checks if c.get('exit')]
                offered = [c for c in failed if 'jadpo fix' in (c.get('output') or '')]
                tokens = sum(scoring.fresh(d['tokens']) for s in steps
                             if (d := json.loads((folder / f'{s.name}.json').read_text())).get('tokens'))
                first = next((i for i, s in enumerate(steps) if any(verb(c) == 'explain' for c in calls_of(s))), None)
                rows.append((len(fix), len(explain), len(topic), len(failed), len(offered)))
                print(f'{session} r{run}: fix {len(fix)} (exit 0: {sum(not c.get("exit") for c in fix)}), explain {len(explain)} '
                      f'(first at step {first + 1 if first is not None else "-"}), help topic {len(topic)}, '
                      f'checks {len(checks)}, failed {len(failed)}, failed offering fix {len(offered)}, in+out {k(tokens)}')
        if rows:
            print(f'sessions {len(rows)}: with fix {sum(r[0] > 0 for r in rows)}, with explain {sum(r[1] > 0 for r in rows)}, '
                  f'with help topic {sum(r[2] > 0 for r in rows)}; totals fix {sum(r[0] for r in rows)}, '
                  f'explain {sum(r[1] for r in rows)}, topic {sum(r[2] for r in rows)}, failed checks {sum(r[3] for r in rows)}, '
                  f'offering fix {sum(r[4] for r in rows)}\n')


# Asking: what each step did before its first edit ---------------------------------

def text_of(content):
    if isinstance(content, str):
        return content
    return '\n'.join(c.get('text', '') for c in content or [] if isinstance(c, dict))


def first_turn_codex(rollout):
    """The first turn of a step, from Codex's record: tool calls, their
    outputs and messages. Reasoning items are left out."""
    out = []
    for line in rollout.read_text(errors='replace').splitlines():
        payload = json.loads(line).get('payload') or {}
        kind = payload.get('type')
        if kind == 'task_complete':
            break
        if kind in ('custom_tool_call', 'function_call'):
            out.append(('call', payload.get('input') or payload.get('arguments') or ''))
        elif kind in ('custom_tool_call_output', 'function_call_output'):
            out.append(('out', text_of(payload.get('output'))))
        elif kind == 'message' and payload.get('role') == 'assistant':
            out.append(('say', text_of(payload.get('content'))))
    return out


def first_turn_claude(transcript):
    """The first turn of a step, from Claude Code's stream: tool calls as
    `<tool> <input>`, their results and the assistant's text. Thinking blocks
    are left out."""
    out = []
    for event in scoring.events(transcript.read_text(errors='replace').splitlines()):
        if event.get('type') == 'result':
            break
        for block in scoring.blocks(event):
            if event.get('type') == 'assistant' and block.get('type') == 'tool_use':
                out.append(('call', f'{block.get("name")} {json.dumps(block.get("input") or {})}'))
            elif event.get('type') == 'assistant' and block.get('type') == 'text':
                out.append(('say', block.get('text') or ''))
            elif event.get('type') == 'user' and block.get('type') == 'tool_result':
                out.append(('out', scoring.result_text(block)))
    return out


def features(steps):
    """What the agent had done before its first edit, and whether its first
    act was to ask."""
    seen = {'explain': False, 'check': False, 'help': False, 'readme': False, 'acceptance': False, 'docs': set()}
    first = 'none'
    for kind, body in steps:
        if kind != 'call':
            continue
        if 'QUESTIONS.md' in body and re.search(r'Add File|cat >|>\s*QUESTIONS|^Write ', body):
            first = 'ask'
            break
        if re.search(r'apply_patch|Update File|Add File|^(Edit|Write|MultiEdit) ', body):
            first = 'edit'
            break
        seen['explain'] |= bool(re.search(r'jadpo"?\s*,?\s*.?explain|jadpo explain', body))
        seen['help'] |= 'jadpo help' in body
        seen['check'] |= 'jadpo check' in body
        seen['readme'] |= bool(re.search(r'README|PRODUCT\.md', body))
        seen['acceptance'] |= 'acceptance' in body
        seen['docs'] |= set(re.findall(r'(?:\.\./)?docs/[\w./-]+', body))
    seen['docs'] = len(seen['docs'])
    return first, seen


def command_ask(args):
    sessions = sessions_of(args.set)
    out = Path(args.out)
    for label in args.label:
        (out / label).mkdir(parents=True, exist_ok=True)
        for session in sessions:
            for run in (1, 2, 3, 4, 5):
                for language in LANGUAGES:
                    for change in harness.desk_set.SESSIONS[session]:
                        name = harness.run_name(change, language, run)
                        data = load(label, change, language, run)
                        raw = harness.RESULTS / label / name
                        if not data:
                            continue
                        if (raw / 'rollout.jsonl').is_file():
                            steps = first_turn_codex(raw / 'rollout.jsonl')
                        elif (raw / 'transcript.jsonl').is_file():
                            steps = first_turn_claude(raw / 'transcript.jsonl')
                        else:
                            continue
                        first, seen = features(steps)
                        (out / label / f'{name}.txt').write_text('\n\n'.join(
                            f'[{kind}] ' + body[:2500 if kind == 'out' else 1500] for kind, body in steps))
                        print(f'{label} {name}: asked={data["outcome"]["asked"]} correct={data["outcome"]["correct"]} '
                              f'first_action={first} ' + ' '.join(f'{key}={value}' for key, value in seen.items())
                              + f' calls={sum(kind == "call" for kind, _ in steps)}')


# Debriefs: one more turn of a finished session, outside the scores ----------------

DEBRIEF_TOOLS = ('\n9. Did you notice `jadpo fix`, `jadpo explain` and `jadpo help topic`? For each, did you use it, and how '
                 'useful was it?')
DEBRIEF_CHANGE = {
    'jadpo': 'If you could change the language, and separately the development flow (the commands, checks, docs and the '
             'order you use them in), what would you change to make this task faster or safer? Give concrete changes.',
    'typescript': 'If a language and its tools were built for coding agents, what would you want from them, for the '
                  'language and separately for the development flow (the commands, checks, docs and the order you use '
                  'them in), to make this task faster or safer? Give concrete changes.'}


def debrief_prompt(language, about):
    """The debrief's questions: decisions, information and effort, never how
    the model reasoned."""
    tools = DEBRIEF_TOOLS if language == 'jadpo' else ''
    return f'''The task is over and your work has been kept; change nothing. For the people who make the tools you used, answer briefly, about decisions, information and effort only, numbering your answers.
About {about}:
1. What did you know about the rules for this change (who may do it, and what happens in its edge cases) before you changed the code, and where in the project did each piece come from?
2. Was there anything about it you weren't sure of that you chose not to ask the product owner about in QUESTIONS.md? What made you decide it was safe to go ahead?
3. Did anything in the project tell you the rule was already handled, or that it wasn't the owner's to decide? Name the file, command output or instruction.
4. Which file, command output or instruction most affected whether you wrote QUESTIONS.md?
About the whole session, all five changes:
5. What went well?
6. What did you struggle with, and what took the most effort or the most attempts?
7. Which error message, documentation page or command helped most, and which was least useful or misleading?
8. What do you wish the language or tools had told you earlier?{tools}
{10 if tools else 9}. {DEBRIEF_CHANGE[language]}'''


def debrief_codex(folder, records, settings, prompt):
    meta = json.loads(records[0])
    if meta.get('type') != 'session_meta':
        raise SystemExit('the first record is not the session\'s start')
    thread = meta['payload']['id']
    home, login = folder / 'codex-home', Path.home() / '.codex/auth.json'
    home.mkdir(mode=0o700)
    descriptor = os.open(home / 'auth.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'wb') as copy:
        copy.write(login.read_bytes())
    (home / 'config.toml').write_text('\n'.join([
        f'model = {json.dumps(settings["model"])}', f'model_reasoning_effort = {json.dumps(settings["effort"])}',
        'approval_policy = "never"', 'sandbox_mode = "read-only"', 'web_search = "disabled"',
        'cli_auth_credentials_store = "file"', 'check_for_update_on_startup = false', '', '[features]',
        *[f'{feature} = false' for feature in ('shell_tool', 'unified_exec', 'view_image')]]) + '\n')
    stamp = meta['payload']['timestamp'][:19].replace(':', '-')
    day = home / 'sessions' / stamp[:4] / stamp[5:7] / stamp[8:10]
    day.mkdir(parents=True)
    (day / f'rollout-{stamp}-{thread}.jsonl').write_text('\n'.join(records) + '\n')
    environment = {**os.environ, 'CODEX_HOME': str(home), 'HOME': str(folder / 'home'), 'TMPDIR': str(folder / 'tmp')}
    transcript = folder / 'debrief.jsonl'
    try:
        with transcript.open('w') as output, (folder / 'stderr').open('w') as errors:
            subprocess.run([harness.tool('codex'), 'exec', 'resume', '--json', '--skip-git-repo-check', thread, prompt],
                           cwd=folder / 'app', env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                           timeout=600, check=False)
    finally:
        harness.keep_refreshed_login(home / 'auth.json', login)
        shutil.rmtree(home, ignore_errors=True)
    answer, usage = [], {}
    for line in transcript.read_text().splitlines():
        event = json.loads(line)
        item = event.get('item') or {}
        if event.get('type') == 'item.completed' and item.get('type') == 'agent_message':
            answer.append(item.get('text', ''))
        if event.get('type') == 'turn.completed':
            usage = event.get('usage') or {}
    return answer, usage


def debrief_claude(folder, session_record, settings, prompt, identifier, cwd):
    """Resume the session from Claude Code's saved record, cut before the
    harness's own end-of-session debrief, with no tools at all."""
    home = folder / 'claude-home'
    (home / 'projects').mkdir(parents=True, mode=0o700)
    shutil.copytree(session_record, home / 'projects', dirs_exist_ok=True)
    first_debrief = harness.DEBRIEF.split('\n')[0]
    for path in (home / 'projects').rglob(f'{identifier}.jsonl'):
        kept = []
        for line in path.read_text().splitlines():
            if first_debrief in line and '"type":"user"' in line.replace(' ', ''):
                break
            kept.append(line)
        path.write_text('\n'.join(kept) + '\n')
    cwd.mkdir(parents=True, exist_ok=True)
    (folder / CLAUDE_EMPTY).write_text(json.dumps({'permissions': {'deny': ['WebFetch', 'WebSearch']}}) + '\n')
    environment = {'PATH': '/usr/bin:/bin', 'HOME': str(folder / 'home'), 'TMPDIR': str(folder / 'tmp'),
                   'CLAUDE_CONFIG_DIR': str(home), 'CLAUDE_CODE_OAUTH_TOKEN': harness.claude_token(),
                   **harness.CLAUDE_ENVIRONMENT}
    transcript = folder / 'debrief.jsonl'
    try:
        with transcript.open('w') as output, (folder / 'stderr').open('w') as errors:
            subprocess.run([harness.claude_code(), '-p', prompt, '--resume', identifier, '--model', settings['model'],
                            '--effort', settings['effort'], '--output-format', 'stream-json', '--verbose', '--safe-mode',
                            '--restricted', '--strict-mcp-config', '--disable-slash-commands', '--tools', '',
                            '--settings', str(folder / CLAUDE_EMPTY), '--permission-prompts', 'none'],
                           cwd=cwd, env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                           timeout=600, check=False)
    finally:
        shutil.rmtree(home, ignore_errors=True)
    parsed = scoring.parse_transcript(transcript.read_text().splitlines(), 'jadpo')
    return [s['text'] for s in parsed['sessions'] if s['text']], parsed['tokens']


CLAUDE_EMPTY = 'debrief-settings.json'


def command_debrief(args):
    if not args.spend:
        raise SystemExit('a debrief is an agent session; pass --spend')
    results = harness.RESULTS / args.label
    steps = harness.desk_set.SESSIONS[args.session]
    about_ids = args.changes.split(',')
    unknown = [c for c in about_ids if c not in steps]
    if unknown:
        raise SystemExit(f'not in {args.session}: {", ".join(unknown)}')
    name = f'{args.label}-{args.session}-{args.language}-r{args.run}'
    folder = harness.WORK / 'debrief' / name
    shutil.rmtree(folder, ignore_errors=True)
    for sub in ('app', 'home', 'tmp'):
        (folder / sub).mkdir(parents=True)
    first = json.loads((results / f'{harness.run_name(steps[0], args.language, args.run)}.json').read_text())
    settings = first['settings']
    titles = {c: harness.desk_set.BY_ID[c].title for c in steps}
    about = '; '.join(f'change {steps.index(c) + 1}, "{titles[c]}"' for c in about_ids)
    prompt = debrief_prompt(args.language, about)
    record = results / 'sessions' / harness.run_name(args.session, args.language, args.run)
    if (record / 'claude-session').is_dir():
        last = json.loads((results / f'{harness.run_name(steps[-1], args.language, args.run)}.json').read_text())
        identifier = next(s['session_id'] for s in last['sessions'] if s.get('session_id'))
        cwd = harness.WORK / args.label / harness.run_name(args.session, args.language, args.run) / 'app'
        answer, usage = debrief_claude(folder, record / 'claude-session', settings, prompt, identifier, cwd)
    else:
        records = [line for c in steps
                   for line in (results / harness.run_name(c, args.language, args.run) / 'rollout.jsonl').read_text().splitlines()]
        answer, usage = debrief_codex(folder, records, settings, prompt)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / f'{name}.md').write_text(f'# {name}: {about}\n\nUsage: {json.dumps(usage)}\n\n' + '\n\n'.join(answer) + '\n')
    print(f'{name}: {len(answer)} messages, usage {usage}')
    if not answer:
        print((folder / 'stderr').read_text()[-1500:])
        return 1
    shutil.rmtree(folder, ignore_errors=True)
    return 0


# The writer's pilot, and the writer's brief ------------------------------------------

def command_pilot(args):
    """A pilot's results, change by change: correct or not, the hidden tests
    that failed and why, and the agent's diff saved beside them."""
    results, out = harness.RESULTS / args.label, Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for path in sorted(results.glob('H*-r*.json')):
        data = json.loads(path.read_text())
        outcome = data['outcome']
        (out / f'{path.stem}.diff').write_text(data.get('diff') or '')
        print(f'{path.stem}: correct={outcome["correct"]} asked={outcome["asked"]} '
              f'disposition={outcome.get("disposition")} visible_green={outcome.get("visible_green")}')
        for report in data['hidden']:
            if report['status'] != 'passed':
                print(f'    failed {report["name"]}: {report.get("message", "")[:300]}')
        for step in data.get('visible') or []:
            if not step.get('passed'):
                print(f'    visible gate: {step.get("tail", "").strip().splitlines()[-1:]}')


def command_brief(args):
    """The blind writer's brief for the next held-out set, filled in."""
    first = int(args.first.lstrip('H'))
    last = first + 9
    sessions = args.sessions.split(',')
    held = args.held or f'HELD{args.set.removeprefix("desk-held") or "1"}'
    values = {'first': f'H{first}', 'last': f'H{last}', 'before': f'H{first - 1}', 'set': args.set, 'held': held,
              'session_a': sessions[0], 'session_b': sessions[1], 'mid': f'H{first + 4}', 'mid_next': f'H{first + 5}',
              'worktree': args.worktree, 'notes': args.notes, 'tools': str(Path(args.notes).parent), 'tag': args.tag,
              'script': str(HERE.relative_to(harness.ROOT) / 'measure.py')}
    text = (HERE / 'writer-brief.md').read_text()
    for key, value in values.items():
        text = text.replace('{{' + key + '}}', value)
    left = re.findall(r'\{\{\w+\}\}', text)
    if left:
        raise SystemExit('unfilled: ' + ', '.join(sorted(set(left))))
    print(text)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    sub = commands.add_parser('sessions', help="the measured sessions, a run at a time, each model in turn")
    sub.add_argument('--set', required=True, choices=sorted(harness.SETS))
    sub.add_argument('--frozen', required=True, metavar='REVISION')
    sub.add_argument('--model', action='append', required=True, help='in the order each run takes them')
    sub.add_argument('--prefix', default='final', help='labels are <prefix>-<model>')
    sub.add_argument('--label', help='one model\'s label, instead of <prefix>-<model>')
    sub.add_argument('--runs', type=int, default=3)
    sub.add_argument('--run', type=int, action='append', help='only these runs')
    sub.add_argument('--language', action='append', choices=LANGUAGES)
    sub.add_argument('--at-once', dest='at_once', type=int, default=4)
    sub.add_argument('--min-free-gb', dest='min_free_gb', type=float, default=MIN_FREE_GB)
    sub.add_argument('--codex-cap', dest='codex_cap', type=float, default=CODEX_WEEK_CAP,
                     help='pause before a Codex run that would take the week past this share')
    sub.add_argument('--codex-run-percent', dest='codex_run_percent', type=float, default=CODEX_RUN_PERCENT)
    sub.add_argument('--limit-wait', dest='limit_wait', type=int, default=CLAUDE_LIMIT_WAIT_SECONDS)
    sub.add_argument('--limit-retries', dest='limit_retries', type=int, default=12)
    sub.add_argument('--poll', type=int, default=30)
    sub.set_defaults(handler=command_sessions)
    for name, handler in [('analyse', command_analyse), ('tools', command_tools), ('ask', command_ask)]:
        sub = commands.add_parser(name)
        sub.add_argument('--set', required=True, choices=sorted(harness.SETS))
        sub.add_argument('--label', action='append', required=True)
        if name == 'ask':
            sub.add_argument('--out', required=True)
        sub.set_defaults(handler=handler)
    sub = commands.add_parser('debrief', help='one more turn of a finished session, outside the scores')
    sub.add_argument('--label', required=True)
    sub.add_argument('--session', required=True)
    sub.add_argument('--language', required=True, choices=LANGUAGES)
    sub.add_argument('--run', type=int, required=True)
    sub.add_argument('--changes', required=True, help='the changes to ask about, such as H52,H53')
    sub.add_argument('--out', required=True)
    sub.add_argument('--spend', action='store_true')
    sub.set_defaults(handler=command_debrief)
    sub = commands.add_parser('pilot', help="a writer's pilot, change by change")
    sub.add_argument('--label', required=True)
    sub.add_argument('--out', required=True)
    sub.set_defaults(handler=command_pilot)
    sub = commands.add_parser('brief', help="the blind writer's brief for the next held-out set")
    sub.add_argument('--set', required=True, help='the new set, such as desk-held4')
    sub.add_argument('--first', required=True, help='its first change, such as H51')
    sub.add_argument('--sessions', default=None, help='its two sessions, such as S11,S12')
    sub.add_argument('--held', help='its name in desk_changes.py, such as HELD4')
    sub.add_argument('--tag', required=True, help='the labels\' suffix, such as 1015')
    sub.add_argument('--worktree', required=True)
    sub.add_argument('--notes', required=True)
    sub.set_defaults(handler=command_brief)
    args = parser.parse_args(argv)
    if args.command == 'brief' and not args.sessions:
        number = (int(args.first.lstrip('H')) - 1) // 5 + 1
        args.sessions = f'S{number},S{number + 1}'
    return args.handler(args) or 0


if __name__ == '__main__':
    raise SystemExit(main())
