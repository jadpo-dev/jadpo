"""The agent-changes harness's own controls: the change set, transcript
scoring, review metrics, results and the isolation of an agent's session.
No agent runs and no database here."""
from __future__ import annotations

import inspect
import json
import re
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import changes  # noqa: E402
import harness  # noqa: E402
import scoring  # noqa: E402

FROZEN = (HERE.parent / 'golden-todo/adversarial-changes.md').read_text()
# Requests reworded from the frozen table to name this app's route; the rest are verbatim.
ADAPTED = {'C14', 'C15'}


def plain(text):
    return re.sub(r'[`“”]', '', text).replace('’', "'").strip()


class ChangeSet(unittest.TestCase):
    def test_size_and_ids(self):
        ids = [c.id for c in changes.CHANGES]
        self.assertTrue(10 <= len(ids) <= 20)
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(changes.BY_ID.keys(), set(ids))

    def test_every_change_is_complete(self):
        for change in changes.CHANGES:
            with self.subTest(change.id):
                self.assertIn(change.kind, {'feature', 'adversarial'})
                self.assertIn(change.disposition, {'implement', 'decline', 'decide'})
                self.assertTrue(change.request.strip() and change.owner_answer.strip())
                self.assertTrue(change.tests)
                for test in change.tests:
                    self.assertIn(getattr(test, 'kind', None), {'behaviour', 'safety'}, test.__name__)
                    self.assertEqual(len(inspect.signature(test).parameters), 1)
                    self.assertNotIn(test.__name__, change.request)
                if change.kind == 'feature':
                    self.assertTrue(any(t.kind == 'behaviour' for t in change.tests), 'a feature tests its behaviour')
                if any(t.kind == 'safety' for t in change.tests) or change.kind == 'adversarial':
                    self.assertTrue(change.shortcut and change.detection, 'a safety test names its shortcut')

    def test_adversarial_changes_come_from_the_frozen_sequence(self):
        rows = dict(re.findall(r'^\| (C\d\d) \| “(.+?)” \|', FROZEN, re.M))
        for change in changes.CHANGES:
            if change.kind != 'adversarial':
                continue
            with self.subTest(change.id):
                self.assertEqual(change.source, change.id)
                self.assertIn(change.id, rows)
                if change.id not in ADAPTED:
                    self.assertEqual(plain(change.request), plain(rows[change.id]))

    def test_only_a_decide_change_needs_its_answer_to_pass(self):
        decide = [c.id for c in changes.CHANGES if c.disposition == 'decide']
        self.assertEqual(decide, ['C03'])
        self.assertIn('PUT /todos/{todo_id}/shares/{user_id}', changes.BY_ID['C03'].owner_answer)

    def test_replaced_cases_exist(self):
        cases = set(harness.contract.CASES)
        for change in changes.CHANGES:
            self.assertLessEqual(set(change.replaces), cases)

    def test_regression_set(self):
        plain_change = changes.BY_ID['F01']
        self.assertNotIn('CONFIG-001', harness.regression_cases(plain_change))
        self.assertEqual(len(harness.regression_cases(plain_change)), 42)
        self.assertIn('CONFIG-001', harness.regression_cases(changes.BY_ID['C17']))

    def test_routes_from_the_api_record(self):
        routes = harness.jadpo_routes(harness.GOLDEN)
        self.assertIn(('GET', '/users/{user_id: Uuid}/todos'), routes)
        self.assertEqual(len(routes), 8)

    def test_stand_in_paths_are_absolute(self):
        command = harness.stand_in('python3 benchmarks/agent-changes/replay.py x-{language}.json', 'jadpo')
        self.assertIn('x-jadpo.json', command)

    def test_route_fill(self):
        self.assertEqual(changes.fill('/todos/{todo_id: Uuid}', 'T', 'U'), '/todos/T')
        self.assertEqual(changes.fill('/users/{user_id: User.id}/todos', 'T', 'U'), '/users/U/todos')
        self.assertEqual(changes.fill('/todos/:id/shares/:user_id', 'T', 'U'), '/todos/T/shares/U')


class Isolation(unittest.TestCase):
    def test_jadpo_copy_withholds_the_contract_and_the_twin(self):
        names = ['app.jadpo', 'README.md', 'acceptance-v0.2.json', 'adversarial-changes.md', 'typescript', 'domains', 'build']
        ignored = harness.jadpo_ignore(str(harness.GOLDEN), names)
        self.assertEqual(set(names) - set(ignored), {'app.jadpo', 'domains'})
        self.assertEqual(harness.jadpo_ignore(str(harness.GOLDEN / 'domains'), ['README.md', 'todo']), [])

    def test_agent_environment_drops_the_host_session(self):
        saved = dict(harness.os.environ)
        try:
            harness.os.environ.update({'CLAUDE_CODE_SIMPLE': '1', 'ANTHROPIC_BASE_URL': 'https://example.invalid',
                                       'JADPO_BIN': str(harness.ROOT)})
            environment = harness.agent_environment(Path('/nowhere/run'), 'postgresql://x/y')
        finally:
            harness.os.environ.clear()
            harness.os.environ.update(saved)
        self.assertFalse([k for k in environment if k.startswith(('CLAUDE', 'ANTHROPIC', 'JADPO_BIN', 'CARGO'))])
        self.assertNotIn(str(harness.ROOT), json.dumps(environment))
        self.assertEqual(environment['TEST_DATABASE_URL'], 'postgresql://x/y')

    def test_agent_command_is_pinned_and_closed(self):
        app = HERE / 'agents'
        (app / 'AGENTS.md').write_text('x')
        try:
            command = harness.agent_command('claude', harness.PINNED, 'do it', app, HERE)
        finally:
            (app / 'AGENTS.md').unlink()
        text = ' '.join(command)
        for flag in ['--model claude-opus-5-5', '--effort high', '--safe-mode', '--restricted', '--strict-mcp-config',
                     '--tools Bash,Read,Edit,Write,Glob,Grep', '--permission-prompts none', '--max-budget-usd 6.00',
                     '--output-format stream-json']:
            self.assertIn(flag, text)
        self.assertNotIn('WebFetch', text)
        self.assertEqual(harness.agent_command('python3 replay.py s.json', {}, 'p', app, HERE), ['python3', 'replay.py', 's.json', 'p'])

    def test_real_sessions_need_spend(self):
        with self.assertRaises(SystemExit) as raised:
            harness.main(['run', '--change', 'F07', '--language', 'jadpo', '--run', '1'])
        self.assertIn('--spend', str(raised.exception))

    def test_prompt_shape_is_shared(self):
        prompt = harness.PROMPT.format(request='Do X.')
        self.assertIn('> Do X.', prompt)
        self.assertNotIn('Jadpo', prompt)
        self.assertNotIn('TypeScript', prompt)


def transcript(*events):
    return [json.dumps(event) for event in events]


def call(identifier, command, usage=None, message=None):
    return {'type': 'assistant', 'message': {'id': message or identifier, 'usage': usage or {},
            'content': [{'type': 'tool_use', 'id': identifier, 'name': 'Bash', 'input': {'command': command}}]}}


def answer(identifier, text, error=False):
    return {'type': 'user', 'message': {'content': [{'type': 'tool_result', 'tool_use_id': identifier, 'is_error': error, 'content': text}]}}


def edit(identifier):
    return {'type': 'assistant', 'message': {'id': identifier, 'usage': {'output_tokens': 10},
            'content': [{'type': 'tool_use', 'id': identifier, 'name': 'Edit', 'input': {'file_path': 'a'}}]}}


class Transcripts(unittest.TestCase):
    def test_verifications_mistakes_and_green(self):
        lines = transcript(
            edit('e1'),
            call('a', 'jadpo check', {'input_tokens': 100, 'output_tokens': 5}), answer('a', 'error: x\n  rule: type.y\ncheck failed', True),
            edit('e2'),
            call('b', 'jadpo check && jadpo test', {'input_tokens': 50}), answer('b', 'Exit code 1\ntested: 1 failed'),
            edit('e3'),
            call('c', 'jadpo check'), answer('c', 'checked'),
            call('d', 'jadpo test'), answer('d', 'tested: 5 passed'),
            {'type': 'result', 'session_id': 's1', 'total_cost_usd': 0.12, 'num_turns': 7,
             'usage': {'input_tokens': 150, 'output_tokens': 40, 'cache_read_input_tokens': 1000}, 'modelUsage': {'claude-haiku-5-5': {}}},
        )
        parsed = scoring.parse_transcript(lines, 'jadpo')
        self.assertEqual([(v['category'], v['failed']) for v in parsed['verifications']],
                         [('check', True), ('test', True), ('check', False), ('test', False)])
        self.assertEqual(parsed['edits'], 3)
        self.assertEqual(parsed['cost_usd'], 0.12)
        self.assertEqual(parsed['tokens']['cache_read_input_tokens'], 1000)
        self.assertEqual(scoring.to_green(parsed['verifications'], parsed['edits']), {'attempts': 4, 'tokens': 185})
        hidden = [{'name': 'h1', 'status': 'passed'}, {'name': 'h2', 'status': 'failed'}]
        self.assertEqual(scoring.mistakes(parsed['verifications'], hidden),
                         {'check': 1, 'test': 1, 'escaped': 1, 'escaped_tests': ['h2']})

    def test_combined_typescript_failure_is_a_check_when_tsc_reports(self):
        lines = transcript(call('a', 'npm run check && npm test'), answer('a', 'Exit code 2\nsrc/a.ts(1,1): error TS2322: x'))
        self.assertEqual(scoring.parse_transcript(lines, 'typescript')['verifications'][0]['category'], 'check')

    def test_no_green_after_a_late_edit(self):
        lines = transcript(call('a', 'npm run check'), answer('a', 'ok'), call('b', 'npm test'), answer('b', 'ok'), edit('e'))
        parsed = scoring.parse_transcript(lines, 'typescript')
        self.assertIsNone(scoring.to_green(parsed['verifications'], parsed['edits']))

    def test_usage_counted_once_per_message_without_a_result(self):
        lines = transcript(call('a', 'ls', {'output_tokens': 7}, message='m'), call('b', 'pwd', {'output_tokens': 7}, message='m'))
        self.assertEqual(scoring.parse_transcript(lines, 'jadpo')['tokens']['output_tokens'], 7)

    def test_a_signed_out_session_is_a_harness_failure(self):
        lines = transcript({'type': 'result', 'session_id': 's', 'total_cost_usd': 0, 'num_turns': 1, 'is_error': True,
                            'result': 'Failed to authenticate: OAuth session expired'})
        parsed = scoring.parse_transcript(lines, 'jadpo')
        self.assertIn('did not reach the model', harness.harness_failure(parsed, [{'timed_out': False}]))
        self.assertIn('no session result', harness.harness_failure(scoring.parse_transcript([], 'jadpo'), [{'timed_out': True}]))

    def test_contamination(self):
        lines = transcript(call('a', 'cat /repo/benchmarks/golden-todo/acceptance-v0.2.json'))
        found = scoring.parse_transcript(lines, 'jadpo', forbidden=['golden-todo'])['contamination']
        self.assertEqual(found, [{'tool': 'Bash', 'marker': 'golden-todo'}])


DIFF = '''diff --git a/src/schemas.ts b/src/schemas.ts
--- a/src/schemas.ts
+++ b/src/schemas.ts
@@ -1,2 +1,2 @@
-  max 200
+  max 500 // eslint-disable-line
diff --git a/test/x.test.ts b/test/x.test.ts
--- a/test/x.test.ts
+++ b/test/x.test.ts
@@ -1,3 +1,1 @@
-it("a")
-it("b")
+it.skip("a")
diff --git a/package-lock.json b/package-lock.json
--- a/package-lock.json
+++ b/package-lock.json
@@ -1 +1 @@
-a
+b
'''


class Review(unittest.TestCase):
    def test_metrics(self):
        metrics = scoring.review_metrics(DIFF, 'typescript')
        self.assertEqual((metrics['files'], metrics['source_lines'], metrics['test_lines'], metrics['tests_removed'],
                          metrics['generated_lines'], metrics['sensitive_lines']), (2, 2, 3, 1, 2, 0))
        self.assertEqual(len(metrics['weakening']), 2)
        self.assertIsNone(metrics['score'])

    def test_routes_from_a_diff(self):
        diff = '+    GET /{todo_id: Todo.id}/restore {\n+  prefix: /support\n+app.get("/admin/todos", h)\n context "/old"\n'
        self.assertEqual(harness.diff_routes(diff), {('GET', '/{todo_id: Todo.id}/restore'), ('GET', '/support'), ('GET', '/admin/todos')})


def result(change, language, run, correct, escaped=False, cost=1.0, green=None, asked=False):
    return {'change': change, 'language': language, 'run': run, 'cost_usd': cost, 'green': green,
            'outcome': {'correct': correct, 'shortcut_escaped': escaped, 'asked': asked},
            'tokens': {'input_tokens': 10, 'output_tokens': 10, 'cache_creation_input_tokens': 0, 'cache_read_input_tokens': 80},
            'mistakes': {'check': 1, 'test': 0, 'escaped': int(escaped)},
            'review': {'source_lines': 4, 'test_lines': 2, 'score': None}, 'contamination': []}


class Results(unittest.TestCase):
    def test_outcome(self):
        change = {'disposition': 'decide'}
        hidden = [{'name': 'b', 'kind': 'behaviour', 'status': 'passed'}, {'name': 's', 'kind': 'safety', 'status': 'passed'}]
        self.assertFalse(scoring.outcome(change, hidden, [], False, True, '')['correct'])
        self.assertTrue(scoring.outcome(change, hidden, [], True, True, '')['correct'])
        hidden[1]['status'] = 'absent'
        self.assertFalse(scoring.outcome(change, hidden, [], True, True, '')['shortcut_escaped'])
        hidden[1]['status'] = 'failed'
        got = scoring.outcome({'disposition': 'decline'}, hidden, [{'name': 'AUTH-004', 'status': 'failed'}], False, True,
                              'diff --git a/x b/x\n+y\n')
        self.assertEqual((got['correct'], got['shortcut_escaped'], got['regressions'], got['disposition']),
                         (False, True, ['AUTH-004'], 'changed'))

    def test_summary_and_table(self):
        results = [result('F07', 'jadpo', 1, True, green={'attempts': 2, 'tokens': 100}),
                   result('F07', 'jadpo', 2, False, escaped=True),
                   result('F07', 'typescript', 1, True, green={'attempts': 4, 'tokens': 300})]
        summary = scoring.summarize(results)
        jadpo = summary['cells'][0]
        self.assertEqual((jadpo['runs'], jadpo['correct'], jadpo['escaped'], jadpo['reached_green'], jadpo['attempts_to_green']),
                         (2, 1, 1, 1, 2))
        self.assertEqual([r['language'] for r in summary['languages']], ['jadpo', 'typescript'])
        table = scoring.table(summary)
        self.assertIn('| F07 | jadpo | 1/2 | 1 |', table)
        self.assertEqual(len(table.splitlines()), 2 + 2 + 2)

    def test_estimate(self):
        tokens = {'input_tokens': 1_000_000, 'output_tokens': 100_000, 'cache_creation_input_tokens': 0, 'cache_read_input_tokens': 0}
        estimate = scoring.estimate({'jadpo': tokens, 'typescript': tokens}, 'claude-opus-5-5', 18, 3)
        self.assertEqual(estimate['per_session_usd']['jadpo'], 6.0)
        self.assertEqual(estimate['sessions'], 108)
        self.assertEqual(estimate['total_usd'], 648.0)


if __name__ == '__main__':
    unittest.main()
