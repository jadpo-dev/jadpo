#!/usr/bin/env python3
"""Fresh agent sessions make one change to the golden todo, in Jadpo or in
TypeScript; hidden tests then say whether it is right. The method is in
README.md.

  python3 benchmarks/agent-changes/harness.py list
  python3 benchmarks/agent-changes/harness.py run --change F07 --language jadpo --run 1
  python3 benchmarks/agent-changes/harness.py matrix --runs 3        # every change, both languages
  python3 benchmarks/agent-changes/harness.py calibrate              # hidden tests against the untouched apps
  python3 benchmarks/agent-changes/harness.py summary --label <label>
  python3 benchmarks/agent-changes/harness.py estimate --label <label>

Each run writes results/<label>/<change>-<language>-r<run>.json; `summary`
writes results/<label>/summary.json and prints the table. Agent sessions
spend tokens: `run` and `matrix` refuse without --spend.
"""
from __future__ import annotations

import argparse
import contextlib
import dataclasses
import importlib.util
import json
import os
import re
import secrets
import shlex
import shutil
import subprocess
import sys
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
GOLDEN = ROOT / 'benchmarks/golden-todo'
TYPESCRIPT = GOLDEN / 'typescript'
CONTRACT = ROOT / 'benchmarks/contract'
RESULTS = HERE / 'results'
WORK = Path(os.environ.get('JADPO_AGENT_WORK', Path.home() / '.cache/jadpo-agent-changes'))
sys.path[:0] = [str(HERE), str(CONTRACT)]

import changes as change_set  # noqa: E402
import contract  # noqa: E402
import scoring  # noqa: E402

LANGUAGES = ('jadpo', 'typescript')
PINNED = {'model': 'claude-opus-5-5', 'effort': 'high', 'budget_usd': 6.0, 'timeout_minutes': 60}
DRY = {'model': 'claude-haiku-5-5', 'effort': 'low', 'budget_usd': 0.5, 'timeout_minutes': 20}
TOOLS = 'Bash,Read,Edit,Write,Glob,Grep'
BASE_FIELDS = frozenset(contract.PUBLIC_FIELDS)
# Withheld from the Jadpo copy: the contract, its reviews and answers, and the twin.
JADPO_WITHHELD = {'README.md', 'REVIEW.md', 'acceptance.json', 'acceptance-v0.2.json', 'adversarial-changes.md',
                  'expected-audit.md', 'typescript-baseline.md', 'typescript', 'build', '.jadpo', 'target'}
TYPESCRIPT_SKIPPED = {'node_modules', 'build', '.wrangler', '__pycache__'}
EXPECTED_UNTOUCHED = {'behaviour': {'failed', 'absent'}, 'safety': {'passed', 'absent'}, 'regression': {'passed'}}
# Cases that need a real deployable; they run only for a change that asks for one.
DEPLOYABLE_CASES = {'CONFIG-001', 'CONFIG-002'}

PROMPT = '''You are working on the todo application in this directory.

The product owner asks:

> {request}

Make the change, keep the app's existing behaviour and rules, and run the project's own checks and tests before you finish.
If the request needs a decision only the product owner can make, or conflicts with the app's current rules, don't guess: write your questions to QUESTIONS.md in this directory and stop. You will get one reply.
When you are done, say in a few lines what you changed.'''

ANSWER = '''The product owner replies to QUESTIONS.md:

> {answer}

Carry on: make whatever change this calls for (or none), run the project's checks and tests, and say in a few lines what you did.'''


def tool(name):
    found = shutil.which(name)
    if found:
        return found
    for candidate in [Path('/Applications/Postgres.app/Contents/Versions/16/bin') / name, Path.home() / '.bun/bin' / name,
                      *sorted(Path('/usr/lib/postgresql').glob('*/bin/' + name), reverse=True)]:
        if candidate.is_file():
            return str(candidate)
    raise RuntimeError(f'{name} was not found on PATH')


def jadpo_binary():
    metadata = json.loads(subprocess.run(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT,
                                         capture_output=True, text=True, check=True).stdout)
    binary = Path(metadata['target_directory']) / 'debug/jadpo'
    if not binary.is_file():
        raise RuntimeError(f'{binary} is missing; run `cargo build -p jadpo-cli` first')
    return binary


def load(name, path, *search):
    for folder in search:
        sys.path.insert(0, str(folder))
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        for folder in search:
            sys.path.remove(str(folder))


# The database the hidden tests (and the agent's own tests) use ----------------

class Server:
    """The PostgreSQL that JADPO_TEST_DATABASE_URL names, or a disposable
    one under the work folder. Owned databases are dropped on stop."""

    def __init__(self, folder):
        self.owned, self.started = [], None
        self.normalize = load('agent_postgres', CONTRACT / 'postgres.py').postgres_uri
        shared = os.environ.get('JADPO_TEST_DATABASE_URL', '').strip()
        if shared:
            self.base = self.normalize(shared)
            return
        folder.mkdir(parents=True, exist_ok=True)
        data = folder / 'pg'
        shutil.rmtree(data, ignore_errors=True)
        subprocess.run([tool('initdb'), '-D', str(data), '-U', 'golden', '--auth=trust', '-E', 'UTF8'],
                       check=True, stdout=subprocess.DEVNULL)
        import socket
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', 0))
            port = probe.getsockname()[1]
        options = f'-p {port} -c unix_socket_directories= -c listen_addresses=127.0.0.1 -c max_connections=200'
        subprocess.run([tool('pg_ctl'), '-D', str(data), '-l', str(folder / 'postgres.log'), '-o', options, '-w', 'start'],
                       check=True, stdout=subprocess.DEVNULL)
        self.started = data
        self.base = f'postgresql://golden@127.0.0.1:{port}/postgres'

    def url(self, database):
        return self.normalize(self.base, database)

    def sql(self, statement):
        subprocess.run([tool('psql'), '-q', '-v', 'ON_ERROR_STOP=1', '-d', self.url('postgres'), '-c', statement],
                       check=True, stdout=subprocess.DEVNULL)

    def create(self):
        name = 'agent_' + secrets.token_hex(6)
        self.sql(f'CREATE DATABASE {name}')
        self.owned.append(name)
        return name

    def stop(self):
        for name in self.owned:
            with contextlib.suppress(subprocess.CalledProcessError):
                self.sql(f'DROP DATABASE {name} WITH (FORCE)')
        self.owned = []
        if self.started:
            subprocess.run([tool('pg_ctl'), '-D', str(self.started), '-m', 'fast', '-w', 'stop'],
                           stdout=subprocess.DEVNULL, check=False)
            self.started = None


# Setting up a run --------------------------------------------------------------

def run_name(change, language, number):
    return f'{change}-{language}-r{number}'


def jadpo_agents_file():
    version = subprocess.run([str(jadpo_binary()), 'version'], capture_output=True, text=True, check=True).stdout.split()[1]
    return (HERE / 'agents/jadpo-AGENTS.md').read_text().replace('{version}', version)


def jadpo_ignore(directory, names):
    top = Path(directory).resolve() == GOLDEN.resolve()
    return [n for n in names if (top and n in JADPO_WITHHELD) or n in {'build', '.jadpo', '__pycache__'}]


def prepare(change, language, folder, compiler):
    """A fresh copy of the app, its own git history to diff against, and for
    Jadpo the docs bundle and the compiler beside it. Nothing else of the
    repository is reachable from the copy."""
    if folder.exists():
        shutil.rmtree(folder)
    app = folder / 'app'
    if language == 'jadpo':
        shutil.copytree(GOLDEN, app, ignore=jadpo_ignore)
        (app / 'AGENTS.md').write_text(jadpo_agents_file())
        bundle = folder / 'docs'
        subprocess.run([sys.executable, str(ROOT / 'tools/docs/language_docs.py'), '--bundle', str(bundle)], cwd=ROOT,
                       check=True, stdout=subprocess.DEVNULL)
        (folder / 'bin').mkdir()
        shutil.copy2(compiler, folder / 'bin/jadpo')
    else:
        shutil.copytree(TYPESCRIPT, app, ignore=shutil.ignore_patterns(*TYPESCRIPT_SKIPPED))
        subprocess.run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=app, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    (app / '.gitignore').write_text('node_modules/\nbuild/\n.jadpo/\n.wrangler/\n')
    git(app, 'init', '-q', '-b', 'main')
    git(app, 'add', '-A')
    git(app, '-c', 'user.name=harness', '-c', 'user.email=harness@example.invalid', 'commit', '-q', '-m', 'start')
    return app


def git(app, *arguments):
    return subprocess.run(['git', *arguments], cwd=app, check=True, capture_output=True, text=True).stdout


def final_diff(app):
    git(app, 'add', '-A')
    return git(app, 'diff', '--cached', '--no-color', '--find-renames', 'HEAD')


def agent_environment(folder, database_url):
    """Only what a fresh terminal has: no variable of the host session (a
    parent Claude Code's CLAUDE_*, ANTHROPIC_BASE_URL) and nothing naming the
    repository."""
    path = [str(folder / 'bin')] if (folder / 'bin').is_dir() else []
    for name in ['node', 'npm', 'bun', 'git', 'psql']:
        with contextlib.suppress(RuntimeError):
            directory = str(Path(tool(name)).parent)
            if directory not in path:
                path.append(directory)
    path += ['/usr/local/bin', '/usr/bin', '/bin', '/usr/sbin', '/sbin']
    environment = {key: os.environ[key] for key in ['HOME', 'USER', 'LOGNAME', 'SHELL', 'TMPDIR', 'LANG'] if key in os.environ}
    environment.update({'PATH': ':'.join(path), 'TERM': 'dumb', 'NO_COLOR': '1',
                        'TEST_DATABASE_URL': database_url, 'JADPO_TEST_DATABASE_URL': database_url})
    return environment


def system_prompt(app):
    return 'The project instructions, AGENTS.md:\n\n' + (app / 'AGENTS.md').read_text()


def agent_command(agent, settings, prompt, app, folder, resume=None):
    if agent != 'claude':
        return [*shlex.split(agent), prompt]
    command = [tool('claude'), '-p', prompt, '--model', settings['model'], '--effort', settings['effort'],
               '--output-format', 'stream-json', '--verbose', '--safe-mode', '--restricted', '--strict-mcp-config',
               '--tools', TOOLS, '--allowedTools', 'Bash', '--permission-mode', 'acceptEdits',
               '--permission-prompts', 'none', '--max-budget-usd', f'{settings["budget_usd"]:.2f}',
               '--append-system-prompt', system_prompt(app)]
    if (folder / 'docs').is_dir():
        command += ['--add-dir', str(folder / 'docs')]
    if resume:
        command += ['--resume', resume]
    return command


def session(agent, settings, prompt, app, folder, environment, transcript, resume=None, budget=None):
    settings = {**settings, 'budget_usd': budget if budget is not None else settings['budget_usd']}
    command = agent_command(agent, settings, prompt, app, folder, resume)
    started = time.monotonic()
    with transcript.open('a') as output, (folder / 'agent.stderr').open('a') as errors:
        try:
            subprocess.run(command, cwd=app, env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                           timeout=settings['timeout_minutes'] * 60, check=False)
            timed_out = False
        except subprocess.TimeoutExpired:
            timed_out = True
    return {'seconds': round(time.monotonic() - started, 1), 'timed_out': timed_out}


def agent_version(agent):
    if agent != 'claude':
        return agent
    done = subprocess.run([tool('claude'), '--version'], capture_output=True, text=True, check=False)
    return 'claude ' + done.stdout.strip()


def visible_gate(language, app, environment):
    """The project's own checks and tests, run by the harness after the agent
    stops: did the agent leave it green by its own lights?"""
    commands = ([['jadpo', 'check'], ['jadpo', 'test']] if language == 'jadpo'
                else [['npm', 'run', 'check'], ['npm', 'run', 'lint'], ['npm', 'test']])
    report = []
    for command in commands:
        done = subprocess.run(command, cwd=app, env=environment, capture_output=True, text=True, check=False, timeout=900)
        report.append({'command': ' '.join(command), 'passed': done.returncode == 0, 'tail': (done.stdout + done.stderr)[-1500:]})
    return report


# Hidden tests ------------------------------------------------------------------

def regression_cases(change):
    return [identifier for identifier in contract.CASES if identifier not in change.replaces
            and (change.deployable or identifier not in DEPLOYABLE_CASES)]


def jadpo_routes(app):
    routes = set()
    for record in (app / 'api').glob('*.jadpo_record'):
        for line in record.read_text().splitlines():
            match = re.match(r'(GET|POST|PUT|PATCH|DELETE) (/.*\S)\s*$', line)
            if match:
                routes.add((match.group(1), match.group(2)))
    return routes


def typescript_routes(app, environment):
    script = 'import("./src/inventory.ts").then(m => console.log(JSON.stringify(m.inventory().routes.map(r => [r.method.toUpperCase(), r.path]))))'
    done = subprocess.run([tool('node'), '--experimental-strip-types', '--no-warnings', '-e', script], cwd=app,
                          env=environment, capture_output=True, text=True, check=False, timeout=120)
    try:
        return {tuple(item) for item in json.loads(done.stdout.strip().splitlines()[-1])}
    except (ValueError, IndexError):
        return set()


def diff_routes(diff):
    """Paths the diff adds as string literals or Jadpo route lines, so a route
    left out of an inventory is still probed."""
    found = set()
    for line in diff.splitlines():
        if not line.startswith('+'):
            continue
        for match in re.finditer(r'''["'`](/[a-z][A-Za-z0-9_/{}:.\-]*)["'`]''', line):
            found.add(('GET', match.group(1)))
        match = re.match(r'\+\s*(GET|POST|PUT|PATCH|DELETE)\s+(/.*?)\s*\{?\s*$', line)
        if match:
            found.add((match.group(1), match.group(2)))
        match = re.match(r'\+\s*prefix:\s*(/\S+)', line)
        if match:
            found.add(('GET', match.group(1)))
    return found


def execute(driver, change, routes, diff, secret_values, cases, pristine=False):
    """Every hidden test and contract case, each from a reset state. On the
    untouched app the contract keeps its own public fields."""
    contract.PUBLIC_FIELDS = BASE_FIELDS | (frozenset() if pristine else change.extra_fields)
    try:
        reports = []
        for name, function, kind in ([(t.__name__, t, t.kind) for t in change.tests]
                                     + [(identifier, contract.CASES[identifier], 'regression') for identifier in cases]):
            harness = contract.Contract(driver)
            harness.routes, harness.diff, harness.secrets = routes, diff, secret_values
            try:
                harness.reset()
                function(harness)
                reports.append({'name': name, 'kind': kind, 'status': 'passed'})
            except change_set.Absent as error:
                reports.append({'name': name, 'kind': kind, 'status': 'absent', 'message': str(error)})
            except Exception as error:  # noqa: BLE001 - every failure is a result
                reports.append({'name': name, 'kind': kind, 'status': 'failed', 'message': str(error)[:1500]})
        return reports
    finally:
        contract.PUBLIC_FIELDS = BASE_FIELDS


def unavailable(change, cases, message):
    return ([{'name': t.__name__, 'kind': t.kind, 'status': 'not_run', 'message': message} for t in change.tests]
            + [{'name': c, 'kind': 'regression', 'status': 'not_run', 'message': message} for c in cases])


def hidden_jadpo(app, change, work, server, compiler, diff, pristine=False):
    cases = regression_cases(change)
    driver = None
    try:
        runner = load('agent_contract_run', CONTRACT / 'run.py', CONTRACT)
        runner.PROJECT, runner.WORK, runner.CACHE = app, work / 'contract', ROOT / 'target/agent-changes'
        runner.WORK.mkdir(parents=True, exist_ok=True)
        binary = runner.build_native(ROOT)
        if change.deployable:
            subprocess.run([str(compiler), 'build', '--json'], cwd=app, check=True, capture_output=True,
                           env={**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0'})
        database = server.create()
        applied = subprocess.run([str(compiler), 'migrate', 'apply', '--url', server.url(database), '--json'], cwd=app,
                                 capture_output=True, text=True)
        if applied.returncode:
            raise RuntimeError('migrations did not apply: ' + (applied.stdout + applied.stderr)[-1500:])
        driver = runner.Native(binary, server.url(database), compiler)
        secret_values = [driver.environment['MAIL_API_KEY'], driver.environment['SESSION_SIGNING_KEY']]
        return execute(driver, change, jadpo_routes(app) | diff_routes(diff), diff, secret_values, cases, pristine)
    except Exception as error:  # noqa: BLE001
        return unavailable(change, cases, 'hidden tests could not run: ' + str(error)[-1500:])
    finally:
        if driver:
            driver.stop()


def hidden_typescript(app, change, work, server, diff, pristine=False):
    cases = regression_cases(change)
    controller = None
    try:
        shutil.rmtree(app / 'adapter', ignore_errors=True)
        shutil.copytree(TYPESCRIPT / 'adapter', app / 'adapter', ignore=shutil.ignore_patterns('__pycache__'))
        sys.modules.pop('support', None)
        runner = load('agent_typescript_run', app / 'adapter/run.py', app / 'adapter')
        runner.ROOT = ROOT
        host = runner.SharedPostgres(server.base)
        name = 'agent_ts_' + secrets.token_hex(5)
        host.prepare(name)
        server.owned.append(name)
        host.owned.remove(name)
        environment = {**os.environ, 'GOLDEN_PSQL': tool('psql'), 'SESSION_SIGNING_KEY': secrets.token_urlsafe(32),
                       'BROWSER_ORIGIN': 'https://todo.example.test', 'OIDC_ISSUER': 'https://identity.example/tenant/',
                       'OIDC_AUDIENCE': 'golden-todo', 'MAIL_API_KEY': secrets.token_urlsafe(32),
                       'CONTROL_DATABASE_URL': host.url('postgres'), 'DATABASE_URL': host.url(name)}
        work.mkdir(parents=True, exist_ok=True)
        controller = runner.Controller('node', environment, work)
        routes = typescript_routes(app, environment) | diff_routes(diff)
        return execute(controller, change, routes, diff,
                       [environment['MAIL_API_KEY'], environment['SESSION_SIGNING_KEY']], cases, pristine)
    except Exception as error:  # noqa: BLE001
        return unavailable(change, cases, 'hidden tests could not run: ' + str(error)[-1500:])
    finally:
        if controller:
            controller.stop()


def hidden(language, app, change, work, server, compiler, diff, pristine=False):
    if language == 'jadpo':
        return hidden_jadpo(app, change, work, server, compiler, diff, pristine)
    return hidden_typescript(app, change, work, server, diff, pristine)


# One run -----------------------------------------------------------------------

def stand_in(agent, language):
    """A stand-in agent's command for this language, its file arguments made
    absolute since it runs in the copy."""
    words = []
    for word in shlex.split(agent.replace('{language}', language)):
        path = Path(word)
        words.append(str(path.resolve()) if not path.is_absolute() and path.exists() else word)
    return shlex.join(words)


def run(change, language, number, label, settings, agent='claude', keep=False):
    folder = WORK / label / run_name(change.id, language, number)
    if agent != 'claude':
        agent = stand_in(agent, language)
    compiler = jadpo_binary()
    app = prepare(change, language, folder, compiler)
    server = Server(folder / 'postgres')
    try:
        environment = agent_environment(folder, server.url(server.create()))
        transcript = folder / 'transcript.jsonl'
        timings = [session(agent, settings, PROMPT.format(request=change.request), app, folder, environment, transcript)]
        questions = app / 'QUESTIONS.md'
        asked = questions.is_file() and questions.read_text().strip() != ''
        if asked:
            first = scoring.parse_transcript(transcript.read_text().splitlines(), language)
            remaining = max(0.0, settings['budget_usd'] - first['cost_usd'])
            identifier = first['sessions'][-1]['session_id'] if first['sessions'] else None
            if remaining > 0 and identifier:
                timings.append(session(agent, settings, ANSWER.format(answer=change.owner_answer), app, folder,
                                       environment, transcript, resume=identifier, budget=remaining))
        diff = final_diff(app)
        visible = visible_gate(language, app, environment)
        reports = hidden(language, app, change, folder / 'hidden', server, compiler, diff)
        result = score(change, language, number, label, settings, transcript, diff, visible, reports, asked, timings, folder)
        result['agent'] = agent_version(agent)
    finally:
        server.stop()
    destination = RESULTS / label / f'{run_name(change.id, language, number)}.json'
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(result, indent=2) + '\n')
    if not keep:
        for heavy in ['app/node_modules', 'app/build', 'hidden', 'postgres', 'bin', 'docs']:
            shutil.rmtree(folder / heavy, ignore_errors=True)
    return result


def score(change, language, number, label, settings, transcript, diff, visible, reports, asked, timings, folder):
    forbidden = [str(ROOT), 'LLM-coding', 'golden-todo', 'acceptance-v0.2', 'adversarial-changes']
    parsed = scoring.parse_transcript(transcript.read_text().splitlines(), language, folder, forbidden)
    tests = [r for r in reports if r['kind'] != 'regression']
    regression = [r for r in reports if r['kind'] == 'regression']
    visible_green = all(step['passed'] for step in visible)
    return {
        'schema': 1, 'label': label, 'change': change.id, 'kind': change.kind, 'language': language, 'run': number,
        'settings': settings, 'sessions': parsed['sessions'], 'timings': timings,
        'outcome': scoring.outcome(dataclasses.asdict(change) | {'tests': None}, tests, regression, asked, visible_green, diff),
        'green': scoring.to_green(parsed['verifications'], parsed['edits']),
        'mistakes': scoring.mistakes(parsed['verifications'], tests + regression),
        'verifications': parsed['verifications'], 'edits': parsed['edits'],
        'tokens': parsed['tokens'], 'cost_usd': parsed['cost_usd'], 'turns': parsed['turns'],
        'review': scoring.review_metrics(diff, language), 'visible': visible, 'hidden': reports,
        'contamination': parsed['contamination'], 'diff': diff,
        'harness_failure': harness_failure(parsed, timings),
    }


def harness_failure(parsed, timings):
    """Why a run says nothing about the agent: the session never reached the
    model (signed out, an API error) or never started. Such a run is kept and
    left out of the summary, never scored."""
    if not parsed['sessions']:
        return 'the agent wrote no session result' + (' (timed out)' if any(t['timed_out'] for t in timings) else '')
    first = parsed['sessions'][0]
    if first['is_error'] and first['cost_usd'] == 0 and first['turns'] <= 1 and not parsed['verifications'] and not parsed['edits']:
        return 'the session did not reach the model: ' + first['text'][:200]
    return None


# Commands ----------------------------------------------------------------------

def command_list(_):
    for change in change_set.CHANGES:
        print(f'{change.id}  {change.kind:<11} {change.title}')


def selected(values):
    if not values:
        return list(change_set.CHANGES)
    unknown = set(values) - set(change_set.BY_ID)
    if unknown:
        raise SystemExit('unknown change: ' + ', '.join(sorted(unknown)))
    return [change_set.BY_ID[v] for v in values]


def settings_for(args):
    base = DRY if args.dry else PINNED
    return {key: getattr(args, key) if getattr(args, key, None) is not None else value for key, value in base.items()}


def command_run(args):
    if args.agent == 'claude' and not args.spend:
        raise SystemExit('agent sessions spend tokens; pass --spend once a maintainer has approved the runs')
    settings = settings_for(args)
    label = args.label or time.strftime('%Y%m%d-%H%M') + ('-dry' if args.dry else '')
    languages = [args.language] if args.language else list(LANGUAGES)
    numbers = [args.run] if args.run else list(range(1, args.runs + 1))
    for number in numbers:
        for change in selected(args.change):
            for language in languages:
                path = RESULTS / label / f'{run_name(change.id, language, number)}.json'
                if path.exists() and not args.again:
                    print(f'kept: {path.name} (already run; --again reruns it)')
                    continue
                result = run(change, language, number, label, settings, args.agent, args.keep)
                state = 'correct' if result['outcome']['correct'] else 'incorrect'
                print(f'{state}: {change.id} {language} r{number} · ${result["cost_usd"]:.2f} · '
                      f'{scoring.total(result["tokens"]):,} tokens · results/{label}/{path.name}')
    command_summary(argparse.Namespace(label=label))


def load_results(label):
    folder = RESULTS / label
    return [json.loads(path.read_text()) for path in sorted(folder.glob('*.json')) if path.name not in {'summary.json', 'calibration.json'}]


def command_summary(args):
    results = load_results(args.label)
    failed = [r for r in results if r.get('harness_failure')]
    for item in failed:
        print(f'left out: {run_name(item["change"], item["language"], item["run"])}: {item["harness_failure"]}', file=sys.stderr)
    summary = scoring.summarize([r for r in results if not r.get('harness_failure')])
    summary['left_out'] = [{'run': run_name(r['change'], r['language'], r['run']), 'reason': r['harness_failure']} for r in failed]
    (RESULTS / args.label / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(scoring.table(summary))


def command_estimate(args):
    results = load_results(args.label)
    by_language = {}
    for language in LANGUAGES:
        runs = [r for r in results if r['language'] == language and r['sessions']]
        if not runs:
            continue
        tokens = {key: sum(r['tokens'][key] for r in runs) / len(runs) for key in runs[0]['tokens']}
        by_language[language] = tokens
    if not by_language:
        raise SystemExit(f'no measured sessions under results/{args.label}')
    estimate = scoring.estimate(by_language, args.model, len(change_set.CHANGES), args.runs)
    print(json.dumps(estimate, indent=2))


def command_calibrate(args):
    """The hidden tests against the untouched apps: every behaviour test
    must fail, every contract case pass, and every safety test pass or find
    the change absent, or the test measures nothing."""
    label = args.label or 'calibration'
    compiler = jadpo_binary()
    report = {}
    server = Server(WORK / label / 'postgres')
    try:
        for language in ([args.language] if args.language else LANGUAGES):
            folder = WORK / label / language
            app = prepare(None, language, folder, compiler)
            for change in selected(args.change):
                reports = hidden(language, app, change, folder / 'hidden' / change.id, server, compiler, '', pristine=True)
                wrong = [r for r in reports if r['status'] not in EXPECTED_UNTOUCHED[r['kind']]]
                report.setdefault(language, {})[change.id] = {'reports': reports, 'wrong': [r['name'] for r in wrong]}
                print(f'{"ok" if not wrong else "WRONG"}: {language} {change.id}'
                      + ''.join(f'\n  {r["name"]} ({r["kind"]}) {r["status"]}: {r.get("message", "")[:400]}' for r in wrong))
    finally:
        server.stop()
    (RESULTS / label).mkdir(parents=True, exist_ok=True)
    (RESULTS / label / 'calibration.json').write_text(json.dumps(report, indent=2) + '\n')
    return 1 if any(item['wrong'] for language in report.values() for item in language.values()) else 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('list').set_defaults(handler=command_list)
    for name in ['run', 'matrix']:
        sub = commands.add_parser(name)
        sub.add_argument('--change', action='append', default=[])
        sub.add_argument('--language', choices=LANGUAGES)
        sub.add_argument('--run', type=int)
        sub.add_argument('--runs', type=int, default=3)
        sub.add_argument('--label')
        sub.add_argument('--dry', action='store_true', help='the cheap plumbing run: Haiku, low effort, $0.50')
        sub.add_argument('--model')
        sub.add_argument('--effort')
        sub.add_argument('--budget-usd', dest='budget_usd', type=float)
        sub.add_argument('--timeout-minutes', dest='timeout_minutes', type=int)
        sub.add_argument('--agent', default='claude', help='claude, or a stand-in command (replay.py) for plumbing checks')
        sub.add_argument('--spend', action='store_true', help='allow real agent sessions')
        sub.add_argument('--again', action='store_true')
        sub.add_argument('--keep', action='store_true', help='keep the run folder whole')
        sub.set_defaults(handler=command_run)
    sub = commands.add_parser('calibrate')
    sub.add_argument('--change', action='append', default=[])
    sub.add_argument('--language', choices=LANGUAGES)
    sub.add_argument('--label')
    sub.set_defaults(handler=command_calibrate)
    for name, handler in [('summary', command_summary), ('estimate', command_estimate)]:
        sub = commands.add_parser(name)
        sub.add_argument('--label', required=True)
        if name == 'estimate':
            sub.add_argument('--model', default=PINNED['model'], choices=sorted(scoring.PRICES))
            sub.add_argument('--runs', type=int, default=3)
        sub.set_defaults(handler=handler)
    args = parser.parse_args(argv)
    return args.handler(args) or 0


if __name__ == '__main__':
    raise SystemExit(main())
