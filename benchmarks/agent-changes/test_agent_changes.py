"""The agent-changes harness's own controls: the change set, transcript
scoring, review metrics, results and the isolation of an agent's session.
No agent runs and no database here."""
from __future__ import annotations

import inspect
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import changes
import desk_changes  # noqa: E402
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
        self.assertEqual(len(ids) - len(changes.FRESH), 18)
        self.assertTrue(10 <= len(changes.FRESH) <= 20)
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
            if change.kind != 'adversarial' or change.id in changes.FRESH:
                continue
            with self.subTest(change.id):
                self.assertEqual(change.source, change.id)
                self.assertIn(change.id, rows)
                if change.id not in ADAPTED:
                    self.assertEqual(plain(change.request), plain(rows[change.id]))

    def test_fresh_changes_are_new(self):
        # 10.4's set isn't drawn from the frozen sequence, and every one has a shortcut to catch.
        for change_id in changes.FRESH:
            change = changes.BY_ID[change_id]
            with self.subTest(change_id):
                self.assertIsNone(change.source)
                self.assertTrue(change.shortcut and any(t.kind == 'safety' for t in change.tests))

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

    def test_typescript_reaches_its_socket_the_way_its_client_reads_one(self):
        # postgres.js takes a hostname literally, so the folder goes in
        # ?host=, the form the app's own adapter writes.
        from urllib.parse import parse_qs, urlsplit
        server = object.__new__(harness.Server)
        server.socket, server.port = Path('/runs/held/F02-typescript-r1/sock'), 5999
        jadpo = urlsplit(server.socket_url('agent_1'))
        typescript = urlsplit(server.socket_url('agent_1', 'typescript'))
        self.assertEqual(jadpo.netloc, 'golden@%2Fruns%2Fheld%2FF02-typescript-r1%2Fsock:5999')
        self.assertEqual((typescript.hostname, typescript.port, typescript.path), ('localhost', 5999, '/agent_1'))
        self.assertEqual(parse_qs(typescript.query), {'host': ['/runs/held/F02-typescript-r1/sock']})

    def test_a_sandboxed_run_is_told_loopback_is_closed(self):
        closed = harness.agent_environment(Path('/nowhere/run'), 'postgresql://x/y', loopback=False)
        self.assertEqual(closed['TEST_LOOPBACK'], 'unavailable')
        self.assertNotIn('TEST_LOOPBACK', harness.agent_environment(Path('/nowhere/run'), 'postgresql://x/y'))
        suite = (harness.TYPESCRIPT / 'test/postgres.test.ts').read_text()
        # Skipped as it runs, so `vitest list` (the adapter's count) still has it.
        self.assertRegex(suite, r'"enforces HTTP decoding boundaries over a real socket",\s*async \(context\) => \{[^}]*'
                                r'if \(process\.env\.TEST_LOOPBACK === "unavailable"\) context\.skip\(\);')

    def test_npm_never_asks_the_registry_for_an_update(self):
        # A run's home is fresh, so npm's update check fires on its first
        # command, and the sandbox kills a command that reaches for the network.
        environment = harness.agent_environment(Path('/nowhere/run'), 'postgresql://x/y')
        self.assertEqual(environment['NPM_CONFIG_UPDATE_NOTIFIER'], 'false')

    def test_contamination_markers_name_the_repository_not_the_app(self):
        # Both apps use `golden-todo` themselves (an OIDC audience, a package
        # name), so an agent copying a test line was marked contaminated.
        def marked(text):
            return [marker for marker in harness.contamination_markers() if marker in text]
        self.assertEqual(marked('OIDC_AUDIENCE: "golden-todo",'), [])
        self.assertEqual(marked('cat ../../benchmarks/golden-todo/expected-audit.md'), ['benchmarks/golden-todo'])
        self.assertEqual(marked("rg --files .. -g 'acceptance-v0.2.json'"), ['acceptance-v0.2'])

    def test_typescript_is_told_its_database_is_provided(self):
        # `jadpo test` finds the provided server itself; TypeScript's own docs
        # say its runner starts a server, which the sandbox refuses.
        for name in ['JADPO_TEST_DATABASE_URL', 'TEST_DATABASE_URL', 'Unix socket', "can't start"]:
            self.assertIn(name, harness.TYPESCRIPT_SANDBOX_NOTE)
        source = inspect.getsource(harness.prepare)
        self.assertIn('TYPESCRIPT_SANDBOX_NOTE', source)

    def test_agent_command_is_pinned_and_closed(self):
        with tempfile.TemporaryDirectory() as scratch:
            app = Path(scratch)
            (app / 'AGENTS.md').write_text('x')
            # CI has no `claude`; the command's shape doesn't depend on where it is.
            with mock.patch.object(harness, 'tool', lambda name: name):
                command = harness.agent_command('claude', harness.PINNED, 'do it', app, HERE)
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
        prompt = harness.PROMPT.format(app='todo', request='Do X.')
        self.assertIn('> Do X.', prompt)
        self.assertNotIn('Jadpo', prompt)
        self.assertNotIn('TypeScript', prompt)

    def test_only_tuning_runs_ask_for_notes(self):
        tuning, held = changes.BY_ID[harness.TUNING[0]], changes.BY_ID[harness.SETS['held-out'][0]]
        self.assertIn('THINKING.md', harness.task_prompt(tuning))
        self.assertIn('not a deliverable', harness.task_prompt(tuning))
        self.assertEqual(harness.task_prompt(held), harness.PROMPT.format(app='todo', request=held.request))
        self.assertIn('the helpdesk application', harness.task_prompt(desk_changes.BY_ID['H01']))

    def test_notes_are_not_part_of_the_diff(self):
        with tempfile.TemporaryDirectory() as scratch:
            app = Path(scratch)
            harness.git(app, 'init', '-q', '-b', 'main')
            (app / 'a.txt').write_text('a\n')
            harness.git(app, 'add', '-A')
            harness.git(app, '-c', 'user.name=t', '-c', 'user.email=t@example.invalid', 'commit', '-q', '-m', 's')
            (app / 'a.txt').write_text('b\n')
            (app / 'THINKING.md').write_text('trying b\n')
            diff = harness.final_diff(app)
        self.assertIn('a.txt', diff)
        self.assertNotIn('THINKING', diff)


class SideBySide(unittest.TestCase):
    def test_each_run_keeps_its_own_native_driver(self):
        with tempfile.TemporaryDirectory() as scratch:
            shared = Path(scratch) / 'target/debug/golden-native-contract'
            shared.parent.mkdir(parents=True)
            work = Path(scratch) / 'run/contract'
            work.mkdir(parents=True)

            def build():
                shared.write_text('driver for this run')
                return shared

            with mock.patch.object(harness, 'ROOT', Path(scratch)):
                own = harness.own_driver(build, work)
            self.assertEqual(own, work / 'golden-native-contract')
            shared.write_text('driver for the run beside it')
            self.assertEqual(own.read_text(), 'driver for this run')
            self.assertTrue((Path(scratch) / 'target/agent-changes/native.lock').exists())


    def test_a_typescript_install_is_checked_and_retried_once(self):
        # npm ci has exited 0 with node_modules missing files; the pristine
        # app's own typecheck is what shows it.
        def runner(outcomes):
            calls = []

            def run(command, **_):
                calls.append(command[:2])
                ok = outcomes.pop(0) if command[:2] == ['npm', 'run'] else True
                return subprocess.CompletedProcess(command, 0 if ok else 2, '', '')
            return calls, run

        with tempfile.TemporaryDirectory() as scratch, mock.patch.object(harness, 'ROOT', Path(scratch)):
            calls, run = runner([False, True])
            harness.install_typescript(Path(scratch), run)
            self.assertEqual(calls, [['npm', 'ci'], ['npm', 'run'], ['npm', 'ci'], ['npm', 'run']])
            self.assertTrue((Path(scratch) / 'target/agent-changes/npm.lock').exists())
            calls, run = runner([False, False])
            with self.assertRaisesRegex(RuntimeError, 'incomplete'):
                harness.install_typescript(Path(scratch), run)


class Sets(unittest.TestCase):
    def test_six_tuning_changes_span_the_kinds(self):
        tuning = [changes.BY_ID[c] for c in harness.TUNING]
        self.assertEqual(len(tuning), 6)
        self.assertTrue(any(c.kind == 'adversarial' for c in tuning))
        self.assertTrue(any(c.disposition == 'decide' for c in tuning))
        self.assertTrue(any(c.kind == 'feature' for c in tuning))
        self.assertEqual(len(harness.SETS['held-out']), 12)
        self.assertEqual(harness.SETS['fresh'], changes.FRESH)
        sets = [set(ids) for name, ids in harness.SETS.items() if name != 'desk']
        self.assertEqual(sum(map(len, sets)), len(changes.CHANGES))
        self.assertEqual(set().union(*sets), set(changes.BY_ID))
        self.assertEqual(harness.set_of('N01'), 'fresh')

    def test_held_out_needs_a_frozen_jadpo(self):
        held = harness.SETS['held-out'][0]
        for argv in (['run', '--spend', '--agent', 'codex', '--set', 'held-out'],
                     ['run', '--spend', '--agent', 'codex', '--change', held],
                     ['run', '--spend', '--agent', 'codex', '--change', held, '--frozen', 'no-such-revision']):
            with self.subTest(argv), self.assertRaises(SystemExit) as raised:
                harness.main(argv)
            self.assertIn('frozen', str(raised.exception))
        with self.assertRaises(SystemExit) as raised:
            harness.main(['run', '--spend', '--agent', 'codex', '--set', 'tuning', '--change', held])
        self.assertIn('not in the tuning set', str(raised.exception))
        # Tuning changes need no frozen revision; the revision checked out is accepted for held-out ones.
        harness.check_frozen([changes.BY_ID[harness.TUNING[0]]], None)
        harness.check_frozen([changes.BY_ID[held]], harness.git(harness.ROOT, 'rev-parse', 'HEAD').strip())


class Helpdesk(unittest.TestCase):
    def test_each_change_runs_once_in_a_session_of_five(self):
        ids = [i for session in desk_changes.SESSIONS.values() for i in session]
        self.assertEqual(sorted(ids), sorted(desk_changes.BY_ID))
        self.assertTrue(all(len(s) == 5 for s in desk_changes.SESSIONS.values()))
        self.assertEqual(harness.set_of('H01'), 'desk')
        self.assertEqual(harness.app_of(desk_changes.BY_ID['H01']), 'desk')
        self.assertEqual(harness.app_of(changes.BY_ID['F01']), 'todo')
        self.assertFalse(set(desk_changes.BY_ID) & set(changes.BY_ID))

    def test_every_change_checks_the_rules_every_change_keeps(self):
        for change in desk_changes.CHANGES:
            with self.subTest(change.id):
                self.assertTrue(set(desk_changes.PROBES) <= set(change.tests))
                self.assertTrue(any(t.kind == 'behaviour' for t in change.tests))
                self.assertTrue(any(t.kind == 'safety' and t not in desk_changes.PROBES for t in change.tests))
                self.assertTrue(change.shortcut and change.detection and change.owner_answer)
                self.assertEqual(harness.regression_cases(change), [])

    def test_jadpo_copy_withholds_the_twin_and_keeps_the_product(self):
        names = ['app.jadpo', 'README.md', 'typescript', 'domains', 'build']
        ignored = harness.jadpo_ignore(str(harness.HELPDESK), names, 'desk')
        self.assertEqual(set(names) - set(ignored), {'app.jadpo', 'README.md', 'domains'})
        readme = (harness.HELPDESK / 'README.md').read_text()
        self.assertNotIn('TypeScript', readme)
        self.assertNotIn('Jadpo', readme)

    def test_route_fill_uses_acme_ids(self):
        self.assertEqual(desk_changes.fill('/tickets/{ticket_id}/tags/{tag_id}'),
                         f'/tickets/{desk_changes.T_OPEN}/tags/{desk_changes.TAG_BILLING}')
        self.assertEqual(desk_changes.fill('/workspaces/{id}/reports'), f'/workspaces/{desk_changes.ACME}/reports')
        self.assertEqual(desk_changes.fill('/customers/:id/merge'), f'/customers/{desk_changes.CORA}/merge')

    def test_sessions_need_spend_and_a_frozen_jadpo(self):
        with self.assertRaises(SystemExit) as raised:
            harness.main(['session', '--session', 'S1', '--agent', 'codex'])
        self.assertIn('--spend', str(raised.exception))
        with self.assertRaises(SystemExit) as raised:
            harness.main(['session', '--session', 'S1', '--agent', 'codex', '--spend'])
        self.assertIn('frozen', str(raised.exception))
        with self.assertRaises(SystemExit) as raised:
            harness.main(['session', '--session', 'S9', '--agent', 'codex', '--spend'])
        self.assertIn('unknown session', str(raised.exception))


class Codex(unittest.TestCase):
    folder = Path('/work/label/F07-jadpo-r1')
    environment = {'PATH': '/work/label/F07-jadpo-r1/bin:/opt/node/bin:/usr/bin:/bin'}

    def config(self, tools=True):
        with mock.patch.object(harness, 'real_git', lambda: '/usr/bin/git'), \
                mock.patch.object(harness.shutil, 'which', lambda name: None):
            return harness.codex_config(harness.PINNED_CODEX, self.folder, self.environment, tools)

    def test_command_line(self):
        with mock.patch.object(harness, 'tool', lambda name: name):
            start = harness.agent_command('codex', harness.PINNED_CODEX, 'do it', self.folder / 'app', self.folder)
            resume = harness.agent_command('codex', harness.PINNED_CODEX, 'reply', self.folder / 'app', self.folder,
                                           resume='thread-1')
        self.assertEqual(start, ['codex', 'exec', '--json', '--strict-config', '--ignore-rules', '--model', 'gpt-6.1-sol',
                                 '-c', 'model_reasoning_effort="high"', 'do it'])
        self.assertEqual(resume[:3], ['codex', 'exec', 'resume'])
        self.assertEqual(resume[-2:], ['thread-1', 'reply'])
        self.assertFalse([word for word in start + resume if 'dangerously' in word or word in {'--search', '-s'}])

    def test_config_confines_the_session(self):
        text = self.config()
        for line in ['model = "gpt-6.1-sol"', 'model_reasoning_effort = "high"', 'model_reasoning_summary = "detailed"',
                     'approval_policy = "never"', 'web_search = "disabled"', 'allow_login_shell = false',
                     'default_permissions = "agent-run"', '":minimal" = "read"', '"/private/tmp" = "none"',
                     '"/work/label/F07-jadpo-r1/app" = "write"', '"/work/label/F07-jadpo-r1/docs" = "read"',
                     '"/opt/node" = "read"', 'enabled = true', 'domains = {}',
                     'unix_sockets = { "/work/label/F07-jadpo-r1/sock" = "allow" }', 'network_proxy = true',
                     'apps = false', 'plugins = false', 'memories = false', 'browser_use = false', 'computer_use = false',
                     'hooks = false', 'include_instructions = false']:
            self.assertIn(line + '\n', text)
        writable = re.findall(r'^"([^"]+)" = "write"$', text, re.M)
        self.assertEqual(writable, [str(self.folder / name) for name in ('app', 'tmp', 'home')])
        self.assertNotIn('codex-home" =', text)
        self.assertNotIn('shell_tool = false', text)
        self.assertIn('shell_tool = false\n', self.config(tools=False))

    def test_home_holds_only_the_login_and_goes(self):
        with tempfile.TemporaryDirectory() as scratch:
            home, folder = Path(scratch) / 'user', Path(scratch) / 'run'
            (home / '.codex').mkdir(parents=True)
            (home / '.codex/auth.json').write_text('{}')
            (home / '.codex/auth.json').chmod(0o600)
            (home / '.codex/AGENTS.md').write_text('personal')
            folder.mkdir()
            with mock.patch.object(harness.Path, 'home', lambda: home), \
                    mock.patch.object(harness, 'real_git', lambda: '/usr/bin/git'):
                with harness.codex_home(folder, harness.PINNED_CODEX, self.environment) as codex:
                    self.assertEqual(sorted(p.name for p in codex.iterdir()), ['auth.json', 'config.toml'])
                    self.assertFalse((codex / 'auth.json').is_symlink())
                    self.assertEqual((codex / 'auth.json').read_text(), '{}')
                    self.assertEqual(stat.S_IMODE(os.stat(codex / 'auth.json').st_mode), 0o600)
                    self.assertEqual(stat.S_IMODE(os.stat(codex).st_mode), 0o700)
                self.assertFalse((folder / 'codex-home').exists())
        environment = harness.codex_environment(self.folder)
        self.assertEqual(environment['CODEX_HOME'], str(self.folder / 'codex-home'))
        # zsh writes a here document under TMPPREFIX, not TMPDIR, and the
        # sandbox refuses /tmp, its default.
        self.assertEqual(environment['TMPPREFIX'], str(self.folder / 'tmp/zsh'))
        self.assertTrue(all(value.startswith(str(self.folder)) for value in environment.values()))

    def test_a_refreshed_login_goes_back_and_the_copy_does_not_stay(self):
        with tempfile.TemporaryDirectory() as scratch:
            used, login = Path(scratch) / 'used.json', Path(scratch) / 'auth.json'
            login.write_text(json.dumps({'last_refresh': '2026-11-11T21:10:00Z', 'tokens': 'old'}))
            used.write_text(json.dumps({'last_refresh': '2026-11-15T08:00:00Z', 'tokens': 'new'}))
            harness.keep_refreshed_login(used, login)
            self.assertEqual(json.loads(login.read_text())['tokens'], 'new')
            self.assertEqual(stat.S_IMODE(os.stat(login).st_mode), 0o600)
            used.write_text(json.dumps({'last_refresh': '2026-11-01T00:00:00Z', 'tokens': 'older'}))
            harness.keep_refreshed_login(used, login)
            self.assertEqual(json.loads(login.read_text())['tokens'], 'new')

    def test_probe_verdicts_come_from_what_the_tools_printed_and_the_files_left(self):
        seen = ('head: /Users/me/.codex/config.toml: Operation not permitted\n'
                'touch: ../../outside-the-copy: Operation not permitted\n# Jadpo primer\n'
                'curl: (56) CONNECT tunnel failed\nNetwork access to "example.com" was blocked')
        row = harness.probe_row
        self.assertTrue(row('a', 'head x', 'refused', '/Users/me/.codex/config.toml', seen, {})['ok'])
        self.assertFalse(row('a', 'head x', 'refused', '/Users/me/.claude/CLAUDE.md', seen, {})['ok'], 'no refusal printed: no proof')
        self.assertTrue(row('w', 'touch', 'refused', '../../outside-the-copy', seen, {})['ok'])
        self.assertFalse(row('w', 'touch', 'refused', '../../outside-the-copy', seen, {'../../outside-the-copy': True})['ok'],
                         'a file left outside is a leak')
        self.assertTrue(row('n', 'curl', 'blocked', 'example.com', seen, {})['ok'])
        self.assertFalse(row('n', 'curl', 'blocked', 'example.com', seen + '\nreached\n', {})['ok'])
        self.assertTrue(row('d', 'head', 'allowed', '# Jadpo primer', seen, {})['ok'])
        self.assertTrue(row('d', 'touch probe-ok', 'allowed', None, seen, {'touch probe-ok': True})['ok'])
        self.assertFalse(row('d', 'touch probe-ok', 'allowed', None, seen, {})['ok'])

    def test_probe_names_real_files_outside_the_copy(self):
        checks = harness.probe_checks()
        refused = [command for _, command, expect, _ in checks if expect == 'refused']
        self.assertTrue(any(str(Path.home() / '.codex') in c for c in refused) or not (Path.home() / '.codex').exists())
        self.assertTrue(any('/tmp/' in c for c in refused) and any(str(harness.ROOT.parent) in c for c in refused))

    def test_real_sessions_need_spend(self):
        with self.assertRaises(SystemExit) as raised:
            harness.main(['run', '--agent', 'codex', '--change', 'F01', '--language', 'jadpo', '--run', '1'])
        self.assertIn('--spend', str(raised.exception))


FIXTURES = HERE / 'fixtures'


class CodexTranscripts(unittest.TestCase):
    """A hand-written session in the shape `codex exec --json` and its rollout
    take (codex-cli 0.166): a start, then a resume after QUESTIONS.md."""

    def parsed(self):
        return scoring.parse_codex((FIXTURES / 'codex-exec.jsonl').read_text().splitlines(),
                                   (FIXTURES / 'codex-rollout.jsonl').read_text().splitlines(), 'jadpo',
                                   ['LLM-coding', 'golden-todo'])

    def test_verifications_tokens_and_green(self):
        parsed = self.parsed()
        self.assertEqual([(v['category'], v['failed'], v['tokens_before'], v['errors']) for v in parsed['verifications']],
                         [('check', True, 20300, ['type.refinement_bound']), ('check', False, 25400, []),
                          ('test', False, 30500, [])])
        self.assertEqual(parsed['edits'], 2, 'THINKING.md is not an edit')
        self.assertEqual(scoring.to_green(parsed['verifications'], parsed['edits']), {'attempts': 3, 'tokens': 30500})
        # Each turn reports the thread's usage so far; the resumed session adds to it.
        self.assertEqual([s['usage'] for s in parsed['sessions']],
                         [{'input_tokens': 10000, 'output_tokens': 500, 'cache_creation_input_tokens': 0,
                           'cache_read_input_tokens': 20000},
                          {'input_tokens': 2000, 'output_tokens': 200, 'cache_creation_input_tokens': 0,
                           'cache_read_input_tokens': 20000}])
        self.assertEqual(scoring.total(parsed['tokens']), 52700)
        self.assertIsNone(parsed['cost_usd'])
        self.assertEqual([s['models'] for s in parsed['sessions']], [['gpt-6.1-sol'], ['gpt-6.1-sol']])
        self.assertEqual(parsed['reasoning'][0].splitlines()[0], '**Finding the title rule**')
        self.assertGreater(parsed['thinking_tokens_estimate'], 0)

    def test_refused_reads_are_contamination_and_denials(self):
        parsed = self.parsed()
        # The refused `cat` never became an item; the tool call's input still shows it.
        self.assertEqual(parsed['contamination'], [{'tool': 'exec', 'marker': 'LLM-coding'},
                                                   {'tool': 'exec', 'marker': 'golden-todo'}])
        self.assertEqual([s['denials'] for s in parsed['sessions']], [1, 0])

    def test_wrapped_calls_give_the_verifications_the_record_leaves_out(self):
        # Only the wrappers' log sees a failure that Codex's record has no item for.
        start = scoring.seconds
        calls = [{'name': 'jadpo', 'args': ['check'], 'start': start('2026-11-15T07:00:11.500Z'), 'exit': 1,
                  'output': 'error: x\n  rule: type.refinement_bound\ncheck failed\n'},
                 {'name': 'jadpo', 'args': ['check'], 'start': start('2026-11-15T07:00:15.500Z'), 'exit': 0, 'output': 'checked\n'},
                 {'name': 'jadpo', 'args': ['help', 'type.refinement_bound'], 'start': start('2026-11-15T07:00:13Z'), 'exit': 0},
                 {'name': 'jadpo', 'args': ['test'], 'start': start('2026-11-15T07:01:05.500Z'), 'exit': 0, 'output': 'ok'}]
        rollout = (FIXTURES / 'codex-rollout.jsonl').read_text().splitlines()
        parsed = scoring.with_wrapped_calls(scoring.parse_codex([], rollout, 'jadpo'), calls, 'jadpo')
        self.assertEqual([(v['category'], v['failed'], v['tokens_before'], v['edits_before'], v['errors'])
                          for v in parsed['verifications']],
                         [('check', True, 20300, 1, ['type.refinement_bound']), ('check', False, 25400, 2, []),
                          ('test', False, 30500, 2, [])])
        self.assertEqual(scoring.to_green(parsed['verifications'], parsed['edits']), {'attempts': 3, 'tokens': 30500})
        typescript = scoring.with_wrapped_calls({'verifications': [], 'timeline': {'tokens': [], 'edits': []}},
                                                [{'name': 'npm', 'args': ['run', 'check'], 'start': 1.0, 'exit': 2,
                                                  'output': 'src/a.ts(1,1): error TS2322: x'},
                                                 {'name': 'npx', 'args': ['vitest', 'run'], 'start': 2.0, 'exit': 0}],
                                                'typescript')
        self.assertEqual([(v['category'], v['failed'], v['errors']) for v in typescript['verifications']],
                         [('check', True, ['TS2322']), ('test', False, [])])

    def test_docs_reads_split_a_compound_command_and_script_reads_use_the_bundle(self):
        calls = [{'tool': 'shell', 'failed': False, 'time': 1, 'output': '# primer\n',
                  'input': "/bin/zsh -c \"rg --files -g '*.jadpo' && cat ../docs/primer.md && cat ../docs/topics/gone.md\""},
                 {'tool': 'script', 'failed': False, 'time': 2, 'output': '', 'input': 'run("cat ../docs/topics/a.md ../docs/topics/b.md")'}]
        reads = scoring.docs_reads(calls, {'topics/a.md'})
        self.assertEqual([(r['path'], r['how'], r['found']) for r in reads],
                         [('primer.md', 'shell', True), ('topics/gone.md', 'shell', True), ('topics/a.md', 'script', True),
                          ('topics/b.md', 'script', False)])

    def test_stream_alone_still_scores(self):
        parsed = scoring.parse_codex((FIXTURES / 'codex-exec.jsonl').read_text().splitlines(), [], 'jadpo')
        # The stream leaves out commands that failed inside code mode; the record has them.
        self.assertEqual(len(parsed['verifications']), 2)
        self.assertEqual(scoring.total(parsed['tokens']), 52700)

    def test_a_failed_turn_is_a_harness_failure(self):
        lines = transcript({'type': 'thread.started', 'thread_id': 't'}, {'type': 'turn.started'},
                           {'type': 'turn.failed', 'error': {'message': 'You are not signed in'}})
        parsed = scoring.parse_codex(lines)
        self.assertIn('did not reach the model', harness.harness_failure(parsed, [{'timed_out': False}]))

    def test_docs_reads_and_stumbles(self):
        parsed = self.parsed()
        reads = scoring.docs_reads(parsed['calls'])
        self.assertEqual([(r['path'], r['how'], r['found']) for r in reads],
                         [('topics/named-types.md', 'shell', True), ('topics', 'search', False)])
        result = {'change': 'F07', 'language': 'jadpo', 'run': 1, 'outcome': {'correct': True}, 'tokens': parsed['tokens'],
                  'verifications': parsed['verifications'], 'calls': parsed['calls'], 'docs_reads': reads,
                  'thinking': 'About to raise the bound.', 'debrief': {'text': '1. The bound error named no file.'}}
        report = scoring.stumbles(result)
        for part in ['`type.refinement_bound`: 1 failing run, fixed', 'grep -rl maximum_length', 'no hit', 'About to raise the bound',
                     'The bound error named no file', '1 of 3 check and test runs failed']:
            self.assertIn(part, report)
        self.assertLess(len(report.splitlines()), 40)
        self.assertEqual(scoring.cost([{'cost_usd': None}]), None)
        self.assertEqual(scoring.cost([{'cost_usd': None}, {'cost_usd': 1.5}]), 1.5)

    def test_error_ids(self):
        self.assertEqual(scoring.error_ids('jadpo', 'error: x\n  rule: a.b\n{"rule": "c.d"}'), ['a.b', 'c.d'])
        self.assertEqual(scoring.error_ids('typescript', 'src/a.ts(1,1): error TS2322: x\n  3:5  error  Bad  no-unused-vars\n'),
                         ['TS2322', 'no-unused-vars'])


class Docs(unittest.TestCase):
    pages = {'/docs/primer.md': 'primer.md', '/llms.txt': 'llms.txt', '/docs/logic-built.md': 'topics/logic-built.md'}

    def test_site_links_point_at_the_bundle(self):
        text = ('Read https://jadpo.dev/docs/primer.md. The same paths under `https://jadpo.dev/v/<version>/` are one '
                'release; https://jadpo.dev/llms.txt lists them; see [Logic](https://jadpo.dev/docs/logic-built.md#x), '
                'and `https://jadpo.dev/docs/errors/<rule.id>.md`.')
        local = harness.local_links(text, self.pages, '../docs/')
        self.assertNotIn('https://jadpo.dev', local)
        for part in ['Read ../docs/primer.md.', '`../docs/` are one', '../docs/llms.txt lists',
                     '(../docs/topics/logic-built.md#x)', '`../docs/errors/<rule.id>.md`']:
            self.assertIn(part, local)

    def test_a_link_with_no_local_page_stops_the_setup(self):
        with self.assertRaises(RuntimeError) as raised:
            harness.local_links('See https://jadpo.dev/docs/nowhere.md.', self.pages, '../docs/')
        self.assertIn('https://jadpo.dev/docs/nowhere.md', str(raised.exception))

    def test_the_website_rules_reach_the_agent(self):
        for part in ["Don't rely on what you remember", '../docs/llms.txt', '../docs/primer.md', '(human)',
                     'stop and ask before you change', "read the error's page"]:
            self.assertIn(part, harness.WEBSITE_RULES)


class Wrapper(unittest.TestCase):
    def test_jadpo_calls_are_logged_and_invisible(self):
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            app = folder / 'app'
            app.mkdir()
            (app / 'a.jadpo').write_text('x\n')
            fake = folder / 'fake-jadpo'
            # Non-ASCII output, valid or not, still makes one readable log line.
            fake.write_bytes(b'#!/bin/sh\necho "checked $* caf\xc3\xa9 \xc3"\necho "  rule: type.example" >&2\nexit 3\n')
            fake.chmod(0o755)
            harness.install_wrapper(folder, app, {'jadpo': fake})
            wrapped = subprocess.run([str(folder / 'bin/jadpo'), 'check', '--all'], cwd=app, capture_output=True)
            direct = subprocess.run([str(folder / 'compiler/jadpo'), 'check', '--all'], cwd=app, capture_output=True)
            self.assertEqual((wrapped.returncode, wrapped.stdout, wrapped.stderr),
                             (direct.returncode, direct.stdout, direct.stderr))
            log = [json.loads(line) for line in (folder / 'tmp/jadpo-calls/calls.jsonl').read_text().splitlines()]
            self.assertEqual(len(log), 1)
            self.assertEqual([(c['name'], c['args'], c['exit'], c['rules']) for c in log],
                             [('jadpo', ['check', '--all'], 3, ['type.example'])])
            self.assertIn('rule: type.example', log[0]['output'])
            snapshots = subprocess.run([harness.real_git(), '--git-dir', str(folder / 'tmp/jadpo-calls/snapshots.git'),
                                        'log', '--format=%s'], capture_output=True, text=True).stdout
            self.assertEqual(snapshots.strip(), 'before jadpo check --all')
            self.assertFalse((app / '.git').exists(), "the copy's own history is untouched")


    def test_npm_and_npx_are_wrapped_for_typescript_and_snapshot_only_checks_and_tests(self):
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            app = folder / 'app'
            app.mkdir()
            (app / 'a.ts').write_text('x\n')
            fake = folder / 'fake-npm'
            fake.write_text('#!/bin/sh\necho "src/a.ts(1,1): error TS2322: x"\nexit 2\n')
            fake.chmod(0o755)
            harness.install_wrapper(folder, app, {'npm': fake, 'npx': fake})
            for words in (['npm', 'run', 'check'], ['npm', 'install'], ['npx', 'vitest', 'run'], ['npm', 'test']):
                done = subprocess.run([str(folder / 'bin' / words[0]), *words[1:]], cwd=app, capture_output=True, text=True)
                self.assertEqual(done.returncode, 2)
            trace = folder / 'tmp/jadpo-calls'
            log = [json.loads(line) for line in (trace / 'calls.jsonl').read_text().splitlines()]
            self.assertEqual([(c['name'], c['args'][0]) for c in log], [('npm', 'run'), ('npm', 'install'), ('npx', 'vitest'), ('npm', 'test')])
            snapshots = subprocess.run([harness.real_git(), '--git-dir', str(trace / 'snapshots.git'), 'log', '--format=%s'],
                                       capture_output=True, text=True).stdout.split('\n')
            self.assertEqual(sorted(filter(None, snapshots)), ['before npm run check', 'before npm test', 'before npx vitest run'])


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
