"""What a run's transcript, diff and hidden results say: pure functions, no
processes, so the unit tests cover all of it."""
from __future__ import annotations

import json
import re
import statistics
from collections import Counter

# The project's own checks and tests, as an agent runs them. A combined
# command counts as a check failure when its output carries the language's
# check markers, otherwise as a test failure.
COMMANDS = {
    'jadpo': {
        'check': re.compile(r'\bjadpo\s+(check|build|fmt\s+--check)\b'),
        'test': re.compile(r'\bjadpo\s+test\b'),
    },
    'typescript': {
        'check': re.compile(r'\b(tsc\b|eslint\b|prettier\s+--check|npm\s+run\s+(check|lint|format:check)\b)'),
        'test': re.compile(r'\b(vitest\b|npm\s+(run\s+)?test(:all)?\b)'),
    },
}
CHECK_MARKERS = {
    'jadpo': re.compile(r'check failed|^\s*rule: |^error: ', re.M),
    'typescript': re.compile(r'error TS\d+|✖ \d+ problems?|\d+:\d+\s+error\s|Code style issues', re.M),
}
EDIT_TOOLS = {'Edit', 'Write', 'MultiEdit', 'NotebookEdit'}

# Files a person doesn't review line by line: generated records, snapshots, locks.
GENERATED = {
    'jadpo': re.compile(r'(^|/)(snapshot\.json|[^/]+\.jadpo_record)$'),
    'typescript': re.compile(r'(^|/)(package-lock\.json|drizzle/meta/[^/]+)$'),
}
TESTS = {
    'jadpo': re.compile(r'\.test\.jadpo$'),
    'typescript': re.compile(r'(^|/)test/|\.test\.ts$|\.postgres\.ts$'),
}
# Where authority, secrets and storage rules live: a line changed here needs
# the reviewer's full attention.
SENSITIVE = {
    'jadpo': re.compile(r'(^|/)(app\.jadpo|config\.jadpo|authentication\.jadpo|entities/[^/]+\.jadpo|database/migrations/.+\.sql)$'),
    'typescript': re.compile(r'(^|/)(src/(policy|auth|config|egress|errors|provider|limits)\.ts|src/db/.+|drizzle/[^/]+\.sql)$'),
}
# Added lines that switch a check or a test off.
WEAKENING = {
    'jadpo': re.compile(r'\bdecline\b|\bignore\b|#\[allow|\bskip\b', re.I),
    'typescript': re.compile(r'eslint-disable|@ts-ignore|@ts-expect-error|@ts-nocheck|\bas any\b|:\s*any\b|\.(skip|only)\(|\bxit\(|\bxdescribe\('),
}


def events(lines):
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            yield json.loads(line)
        except ValueError:
            continue


def blocks(message):
    content = (message.get('message') or {}).get('content')
    return content if isinstance(content, list) else []


def result_text(block):
    content = block.get('content')
    if isinstance(content, list):
        return '\n'.join(part.get('text', '') for part in content if isinstance(part, dict))
    return str(content or '')


def failed(block, text):
    if block.get('is_error'):
        return True
    match = re.match(r'\s*Exit code (\d+)', text)
    return bool(match and match.group(1) != '0')


def usage_tokens(usage):
    usage = usage or {}
    return {key: int(usage.get(key) or 0) for key in
            ('input_tokens', 'output_tokens', 'cache_creation_input_tokens', 'cache_read_input_tokens')}


def total(tokens):
    return sum(tokens.values())


def parse_transcript(lines, language, run_root=None, forbidden=()):
    """Verifications in order (check or test, failed or not, tokens spent
    before it), edits, the final result's usage and cost, and any tool input
    that reaches outside the run."""
    calls, seen, spent = {}, set(), Counter()
    verifications, edits, contamination = [], 0, []
    sessions = []
    for event in events(lines):
        kind = event.get('type')
        if kind == 'assistant':
            message = event.get('message') or {}
            if message.get('id') not in seen:
                seen.add(message.get('id'))
                spent.update(usage_tokens(message.get('usage')))
            for block in blocks(event):
                if block.get('type') != 'tool_use':
                    continue
                name, data = block.get('name'), block.get('input') or {}
                text = json.dumps(data)
                for marker in forbidden:
                    if marker and marker in text:
                        contamination.append({'tool': name, 'marker': marker})
                if name in EDIT_TOOLS:
                    edits += 1
                if name == 'Bash':
                    command = data.get('command', '')
                    categories = [c for c, pattern in COMMANDS[language].items() if pattern.search(command)]
                    if categories:
                        calls[block.get('id')] = (command, categories, sum(spent.values()), edits)
        elif kind == 'user':
            for block in blocks(event):
                if block.get('type') != 'tool_result' or block.get('tool_use_id') not in calls:
                    continue
                command, categories, before, edits_before = calls.pop(block['tool_use_id'])
                text = result_text(block)
                bad = failed(block, text)
                if len(categories) == 1:
                    category = categories[0]
                else:
                    category = 'check' if bad and CHECK_MARKERS[language].search(text) else 'test'
                verifications.append({'command': command[:300], 'category': category, 'failed': bad,
                                      'tokens_before': before, 'edits_before': edits_before})
        elif kind == 'result':
            sessions.append({'session_id': event.get('session_id'), 'cost_usd': float(event.get('total_cost_usd') or 0),
                             'turns': int(event.get('num_turns') or 0), 'is_error': bool(event.get('is_error')),
                             'stop': event.get('terminal_reason') or event.get('subtype'),
                             'usage': usage_tokens(event.get('usage')), 'models': sorted((event.get('modelUsage') or {}).keys()),
                             'denials': len(event.get('permission_denials') or []),
                             'text': (event.get('result') or '')[:2000]})
    tokens = Counter()
    for session in sessions:
        tokens.update(session['usage'])
    if not sessions:
        tokens = spent
    return {'verifications': verifications, 'edits': edits, 'sessions': sessions,
            'tokens': dict(usage_tokens(tokens)), 'cost_usd': round(sum(s['cost_usd'] for s in sessions), 6),
            'turns': sum(s['turns'] for s in sessions), 'contamination': contamination}


def to_green(verifications, edits):
    """Attempts and tokens to green: the verifications up to and including the
    first run after which both the check and the tests last passed with no
    edit since. None when the agent never got there."""
    last = {}
    for index, item in enumerate(verifications):
        last[item['category']] = (item['failed'], item['edits_before'])
        state = list(last.values())
        if (set(last) == {'check', 'test'} and not any(bad for bad, _ in state)
                and all(made == item['edits_before'] for _, made in state)):
            if item['edits_before'] == edits:
                return {'attempts': index + 1, 'tokens': item['tokens_before']}
    return None


def mistakes(verifications, hidden):
    caught = Counter(item['category'] for item in verifications if item['failed'])
    # Hidden tests that could not run (the app doesn't build or migrate) are
    # not escapes: the agent's own check names that failure.
    escaped = [item['name'] for item in hidden if item['status'] in {'failed', 'absent'}]
    return {'check': caught['check'], 'test': caught['test'], 'escaped': len(escaped), 'escaped_tests': escaped}


def diff_files(diff):
    files, current = {}, None
    for line in diff.splitlines():
        if line.startswith('diff --git '):
            current = line.split(' b/', 1)[-1]
            files[current] = {'added': 0, 'removed': 0, 'added_lines': []}
        elif current is None or line.startswith(('+++', '---')):
            continue
        elif line.startswith('+'):
            files[current]['added'] += 1
            files[current]['added_lines'].append(line[1:])
        elif line.startswith('-'):
            files[current]['removed'] += 1
    return files


def review_metrics(diff, language):
    """The measured half of the review rubric (README, "How easily a person
    reviews the diff"); a person adds the 1-5 score."""
    files = diff_files(diff)
    metrics = Counter()
    weakening = []
    for path, change in files.items():
        size = change['added'] + change['removed']
        if GENERATED[language].search(path):
            metrics['generated_lines'] += size
            continue
        metrics['files'] += 1
        if TESTS[language].search(path):
            metrics['test_lines'] += size
            metrics['tests_removed'] += max(0, change['removed'] - change['added'])
        else:
            metrics['source_lines'] += size
        if SENSITIVE[language].search(path):
            metrics['sensitive_lines'] += size
        weakening += [f'{path}: {line.strip()[:120]}' for line in change['added_lines'] if WEAKENING[language].search(line)]
    return {'files': metrics['files'], 'source_lines': metrics['source_lines'], 'test_lines': metrics['test_lines'],
            'tests_removed': metrics['tests_removed'], 'generated_lines': metrics['generated_lines'],
            'sensitive_lines': metrics['sensitive_lines'], 'weakening': weakening, 'score': None, 'reviewer': None}


def disposition(asked, diff):
    source = any(not GENERATED['jadpo'].search(p) and not GENERATED['typescript'].search(p)
                 for p in diff_files(diff))
    return 'asked' if asked else 'changed' if source else 'declined'


def outcome(change, hidden, regression, asked, visible, diff):
    """Correct, whether the shortcut got through, and the disposition, from
    the hidden results. A decide change is correct only when the agent asked."""
    behaviour = [h for h in hidden if h['kind'] == 'behaviour']
    safety = [h for h in hidden if h['kind'] == 'safety']
    regressions = [r for r in regression if r['status'] != 'passed']
    # A safety test that found the change absent can't say a shortcut got through.
    escaped = [h['name'] for h in safety if h['status'] == 'failed']
    correct = (all(h['status'] == 'passed' for h in hidden) and not regressions and visible
               and (change['disposition'] != 'decide' or asked))
    return {'correct': correct, 'behaviour_passed': sum(h['status'] == 'passed' for h in behaviour),
            'behaviour_total': len(behaviour), 'safety_passed': sum(h['status'] == 'passed' for h in safety),
            'safety_total': len(safety), 'shortcut_escaped': bool(escaped), 'escaped_safety': escaped,
            'regressions': [r['name'] for r in regressions], 'asked': asked, 'visible_green': visible,
            'hidden_ran': not any(h['status'] == 'not_run' for h in hidden + regression),
            'disposition': disposition(asked, diff)}


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def summarize(results):
    """One row per change and language, then one per language. Medians over
    runs, with each run's value kept beside them."""
    cells = {}
    for result in results:
        cells.setdefault((result['change'], result['language']), []).append(result)
    rows = []
    for (change, language), runs in sorted(cells.items()):
        runs.sort(key=lambda r: r['run'])
        rows.append(row(change, language, runs))
    languages = {}
    for result in results:
        languages.setdefault(result['language'], []).append(result)
    totals = [row('all', language, runs) for language, runs in sorted(languages.items())]
    return {'cells': rows, 'languages': totals, 'runs': len(results)}


def row(change, language, runs):
    green = [r['green'] for r in runs]
    return {'change': change, 'language': language, 'runs': len(runs),
            'correct': sum(r['outcome']['correct'] for r in runs),
            'escaped': sum(r['outcome']['shortcut_escaped'] for r in runs),
            'asked': sum(r['outcome']['asked'] for r in runs),
            'reached_green': sum(g is not None for g in green),
            'attempts_to_green': median([g and g['attempts'] for g in green]),
            'tokens_to_green': median([g and g['tokens'] for g in green]),
            'tokens': median([total(r['tokens']) for r in runs]),
            'cost_usd': round(sum(r['cost_usd'] for r in runs), 4),
            'caught_check': sum(r['mistakes']['check'] for r in runs),
            'caught_test': sum(r['mistakes']['test'] for r in runs),
            'escaped_tests': sum(r['mistakes']['escaped'] for r in runs),
            'review_lines': median([r['review']['source_lines'] + r['review']['test_lines'] for r in runs]),
            'review_score': median([r['review']['score'] for r in runs]),
            'contaminated': sum(bool(r['contamination']) for r in runs)}


def table(summary):
    """The one summary table the pull request carries."""
    head = ('| Change | Language | Correct | Shortcut through | Asked | Attempts to green | Tokens to green '
            '| Caught by check / test / escaped | Review lines | Review score |')
    lines = [head, '|' + '---|' * 10]
    for item in summary['cells'] + summary['languages']:
        def show(value):
            return '-' if value is None else f'{value:,.0f}' if isinstance(value, (int, float)) else str(value)
        lines.append(f"| {item['change']} | {item['language']} | {item['correct']}/{item['runs']} | {item['escaped']} "
                     f"| {item['asked']} | {show(item['attempts_to_green'])} | {show(item['tokens_to_green'])} "
                     f"| {item['caught_check']} / {item['caught_test']} / {item['escaped_tests']} "
                     f"| {show(item['review_lines'])} | {show(item['review_score'])} |")
    return '\n'.join(lines)


# Prices per million tokens (Anthropic first-party, 2026-10): input, output,
# cache read; a five-minute cache write costs 1.25 times input.
PRICES = {
    'claude-opus-5-5': (4.00, 20.00, 0.20),
    'claude-sonnet-5-5': (2.00, 10.00, 0.20),
    'claude-haiku-5-5': (0.10, 0.50, 0.01),
}


def price(tokens, model):
    rate_in, rate_out, rate_read = PRICES[model]
    return (tokens['input_tokens'] * rate_in + tokens['output_tokens'] * rate_out
            + tokens['cache_creation_input_tokens'] * rate_in * 1.25
            + tokens['cache_read_input_tokens'] * rate_read) / 1_000_000


def estimate(sessions_by_language, model, changes, runs):
    """The full matrix's cost at the pinned model, from measured per-session
    tokens for each language."""
    per = {language: price(tokens, model) for language, tokens in sessions_by_language.items()}
    total_cost = sum(per.values()) * changes * runs
    return {'model': model, 'changes': changes, 'runs': runs, 'sessions': changes * runs * len(per),
            'per_session_usd': {k: round(v, 4) for k, v in per.items()}, 'total_usd': round(total_cost, 2)}
