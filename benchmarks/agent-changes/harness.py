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
import functools
import dataclasses
import hashlib
import importlib.util
import json
import os
import re
import secrets
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path
from urllib.parse import quote

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
GOLDEN = ROOT / 'benchmarks/golden-todo'
TYPESCRIPT = GOLDEN / 'typescript'
HELPDESK = ROOT / 'benchmarks/helpdesk'
CONTRACT = ROOT / 'benchmarks/contract'
RESULTS = HERE / 'results'
WORK = Path(os.environ.get('JADPO_AGENT_WORK', Path.home() / '.cache/jadpo-agent-changes'))
sys.path[:0] = [str(HERE), str(CONTRACT)]

import changes as change_set  # noqa: E402
import contract  # noqa: E402
import desk  # noqa: E402
import desk_changes as desk_set  # noqa: E402
import scoring  # noqa: E402
from scoring import events  # noqa: E402

LANGUAGES = ('jadpo', 'typescript')
AGENTS = ('claude', 'codex')
# Claude Code on the owner's Claude subscription, as Codex is on ChatGPT's: no
# dollar cap (--budget-usd adds one), so a run is capped by time alone.
PINNED = {'model': 'claude-opus-5-5', 'effort': 'high', 'timeout_minutes': 60}
DRY = {'model': 'claude-haiku-5-5', 'effort': 'low', 'timeout_minutes': 20}
# The subscription login a Claude run uses: a token from `claude setup-token`,
# saved by the owner, 0600. Never printed and never copied into a run.
CLAUDE_TOKEN = Path(os.environ.get('JADPO_CLAUDE_TOKEN_FILE', Path.home() / '.config/jadpo-agent-changes/claude-oauth-token'))
# PostgreSQL's socket for a sandboxed run, outside the run's folder so a long
# label or work folder can't take its path past the 103 bytes macOS allows.
SOCKET_ROOT = Path(os.environ.get('JADPO_SOCKET_ROOT', Path.home() / '.cache/jadpo-sock'))
SOCKET_LIMIT = 103
# Codex on a ChatGPT subscription: no dollar cap and, in this Codex, no
# enforced token cap (its rollout budget is still under development), so a
# run is capped by time alone.
PINNED_CODEX = {'model': 'gpt-6.1-sol', 'effort': 'high', 'service_tier': 'default', 'timeout_minutes': 60}
DRY_CODEX = {'model': 'gpt-6.1-sol', 'effort': 'low', 'service_tier': 'default', 'timeout_minutes': 20}
CODEX_APP = Path('/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex')
# Codex features that reach past the run's copy: the ChatGPT account's apps,
# plugins and memories, the browser and the desktop, image generation, and
# hooks. Web search is off in the config itself.
CODEX_FEATURES_OFF = ('apps', 'plugins', 'remote_plugin', 'memories', 'browser_use', 'browser_use_external',
                      'in_app_browser', 'computer_use', 'image_generation', 'goals', 'tool_suggest', 'skill_search',
                      'skill_mcp_dependency_install', 'hooks', 'multi_agent', 'multi_agent_v2', 'sleep_tool')
TOOLS = 'Bash,Read,Edit,Write,Glob,Grep'
BASE_FIELDS = frozenset(contract.PUBLIC_FIELDS)
# Withheld from the Jadpo copy: the contract, its reviews and answers, and the twin.
JADPO_WITHHELD = {'README.md', 'REVIEW.md', 'acceptance.json', 'acceptance-v0.2.json', 'adversarial-changes.md',
                  'expected-audit.md', 'typescript-baseline.md', 'typescript', 'build', '.jadpo', 'target'}
TYPESCRIPT_SKIPPED = {'node_modules', 'build', '.wrangler', '__pycache__'}
EXPECTED_UNTOUCHED = {'behaviour': {'failed', 'absent'}, 'safety': {'passed', 'absent'}, 'regression': {'passed'}}
# Cases that need a real deployable; they run only for a change that asks for one.
DEPLOYABLE_CASES = {'CONFIG-001', 'CONFIG-002'}

PROMPT = '''You are working on the {app} application in this directory.

The product owner asks:

> {request}

Make the change, keep the app's existing behaviour and rules, and run the project's own checks and tests before you finish.
If the request needs a decision only the product owner can make, or conflicts with the app's current rules, don't guess: write your questions to QUESTIONS.md in this directory and stop. You will get one reply.
When you are done, say in a few lines what you changed.'''

# Tuning runs only: narrating changes behaviour and tokens, so held-out runs
# never ask for it (README, "Runs").
THINKING = '''

As you go, keep a running THINKING.md in this directory: brief notes on what you are about to try, what you expected, what surprised you, and where you are unsure or couldn't find something. It is raw notes, not a deliverable: nobody reviews it as part of the change.'''

DEBRIEF = '''The task is over and your work has been kept; change nothing more. For the people who make the tools you used, answer briefly:
1. What was confusing?
2. What did you look for and not find?
3. Which error message or documentation page misled you, if any?
4. What would you change?'''

# Six changes tune Jadpo; the other twelve are held out until Jadpo is frozen
# (README, "Runs"). The six span the kinds: a plain feature, two with an
# unsafe shortcut, the change the owner decides, and two to decline.
TUNING = ('F01', 'F03', 'F06', 'C03', 'C11', 'C14')
# 10.2's held-out twelve, and 10.4's fresh twelve, written before any was measured.
SETS = {'tuning': TUNING,
        'held-out': tuple(c.id for c in change_set.CHANGES if c.id not in TUNING and c.id not in change_set.FRESH),
        'fresh': change_set.FRESH,
        'desk': tuple(i for i in desk_set.BY_ID if i not in desk_set.FRESH + desk_set.HELD + desk_set.HELD2 + desk_set.HELD3),
        'desk-fresh': desk_set.FRESH,
        'desk-held': desk_set.HELD,
        'desk-held2': desk_set.HELD2,
        'desk-held3': desk_set.HELD3}

# The apps a change is made to: the golden todo, and 10.5's helpdesk, whose
# changes run five to a session (desk_changes.SESSIONS).
APPS = {
    'todo': {'jadpo': GOLDEN, 'typescript': TYPESCRIPT, 'noun': 'todo', 'withheld': JADPO_WITHHELD},
    'desk': {'jadpo': HELPDESK, 'typescript': HELPDESK / 'typescript', 'noun': 'helpdesk',
             'withheld': {'typescript', 'build', '.jadpo', 'target'}},
}


def app_of(change):
    return 'desk' if change is not None and change.id in desk_set.BY_ID else 'todo'


def change_by_id(identifier):
    return desk_set.BY_ID.get(identifier) or change_set.BY_ID[identifier]


def set_of(change_id):
    return next(name for name, ids in SETS.items() if change_id in ids)

# What jadpo.dev's "Paste into your agent" prompt asks, beyond what `jadpo
# new`'s AGENTS.md already says.
WEBSITE_RULES = '''
## Before you start

- Don't rely on what you remember about Jadpo. Start at `../docs/llms.txt`,
  and read `../docs/primer.md` before you write any Jadpo.
- When an error's `next:` ends in `(human)`, stop and ask before you change
  anything.
- For any other error, read the error's page (`jadpo help <rule>`, or
  `../docs/errors/<rule>.md`) and fix it.
'''

ANSWER = '''The product owner replies to QUESTIONS.md:

> {answer}

Carry on: make whatever change this calls for (or none), run the project's checks and tests, and say in a few lines what you did.'''


def owner_reply(change):
    """The owner's one reply, the same in both languages. From 10.6 it ends
    with what the owner accepts, in their own words, as an owner answering an
    acceptance would: a check that needs a person's acceptance then has one to
    quote, and a change that needs none reads it as a plain yes."""
    answer = change.owner_answer
    if change.owner_accepts:
        answer += f'\n>\n> If a check needs me to accept something for this: I accept it. Yes, {change.owner_accepts}.'
    return ANSWER.format(answer=answer)


def tool(name):
    found = shutil.which(name)
    if found:
        return found
    for candidate in [Path('/Applications/Postgres.app/Contents/Versions/16/bin') / name, Path.home() / '.bun/bin' / name,
                      *([CODEX_APP] if name == 'codex' else []),
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

    def __init__(self, folder, unix=None):
        self.owned, self.started, self.socket = [], None, None
        self.normalize = load('agent_postgres', CONTRACT / 'postgres.py').postgres_uri
        shared = os.environ.get('JADPO_TEST_DATABASE_URL', '').strip()
        if shared:
            if unix:
                raise SystemExit('a Codex run needs its own PostgreSQL on a Unix socket; unset JADPO_TEST_DATABASE_URL')
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
        if unix:
            # A sandbox with no network still reaches a socket it is allowed.
            unix.mkdir(parents=True, exist_ok=True)
            if len(str(unix / f'.s.PGSQL.{port}')) > SOCKET_LIMIT:
                raise SystemExit(f'{unix} is too long for a Unix socket; set JADPO_SOCKET_ROOT to a shorter folder')
            self.socket = unix
        directories = self.socket or ''
        options = f'-p {port} -c unix_socket_directories={directories} -c listen_addresses=127.0.0.1 -c max_connections=200'
        subprocess.run([tool('pg_ctl'), '-D', str(data), '-l', str(folder / 'postgres.log'), '-o', options, '-w', 'start'],
                       check=True, stdout=subprocess.DEVNULL)
        self.started, self.port = data, port
        self.base = f'postgresql://golden@127.0.0.1:{port}/postgres'

    def url(self, database):
        return self.normalize(self.base, database)

    def socket_url(self, database, language='jadpo'):
        """The database through the Unix socket. Jadpo takes the folder as
        the host; postgres.js takes a hostname literally, so TypeScript gets
        it in ?host=, as its own adapter writes it."""
        if language == 'typescript':
            return f'postgresql://golden@localhost:{self.port}/{database}?host={quote(str(self.socket), safe="/")}'
        return f'postgresql://golden@{quote(str(self.socket), safe="")}:{self.port}/{database}'

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
        if self.socket and self.socket.is_relative_to(SOCKET_ROOT):
            shutil.rmtree(self.socket, ignore_errors=True)


def socket_folder(folder):
    """Where a sandboxed run's PostgreSQL listens: a short folder named
    for the run, whatever the label and the work folder are."""
    return SOCKET_ROOT / hashlib.sha256(str(folder).encode()).hexdigest()[:12]


# Setting up a run --------------------------------------------------------------

def run_name(change, language, number):
    return f'{change}-{language}-r{number}'


# The bundle's folder for each kind of page index.json lists.
BUNDLE_FOLDERS = {'topic': 'topics', 'command': 'commands', 'landing': 'landings', 'type': 'types', 'error': 'errors',
                  'learn': 'learn', 'guide': 'guides'}
SITE_LINK = re.compile(r'https://jadpo\.dev(/[^\s)>\]`"\']*[^\s)>\]`"\'.,;:])?/?')


def bundle_pages(bundle):
    """Each jadpo.dev page's path, as the site serves it, to its file in the
    bundle (docs/documentation.md, "What the site serves")."""
    docs = load('agent_language_docs', ROOT / 'tools/docs/language_docs.py', ROOT / 'tools/docs')
    pages = {'/llms.txt': 'llms.txt', '/.well-known/agent-skills/jadpo/SKILL.md': 'skills/jadpo/SKILL.md'}
    for page in json.loads((bundle / 'index.json').read_text())['pages']:
        local = 'primer.md' if page['kind'] == 'primer' else f'{BUNDLE_FOLDERS[page["kind"]]}/{page["id"]}.md'
        if (bundle / local).is_file():
            pages[docs.site_path(page['kind'], page['id']) + '.md'] = local
    return pages


def local_links(text, pages, prefix):
    """Every jadpo.dev link in `text` pointed at the bundle (`prefix` is the
    bundle's path from the file). A link with no page in the bundle stops
    the run's setup: the agent has no network to follow it."""
    missing = []

    def local(match):
        path, _, anchor = (match.group(1) or '/').partition('#')
        if path.startswith('/v/'):
            # One release's docs: the bundle is that release.
            path = '/' + path.split('/', 3)[3] if path.count('/') >= 3 else '/'
            if path == '/':
                return prefix
        if '<' in path:
            # A pattern such as /docs/errors/<rule.id>.md: the bundle's folder for it.
            folder = next((f'{name}/' for kind, name in BUNDLE_FOLDERS.items()
                           if path.startswith(f'/docs/{name}/') or path.startswith(f'/docs/{kind}s/')), None)
            if folder:
                return prefix + folder + path.rsplit('/', 1)[1]
        if path in pages:
            return prefix + pages[path] + (f'#{anchor}' if anchor else '')
        missing.append(match.group(0))
        return match.group(0)

    # One release's docs, `https://jadpo.dev/v/<version>/`: the bundle is that release.
    rewritten = SITE_LINK.sub(local, text.replace('https://jadpo.dev/v/<version>/', prefix or './'))
    if missing:
        raise RuntimeError('jadpo.dev links with no page in the docs bundle: ' + ', '.join(sorted(set(missing))))
    return rewritten


EXAMPLE_LINK = re.compile(r'https://github\.com/jadpo-dev/jadpo/tree/[^/\s)]+/examples/([\w-]+)')


def copy_examples(bundle):
    """The teaching examples `jadpo new`'s AGENTS.md points to, as the
    repository tracks them, into the bundle: the run has no network."""
    listed = git(ROOT, 'ls-files', '-z', 'examples').split('\0')
    for name in filter(None, listed):
        target = bundle / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, target)


def local_examples(text, bundle, prefix):
    missing = []

    def local(match):
        if (bundle / 'examples' / match.group(1)).is_dir():
            return f'{prefix}examples/{match.group(1)}'
        missing.append(match.group(0))
        return match.group(0)

    rewritten = EXAMPLE_LINK.sub(local, text)
    if missing:
        raise RuntimeError('example links with no example in the docs bundle: ' + ', '.join(sorted(set(missing))))
    return rewritten


def jadpo_agents_file(compiler, bundle):
    """What `jadpo new` writes for a project's agent, its jadpo.dev and
    example links pointed at the bundle beside the copy, with the website
    prompt's rules."""
    with tempfile.TemporaryDirectory() as scratch:
        subprocess.run([str(compiler), 'new', 'todo'], cwd=scratch, check=True, capture_output=True)
        text = (Path(scratch) / 'todo/AGENTS.md').read_text()
    text = local_examples(local_links(text, bundle_pages(bundle), '../docs/'), bundle, '../docs/')
    return text.rstrip('\n') + '\n' + WEBSITE_RULES


def real_git():
    """git itself, not macOS's /usr/bin shim, which needs a cache folder a
    sandbox doesn't give."""
    found = tool('git')
    if found == '/usr/bin/git' and shutil.which('xcrun'):
        done = subprocess.run(['xcrun', '-f', 'git'], capture_output=True, text=True, check=False)
        if done.returncode == 0 and done.stdout.strip():
            return done.stdout.strip()
    return found


# The programs the agent runs to check its work (`jadpo`; `npm` and `npx`),
# each wrapped: same output and exit code, but first a snapshot of the copy
# (committed in a git folder of its own, so the copy's history and diff are
# untouched) before a check or a test, and then one line in calls.jsonl.
# Codex's own record leaves out commands that exit 1 inside a script, so for
# Codex these logs are what says which checks and tests failed.
WRAPPER = r"""#!/bin/sh
trace=@trace@; real=@real@; app=@app@; git=@git@; name=@name@
start=$(/usr/bin/perl -MTime::HiRes=time -e 'printf "%.3f", time' 2>/dev/null)
case "$name $1" in @snap@)
  "$git" --git-dir="$trace/snapshots.git" --work-tree="$app" add -A >/dev/null 2>&1 &&
    "$git" --git-dir="$trace/snapshots.git" --work-tree="$app" -c user.name=harness -c user.email=harness@example.invalid \
      commit -q --allow-empty -m "before $name $*" >/dev/null 2>&1;;
esac
call="$trace/call.$$"
mkfifo "$call.out" "$call.err" 2>/dev/null || exec "$real" "$@"
tee "$call.stdout" < "$call.out" &
tee "$call.stderr" < "$call.err" >&2 &
"$real" "$@" > "$call.out" 2> "$call.err"
code=$?
wait
/usr/bin/perl -MJSON::PP -MTime::HiRes=time -e '
  my ($name, $start, $code, $call, @args) = @ARGV; my $said = "";
  for my $part ("stdout", "stderr") { if (open(my $f, "<", "$call.$part")) { local $/; $said .= <$f>; } }
  utf8::decode($said); utf8::decode($_) for @args;
  my %seen; my @rules = grep { !$seen{$_}++ } ($said =~ /^\s*rule: (\S+)/mg, $said =~ /"rule"\s*:\s*"([^"]+)"/g);
  my $kept = length($said) > 6000 ? substr($said, 0, 3000) . "\n...\n" . substr($said, -3000) : $said;
  print JSON::PP->new->canonical->ascii->encode({name => $name, args => [@args], start => $start + 0,
    seconds => sprintf("%.3f", time - $start) + 0, exit => $code + 0, rules => [@rules], output => $kept}), "\n";' \
    "$name" "$start" "$code" "$call" "$@" >> "$trace/calls.jsonl" 2>/dev/null
rm -f "$call.out" "$call.err" "$call.stdout" "$call.stderr"
exit $code
"""
# Which calls get a snapshot first (a `case` pattern on "<program> <first argument>").
SNAPSHOT_BEFORE = {'jadpo': '"jadpo check"|"jadpo test"', 'npm': '"npm run"|"npm test"|"npm t"|"npm run-script"',
                   'npx': '"npx "*'}


def install_wrapper(folder, app, programs):
    """`programs` maps each wrapped program's name to the real one; the
    wrappers go in folder/bin, which leads the agent's PATH."""
    trace = folder / 'tmp/jadpo-calls'
    trace.mkdir(parents=True)
    subprocess.run([real_git(), 'init', '-q', '--bare', str(trace / 'snapshots.git')], check=True)
    (folder / 'compiler').mkdir(exist_ok=True)
    (folder / 'bin').mkdir()
    for name, real in programs.items():
        if name == 'jadpo':
            shutil.copy2(real, folder / 'compiler/jadpo')
            real = folder / 'compiler/jadpo'
        text = WRAPPER
        for key, value in {'trace': trace, 'real': real, 'app': app, 'git': real_git(), 'name': name,
                           'snap': SNAPSHOT_BEFORE[name]}.items():
            text = text.replace(f'@{key}@', value if key == 'snap' else shlex.quote(str(value)))
        wrapper = folder / 'bin' / name
        wrapper.write_text(text)
        wrapper.chmod(0o755)


def jadpo_ignore(directory, names, app='todo'):
    top = Path(directory).resolve() == APPS[app]['jadpo'].resolve()
    return [n for n in names if (top and n in APPS[app]['withheld']) or n in {'build', '.jadpo', '__pycache__'}]


# What a TypeScript agent isn't told by the app's own docs, which say its
# runner starts a PostgreSQL server: `jadpo test` finds the provided one itself.
TYPESCRIPT_SANDBOX_NOTE = """
## In this environment

A PostgreSQL server is provided, reached through a Unix socket: `JADPO_TEST_DATABASE_URL` names it, and
`TEST_DATABASE_URL` names an empty database on it. The sandbox can't start another PostgreSQL server or
listen on a network port.
"""


# What a Jadpo agent isn't told by `jadpo new`'s AGENTS.md: as for the
# TypeScript copy, the sandbox refuses a listening port, so `jadpo dev` can't
# start (10.6: three Sol runs met `cli.dev_start_failed` and one stopped on it).
JADPO_SANDBOX_NOTE = """
## In this environment

The sandbox can't listen on a network port, so `jadpo dev` can't start here.
"""


def prepare(change, language, folder, compiler):
    """A fresh copy of the app, its own git history to diff against, and for
    Jadpo the docs bundle and the compiler beside it. Nothing else of the
    repository is reachable from the copy."""
    if folder.exists():
        shutil.rmtree(folder)
    app = folder / 'app'
    (folder / 'tmp').mkdir(parents=True)
    which = app_of(change)
    if language == 'jadpo':
        shutil.copytree(APPS[which]['jadpo'], app, ignore=lambda directory, names: jadpo_ignore(directory, names, which))
        bundle = folder / 'docs'
        subprocess.run([sys.executable, str(ROOT / 'tools/docs/language_docs.py'), '--bundle', str(bundle)], cwd=ROOT,
                       check=True, stdout=subprocess.DEVNULL)
        (bundle / 'llms.txt').write_text(local_links((ROOT / 'llms.txt').read_text(), bundle_pages(bundle), ''))
        copy_examples(bundle)
        (app / 'AGENTS.md').write_text(jadpo_agents_file(compiler, bundle) + JADPO_SANDBOX_NOTE)
        install_wrapper(folder, app, {'jadpo': compiler})
    else:
        shutil.copytree(APPS[which]['typescript'], app, ignore=shutil.ignore_patterns(*TYPESCRIPT_SKIPPED))
        if which == 'desk':
            # The product's description, which the Jadpo copy has as its README.
            shutil.copy2(HELPDESK / 'README.md', app / 'PRODUCT.md')
        install_typescript(app)
        (app / 'AGENTS.md').write_text((app / 'AGENTS.md').read_text().rstrip('\n') + '\n' + TYPESCRIPT_SANDBOX_NOTE)
        install_wrapper(folder, app, {'npm': tool('npm'), 'npx': tool('npx')})
    (app / '.gitignore').write_text('node_modules/\nbuild/\n.jadpo/\n.wrangler/\n')
    git(app, 'init', '-q', '-b', 'main')
    git(app, 'add', '-A')
    git(app, '-c', 'user.name=harness', '-c', 'user.email=harness@example.invalid', 'commit', '-q', '-m', 'start')
    # As in a real project: the app is committed on main and the agent works
    # on a branch, so `jadpo check` has a before side and judges acceptances.
    git(app, 'tag', 'start')
    git(app, 'checkout', '-q', '-b', 'change')
    return app


def git(app, *arguments):
    return subprocess.run(['git', *arguments], cwd=app, check=True, capture_output=True, text=True).stdout


def final_diff(app, base='main'):
    """The change as a reviewer sees it, against the branch it will merge
    into, the agent's own commits included: THINKING.md, the tuning runs'
    notes, is not part of it."""
    git(app, 'add', '-A')
    return git(app, 'diff', '--cached', '--no-color', '--find-renames', base, '--', '.', ':(exclude)THINKING.md')


def agent_environment(folder, database_url, loopback=True):
    """Only what a fresh terminal has: no variable of the host session (a
    parent Claude Code's CLAUDE_*, ANTHROPIC_BASE_URL) and nothing naming the
    repository. A sandbox that can't listen on loopback says so, and the
    one test that needs it skips instead of failing for a reason no change
    could fix."""
    path = [str(folder / 'bin')] if (folder / 'bin').is_dir() else []
    for name in ['node', 'npm', 'bun', 'git', 'psql']:
        with contextlib.suppress(RuntimeError):
            directory = str(Path(real_git() if name == 'git' else tool(name)).parent)
            if directory not in path:
                path.append(directory)
    path += ['/usr/local/bin', '/usr/bin', '/bin', '/usr/sbin', '/sbin']
    environment = {key: os.environ[key] for key in ['HOME', 'USER', 'LOGNAME', 'SHELL', 'TMPDIR', 'LANG'] if key in os.environ}
    environment.update({'PATH': ':'.join(path), 'TERM': 'dumb', 'NO_COLOR': '1',
                        'TEST_DATABASE_URL': database_url, 'JADPO_TEST_DATABASE_URL': database_url,
                        # A fresh home makes npm check for its own update on the first
                        # command, and the sandbox kills a command that reaches the network.
                        'NPM_CONFIG_UPDATE_NOTIFIER': 'false'})
    if not loopback:
        environment['TEST_LOOPBACK'] = 'unavailable'
    return environment


def system_prompt(app):
    return 'The project instructions, AGENTS.md:\n\n' + (app / 'AGENTS.md').read_text()


def agent_command(agent, settings, prompt, app, folder, resume=None, tools=TOOLS):
    if agent == 'codex':
        return codex_command(settings, prompt, resume)
    if agent != 'claude':
        return [*shlex.split(agent), prompt]
    command = [claude_code(), '-p', prompt, '--model', settings['model'], '--effort', settings['effort'],
               '--output-format', 'stream-json', '--verbose', '--safe-mode', '--restricted', '--strict-mcp-config',
               '--disable-slash-commands', '--settings', str(folder / CLAUDE_SETTINGS),
               '--tools', tools, *(['--allowedTools', tools] if tools else []), '--permission-mode', 'acceptEdits', '--permission-prompts', 'none',
               *(['--max-budget-usd', f'{settings["budget_usd"]:.2f}'] if settings.get('budget_usd') is not None else []),
               *(['--append-system-prompt', system_prompt(app)] if (app / 'AGENTS.md').is_file() else [])]
    if (folder / 'docs').is_dir():
        command += ['--add-dir', str(folder / 'docs')]
    if resume:
        command += ['--resume', resume]
    return command


# Codex: `codex exec` with a per-run CODEX_HOME holding only the login and the
# pinned config. Its permission profile lets the shell write the copy, read the
# docs bundle, the compiler and the tools on PATH, and reach the run's
# PostgreSQL socket; nothing else of the home folder, no temporary folder of
# the host, and no network (a proxy that allows no domain).

def codex_command(settings, prompt, resume=None):
    command = [tool('codex'), 'exec', *(['resume'] if resume else []), '--json', '--strict-config', '--ignore-rules',
               '--model', settings['model'], '-c', f'model_reasoning_effort="{settings["effort"]}"']
    return command + ([resume] if resume else []) + [prompt]


def linked_folders(binary):
    """The install folders of a binary's libraries outside the system's own
    (Homebrew's OpenSSL for the compiler), as linked and as resolved."""
    done = subprocess.run(['otool', '-L', str(binary)], capture_output=True, text=True, check=False)
    folders = []
    for line in done.stdout.splitlines()[1:]:
        library = Path(line.strip().split(' (')[0])
        if library.parts[1:2] in (('usr',), ('System',)) and not library.is_relative_to('/usr/local'):
            continue
        for path in (library, library.resolve()):
            prefix = path.parent.parent if path.parent.name == 'lib' else path.parent
            # The sandbox grants a link's target, not the link: grant the
            # folder holding the link (Homebrew's opt/) so it resolves.
            prefix = prefix.parent if prefix.is_symlink() else prefix
            if prefix not in folders:
                folders.append(prefix)
    return folders


def codex_readable(folder, environment):
    """Folders the agent's shell reads besides the system's own: the docs
    bundle, the compiler and its libraries, and each tool's install folder on
    PATH."""
    # Codex puts its own helpers (CODEX_HOME/tmp) first on the shell's PATH;
    # the login beside them stays unreadable.
    readable = [folder / 'docs', folder / 'bin', folder / 'compiler', folder / 'codex-home/tmp',
                Path('/System/Library/Perl'), Path(real_git()).parent.parent.parent]
    if (folder / 'compiler/jadpo').is_file() and shutil.which('otool'):
        readable += [path for path in linked_folders(folder / 'compiler/jadpo') if path not in readable]
    for entry in environment['PATH'].split(':'):
        directory = Path(entry)
        if directory in {Path(p) for p in ('/usr/bin', '/bin', '/usr/sbin', '/sbin')} or directory.is_relative_to(folder):
            continue
        prefix = directory.parent if directory.name == 'bin' else directory
        if prefix not in readable:
            readable.append(prefix)
    return readable


def codex_config(settings, folder, environment, tools=True):
    def text(value):
        return json.dumps(str(value))
    lines = [f'model = {text(settings["model"])}', f'model_reasoning_effort = {text(settings["effort"])}',
             # The most detailed reasoning summaries; raw reasoning isn't given.
             'model_reasoning_summary = "detailed"',
             f'service_tier = {text(settings["service_tier"])}', 'approval_policy = "never"', 'web_search = "disabled"',
             'cli_auth_credentials_store = "file"', 'check_for_update_on_startup = false',
             # Commands run as `zsh -c` with the harness's PATH, not a login
             # shell that adds /etc/paths folders the sandbox can't read.
             'allow_login_shell = false',
             'default_permissions = "agent-run"', '', '[permissions.agent-run.filesystem]', '":minimal" = "read"',
             '"/System/Library/OpenSSL" = "read"', '"/tmp" = "none"', '"/private/tmp" = "none"',
             *[f'{text(folder / name)} = "write"' for name in ('app', 'tmp', 'home')],
             *[f'{text(path)} = "read"' for path in codex_readable(folder, environment)],
             '', '[permissions.agent-run.network]', 'enabled = true', 'domains = {}',
             f'unix_sockets = {{ {text(socket_folder(folder))} = "allow" }}', '',
             '[skills]', 'include_instructions = false', '', '[skills.bundled]', 'enabled = false', '',
             '[features]', 'network_proxy = true', *[f'{name} = false' for name in CODEX_FEATURES_OFF],
             # The knowledge probe: no shell, no files, nothing to look anything up with.
             *([] if tools else [f'{name} = false' for name in ('shell_tool', 'unified_exec', 'view_image')])]
    return '\n'.join(lines) + '\n'


def codex_environment(folder):
    # zsh writes here documents under TMPPREFIX (default /tmp/zsh), which the
    # sandbox refuses, so it gets the run's own temporary folder too.
    return {'CODEX_HOME': str(folder / 'codex-home'), 'HOME': str(folder / 'home'), 'TMPDIR': str(folder / 'tmp'),
            'TMPPREFIX': str(folder / 'tmp/zsh')}


@contextlib.contextmanager
def codex_home(folder, settings, environment, tools=True):
    """A fresh CODEX_HOME for one run: a 0600 copy of the login from
    ~/.codex (never printed, never kept), the pinned config, nothing else. Its
    session record is kept as rollout.jsonl and the folder deleted
    afterwards."""
    home, login = folder / 'codex-home', Path.home() / '.codex/auth.json'
    if not login.is_file():
        raise SystemExit('Codex is not signed in: ~/.codex/auth.json is missing')
    for name in ('tmp', 'home'):
        (folder / name).mkdir(parents=True, exist_ok=True)
    home.mkdir(mode=0o700)
    descriptor = os.open(home / 'auth.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'wb') as copy:
        copy.write(login.read_bytes())
    (home / 'config.toml').write_text(codex_config(settings, folder, environment, tools))
    try:
        yield home
    finally:
        keep_refreshed_login(home / 'auth.json', login)
        shutil.rmtree(home, ignore_errors=True)


def save_rollout(folder):
    """Codex's own record of the sessions so far, as rollout.jsonl."""
    with (folder / 'rollout.jsonl').open('w') as rollout:
        for record in sorted((folder / 'codex-home/sessions').rglob('*.jsonl')):
            rollout.write(record.read_text())


def keep_refreshed_login(used, login):
    """If Codex refreshed the login during the run, the old refresh token is
    spent: put the newer login back where it came from."""
    if used.is_symlink() or not used.is_file():
        return
    try:
        fresh, current = json.loads(used.read_text()), json.loads(login.read_text())
    except (OSError, ValueError):
        return
    if str(fresh.get('last_refresh') or '') > str(current.get('last_refresh') or ''):
        staged = login.with_name('auth.json.agent-changes')
        staged.write_text(used.read_text())
        staged.chmod(0o600)
        os.replace(staged, login)


# Claude Code: `claude -p` with a per-run CLAUDE_CONFIG_DIR that starts empty
# (no CLAUDE.md, memory, plugins, skills, hooks or MCP servers; --safe-mode and
# --restricted load none either), the subscription token in its environment,
# and its shell in Claude Code's own sandbox: reads of the home folder, /tmp
# and other volumes refused except the run's copy, docs, compiler and tools;
# writes to the copy alone; no network but the run's PostgreSQL socket.
# The sandbox covers shell commands; --restricted keeps the file tools to the
# copy and the docs bundle.

CLAUDE_SETTINGS = 'claude-settings.json'
CLAUDE_ENVIRONMENT = {'CLAUDE_CODE_SUBPROCESS_ENV_SCRUB': '1', 'CLAUDE_CODE_DISABLE_AUTO_MEMORY': '1',
                      'CLAUDE_CODE_DISABLE_CLAUDE_MDS': '1', 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1',
                      'DISABLE_AUTOUPDATER': '1'}


# Claude Code keeps its own temporary files, the shell's working-directory
# file after every command among them, in a per-uid folder under
# CLAUDE_CODE_TMPDIR (`<it>/claude-<uid>`). Its sandboxed commands use that
# folder only while it fits in 44 bytes, the room a Unix socket path leaves;
# past that they fall back to /tmp/claude-<uid>, which the sandbox refuses,
# and every command comes back failed. A run folder is far longer, so a
# Claude run's temporary folder is a short one of its own outside it:
# ~/.jt/<6 hex>, 0700, made when the session starts and removed when it ends.
CLAUDE_TMP_ROOT = Path.home() / '.jt'
CLAUDE_TMPS = {}
CLAUDE_TMP_LIMIT = 44 - len('/claude-') - len(str(os.getuid()))


def claude_tmp(folder):
    """This run's temporary folder for Claude Code, named once per run."""
    path = CLAUDE_TMP_ROOT / CLAUDE_TMPS.setdefault(str(folder), secrets.token_hex(3))
    if len(str(path).encode()) > CLAUDE_TMP_LIMIT:
        raise SystemExit(f'{path} is over {CLAUDE_TMP_LIMIT} bytes: Claude Code would put its temporary files in /tmp, '
                         'which the sandbox refuses')
    return path


def claude_environment(folder):
    tmp = claude_tmp(folder)
    return {'CLAUDE_CONFIG_DIR': str(folder / 'claude-home'), 'HOME': str(folder / 'home'), 'TMPDIR': str(tmp),
            'TMPPREFIX': str(tmp / 'zsh'), 'CLAUDE_CODE_TMPDIR': str(tmp), **CLAUDE_ENVIRONMENT}


# Plugins the pinned Claude Code ships and loads even in safe mode; the run's
# settings turn each off, and the probes fail on any plugin a session lists.
CLAUDE_BUILT_IN_PLUGINS = ('cc-plugin-agents-md', 'cc-plugin-privacy', 'cc-plugin-plugin-authoring')


def claude_settings(folder, environment, sockets=()):
    own = [str(folder / 'app'), str(claude_tmp(folder)), str(folder / 'home')]
    readable = [str(path) for path in codex_readable(folder, environment) if 'codex-home' not in path.parts]
    return {'sandbox': {'enabled': True, 'autoAllowBashIfSandboxed': True, 'allowUnsandboxedCommands': False,
                        'filesystem': {'denyRead': [str(Path.home()), '/Users', '/Volumes', '/tmp', '/private/tmp',
                                                    '/private/var/folders'],
                                       'allowRead': own + [path for path in readable if path not in own],
                                       'allowWrite': own},
                        'network': {'allowedDomains': [], 'allowUnixSockets': [str(path) for path in sockets],
                                    'allowLocalBinding': False}},
            'permissions': {'defaultMode': 'acceptEdits', 'allow': TOOLS.split(','), 'deny': ['WebFetch', 'WebSearch']},
            'enabledPlugins': {plugin + '@builtin': False for plugin in CLAUDE_BUILT_IN_PLUGINS}}


# Claude Code is pinned for the measurement: one exact release, installed for
# the benchmark alone, never the owner's own CLI. The probes and every run use
# the same binary, and each result records its version.
CLAUDE_CODE_VERSION = '2.1.326'
CLAUDE_CODE_PATH = 'JADPO_CLAUDE_CODE'


@functools.cache
def claude_code():
    """The pinned Claude Code binary, by the absolute path in JADPO_CLAUDE_CODE."""
    install = (f'install it for the benchmark alone with `npm install --prefix <dir> --save-exact '
               f'@anthropic-ai/claude-code@{CLAUDE_CODE_VERSION}` and set {CLAUDE_CODE_PATH} to <dir>/node_modules/.bin/claude')
    path = os.environ.get(CLAUDE_CODE_PATH, '')
    if not path or not Path(path).is_absolute() or not Path(path).is_file():
        raise SystemExit(f'{CLAUDE_CODE_PATH} must be the absolute path of Claude Code {CLAUDE_CODE_VERSION}: {install}')
    done = subprocess.run([path, '--version'], capture_output=True, text=True, check=False)
    if done.stdout.split()[:1] != [CLAUDE_CODE_VERSION]:
        raise SystemExit(f'{path} is Claude Code {done.stdout.strip() or "of no known version"}, '
                         f'not the pinned {CLAUDE_CODE_VERSION}: {install}')
    return path


def claude_token():
    """The owner's subscription token, read fresh for each session."""
    if not CLAUDE_TOKEN.is_file():
        raise SystemExit(f'Claude is not signed in for runs: save a `claude setup-token` token to {CLAUDE_TOKEN} (0600)')
    if stat_mode(CLAUDE_TOKEN) & 0o077:
        raise SystemExit(f'{CLAUDE_TOKEN} is readable by others; chmod 600 it')
    return CLAUDE_TOKEN.read_text().strip()


def stat_mode(path):
    return os.stat(path).st_mode & 0o777


@contextlib.contextmanager
def claude_home(folder, settings, environment, sockets=()):
    """An empty CLAUDE_CONFIG_DIR for one run and the sandbox settings beside
    it. Claude Code's own record of the conversation is kept as
    claude-session/ and the config folder deleted afterwards."""
    claude_token()
    home, tmp = folder / 'claude-home', claude_tmp(folder)
    if tmp.exists():
        raise SystemExit(f'{tmp} already exists: another run has it, or a run left it behind')
    (folder / 'home').mkdir(parents=True, exist_ok=True)
    CLAUDE_TMP_ROOT.mkdir(mode=0o700, exist_ok=True)
    tmp.mkdir(mode=0o700)
    try:
        home.mkdir(mode=0o700)
        (folder / CLAUDE_SETTINGS).write_text(json.dumps(claude_settings(folder, environment, sockets), indent=2) + '\n')
        yield home
    finally:
        save_claude_session(folder)
        shutil.rmtree(home, ignore_errors=True)
        shutil.rmtree(tmp, ignore_errors=True)
        # ~/.jt itself stays: runs side by side make and remove their own
        # folders in it, and it is on no sandbox list.
        CLAUDE_TMPS.pop(str(folder), None)


def save_claude_session(folder):
    """Claude Code's own record of the sessions so far, as claude-session/."""
    projects = folder / 'claude-home/projects'
    if projects.is_dir():
        shutil.copytree(projects, folder / 'claude-session', dirs_exist_ok=True)


def sandboxed(agent):
    return agent in AGENTS


def agent_extra_environment(agent, folder):
    return codex_environment(folder) if agent == 'codex' else claude_environment(folder) if agent == 'claude' else {}


def agent_home(agent, folder, settings, environment, server=None, tools=True):
    if agent == 'codex':
        return codex_home(folder, settings, environment, tools)
    if agent == 'claude':
        sockets = [server.socket, server.socket / f'.s.PGSQL.{server.port}'] if server and server.socket else []
        return claude_home(folder, settings, environment, sockets)
    return contextlib.nullcontext()


def save_record(agent, folder):
    if agent == 'codex':
        save_rollout(folder)
    elif agent == 'claude':
        save_claude_session(folder)


def sandbox_record(agent, settings, folder, environment):
    if agent == 'codex':
        return codex_config(settings, folder, environment)
    if agent == 'claude' and (folder / CLAUDE_SETTINGS).is_file():
        return (folder / CLAUDE_SETTINGS).read_text()
    return None


def session(agent, settings, prompt, app, folder, environment, transcript, resume=None, budget=None, minutes=None,
            tools=TOOLS):
    if settings.get('budget_usd') is not None:
        settings = {**settings, 'budget_usd': budget if budget is not None else settings['budget_usd']}
    if agent == 'claude':
        # The login reaches Claude Code alone: the scrub strips it from every
        # command the agent runs, and the token file is outside the sandbox.
        environment = environment | {'CLAUDE_CODE_OAUTH_TOKEN': claude_token()}
    command = agent_command(agent, settings, prompt, app, folder, resume, tools)
    started = time.monotonic()
    with transcript.open('a') as output, (folder / 'agent.stderr').open('a') as errors:
        try:
            subprocess.run(command, cwd=app, env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                           timeout=(minutes if minutes is not None else settings['timeout_minutes']) * 60, check=False)
            timed_out = False
        except subprocess.TimeoutExpired:
            timed_out = True
    return {'seconds': round(time.monotonic() - started, 1), 'timed_out': timed_out}


def agent_version(agent):
    if agent not in AGENTS:
        return agent
    done = subprocess.run([claude_code() if agent == 'claude' else tool(agent), '--version'], capture_output=True,
                          text=True, check=False)
    return done.stdout.strip() if agent == 'codex' else 'claude ' + done.stdout.strip()


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
    if app_of(change) == 'desk':
        return []  # the helpdesk's rule probes are among each change's own tests
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
            harness = (desk.Desk if app_of(change) == 'desk' else contract.Contract)(driver)
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


@contextlib.contextmanager
def native_lock(folder, name='native'):
    """One run at a time builds the contract's native driver: every run
    builds it into the same target folder, so runs side by side would
    replace each other's driver."""
    import fcntl
    folder.mkdir(parents=True, exist_ok=True)
    with (folder / f'{name}.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def install_typescript(app, run=subprocess.run):
    """npm ci, one run at a time, then the pristine app's own typecheck:
    npm ci has exited 0 leaving node_modules short of files, and an agent
    then spends its session on a broken install. One retry, then the run
    fails as the harness's fault."""
    quiet = {'cwd': app, 'stdout': subprocess.DEVNULL, 'stderr': subprocess.DEVNULL}
    with native_lock(ROOT / 'target/agent-changes', 'npm'):
        for _ in range(2):
            run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], check=True, **quiet)
            if run(['npm', 'run', 'check'], check=False, **quiet).returncode == 0:
                return
    raise RuntimeError(f'npm ci left an incomplete node_modules in {app} twice')


def own_driver(build, work):
    """The native driver `build` gives, built under the lock and copied into
    the run's own folder, so a run beside it rebuilding the shared one
    leaves this run's alone."""
    with native_lock(ROOT / 'target/agent-changes'):
        built = Path(build())
        own = work / built.name
        shutil.copy2(built, own)
    return own


def hidden_jadpo(app, change, work, server, compiler, diff, pristine=False):
    cases = regression_cases(change)
    driver = None
    try:
        runner = load('agent_contract_run', CONTRACT / 'run.py', CONTRACT)
        runner.PROJECT, runner.WORK, runner.CACHE = app, work / 'contract', ROOT / 'target/agent-changes'
        runner.WORK.mkdir(parents=True, exist_ok=True)
        binary = own_driver(lambda: runner.build_native(ROOT), runner.WORK)
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
        shutil.copytree(APPS[app_of(change)]['typescript'] / 'adapter', app / 'adapter',
                        ignore=shutil.ignore_patterns('__pycache__'))
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


def run(change, language, number, label, settings, agent='claude', keep=False, frozen=None):
    folder = WORK / label / run_name(change.id, language, number)
    if agent not in AGENTS:
        agent = stand_in(agent, language)
    codex, closed = agent == 'codex', sandboxed(agent)
    compiler = jadpo_binary()
    app = prepare(change, language, folder, compiler)
    server = Server(folder / 'postgres', unix=socket_folder(folder) if closed else None)
    try:
        database = server.create()
        environment = agent_environment(folder, server.socket_url(database, language) if closed else server.url(database),
                                        loopback=not closed)
        environment |= agent_extra_environment(agent, folder)
        transcript = folder / 'transcript.jsonl'
        with agent_home(agent, folder, settings, environment, server):
            timings = [session(agent, settings, task_prompt(change), app, folder, environment, transcript)]
            questions = app / 'QUESTIONS.md'
            asked = questions.is_file() and questions.read_text().strip() != ''
            # One reply, inside the run's time (and, for Claude, its budget).
            minutes = settings['timeout_minutes'] - timings[0]['seconds'] / 60
            identifier = session_id(transcript, codex, language)
            if asked and minutes > 0 and identifier:
                first = scoring.parse_transcript(transcript.read_text().splitlines(), language) if not codex else None
                remaining = max(0.0, settings['budget_usd'] - first['cost_usd']) if settings.get('budget_usd') is not None else None
                if remaining is None or remaining > 0:
                    timings.append(session(agent, settings, owner_reply(change), app, folder,
                                           environment, transcript, resume=identifier, budget=remaining, minutes=minutes))
            save_record(agent, folder)
            diff = final_diff(app)
            thinking = (app / THINKING_FILE).read_text() if (app / THINKING_FILE).is_file() else None
            visible = visible_gate(language, app, environment)
            reports = hidden(language, app, change, folder / 'hidden', server, compiler, diff)
            result = score(change, language, number, label, settings, transcript, diff, visible, reports, asked, timings,
                           folder)
            result |= {'agent': agent_version(agent), 'set': set_of(change.id), 'frozen': frozen,
                       'thinking': thinking}
            if closed:
                result['sandbox'] = sandbox_record(agent, settings, folder, environment)
            # After scoring, and apart from it: what the agent says got in its way.
            if identifier and agent in AGENTS:
                result['debrief'] = debrief(agent, settings, app, folder, environment, identifier, result['tokens'])
    finally:
        server.stop()
    keep_raw(folder, label, result)
    if not keep:
        for heavy in ['app/node_modules', 'app/build', 'hidden', 'postgres', 'bin', 'compiler', 'docs', 'tmp', 'home']:
            shutil.rmtree(folder / heavy, ignore_errors=True)
    return result


THINKING_FILE = scoring.THINKING_FILE


# A session: several changes in one conversation --------------------------------

def questions_text(app):
    questions = app / 'QUESTIONS.md'
    return questions.read_text().strip() if questions.is_file() else ''


def thread_usage(transcript):
    """Codex's usage for the whole thread so far, from its last turn."""
    last = [json.loads(line) for line in transcript.read_text().splitlines() if '"turn.completed"' in line] \
        if transcript.is_file() else []
    return scoring.codex_usage(last[-1].get('usage') if last else {})


def line_count(path):
    return len(path.read_text(errors='replace').splitlines()) if path.is_file() else 0


def rollout_spent(path, upto):
    """The thread's running token total in Codex's record before line `upto`,
    counted as scoring.parse_codex counts it."""
    spent = 0
    for line in (path.read_text(errors='replace').splitlines()[:upto] if path.is_file() else []):
        if '"token_count"' not in line:
            continue
        try:
            payload = json.loads(line).get('payload') or {}
        except ValueError:
            continue
        usage = (payload.get('info') or {}).get('total_token_usage') or {}
        if usage:
            spent = int(usage.get('total_tokens') or scoring.total(scoring.codex_usage(usage)))
    return spent


def per_change(result, before):
    """A session step's tokens before each command, and to green, less what
    the thread had spent before the step began."""
    for item in result.get('verifications') or []:
        item['tokens_before'] = max(0, (item.get('tokens_before') or 0) - before)
    result['green'] = scoring.to_green(result['verifications'], result['edits'])


def tail_file(source, start, destination):
    lines = source.read_text(errors='replace').splitlines(keepends=True) if source.is_file() else []
    destination.write_text(''.join(lines[start:]))
    return destination


def run_session(name, language, number, label, settings, agent='codex', keep=False, frozen=None):
    """One session's changes, one after another in the same conversation: the
    first starts it and each later one resumes it, so what the first met has
    scrolled back by the last. After each change the harness commits the app,
    as a merge would, and scores that change on the app as it stands; at the
    end every change of the session is scored again on the final app, which
    shows a rule a later change broke. One result per change, as for a
    single-change run, each with the session's name and its step."""
    ids = desk_set.SESSIONS[name]
    changes = [desk_set.BY_ID[i] for i in ids]
    folder = WORK / label / run_name(name, language, number)
    if agent not in AGENTS:
        agent = stand_in(agent, language)
    codex, closed = agent == 'codex', sandboxed(agent)
    compiler = jadpo_binary()
    app = prepare(changes[0], language, folder, compiler)
    server = Server(folder / 'postgres', unix=socket_folder(folder) if closed else None)
    results = []
    try:
        database = server.create()
        environment = agent_environment(folder, server.socket_url(database, language) if closed else server.url(database),
                                        loopback=not closed)
        environment |= agent_extra_environment(agent, folder)
        transcript = folder / 'transcript.jsonl'
        calls = folder / 'tmp/jadpo-calls/calls.jsonl'
        with agent_home(agent, folder, settings, environment, server):
            identifier, spent, marks, cost_so_far = None, scoring.codex_usage({}), [], {}
            for step, change in enumerate(changes, 1):
                marks.append({'transcript': line_count(transcript), 'calls': line_count(calls),
                              'rollout': line_count(folder / 'rollout.jsonl')})
                before = questions_text(app)
                timings = [session(agent, settings, task_prompt(change), app, folder, environment, transcript,
                                   resume=identifier)]
                identifier = identifier or session_id(transcript, codex, language)
                asked = questions_text(app) not in ('', before)
                minutes = settings['timeout_minutes'] - timings[0]['seconds'] / 60
                if asked and minutes > 0 and identifier:
                    timings.append(session(agent, settings, owner_reply(change), app, folder,
                                           environment, transcript, resume=identifier, minutes=minutes))
                save_record(agent, folder)
                diff = final_diff(app)
                git(app, '-c', 'user.name=harness', '-c', 'user.email=harness@example.invalid', 'commit', '-q',
                    '--allow-empty', '-m', f'{change.id}: {change.title}')
                # The gate a pull request meets: main is still the step before,
                # so `jadpo check` judges this change's acceptances.
                visible = visible_gate(language, app, environment)
                # The change merges: the next one's before side is this one's result.
                git(app, 'branch', '-f', 'main', 'HEAD')
                reports = hidden(language, app, change, folder / 'hidden' / change.id, server, compiler, diff)
                mark = marks[-1]
                step_folder = folder / 'steps' / run_name(change.id, language, number)
                (step_folder / 'tmp/jadpo-calls').mkdir(parents=True, exist_ok=True)
                step_transcript = tail_file(transcript, mark['transcript'], step_folder / 'transcript.jsonl')
                tail_file(calls, mark['calls'], step_folder / 'tmp/jadpo-calls/calls.jsonl')
                if codex:
                    tail_file(folder / 'rollout.jsonl', mark['rollout'], step_folder / 'rollout.jsonl')
                result = score(change, language, number, label, settings, step_transcript, diff, visible, reports, asked,
                               timings, step_folder)
                if codex:
                    # Codex counts tokens for the whole thread: before each
                    # command, and to green, a step counts only its own.
                    per_change(result, rollout_spent(folder / 'rollout.jsonl', mark['rollout']))
                    # Codex reports the thread's usage so far: the step is what it added.
                    total = thread_usage(transcript)
                    result['tokens'] = {key: max(0, total[key] - spent.get(key, 0)) for key in total}
                    spent = total
                elif agent == 'claude':
                    # Claude Code reports the conversation's cost so far: the step is what it added.
                    reported = scoring.parse_transcript(step_transcript.read_text().splitlines(), language)['cost_so_far']
                    result['cost_usd'] = round(sum(max(0.0, total - cost_so_far.get(key, 0.0))
                                                   for key, total in reported.items()), 6)
                    cost_so_far |= reported
                result |= {'agent': agent_version(agent), 'set': set_of(change.id), 'frozen': frozen, 'thinking': None,
                           'session': name, 'step': step}
                if closed:
                    result['sandbox'] = sandbox_record(agent, settings, folder, environment)
                results.append(result)
            # Every change again, on the app the whole session left.
            final = final_diff(app, 'start')
            for result, change in zip(results, changes):
                kept = hidden(language, app, change, folder / 'hidden-final' / change.id, server, compiler, final)
                tests = [r for r in kept if r['kind'] != 'regression']
                result['kept_hidden'] = kept
                result['kept'] = scoring.outcome(dataclasses.asdict(change) | {'tests': None}, tests, [], result['outcome']['asked'],
                                                 result['outcome']['visible_green'], result['diff'])
            if identifier and agent in AGENTS:
                spent_all = thread_usage(transcript) if codex else {}
                results[-1]['debrief'] = debrief(agent, settings, app, folder, environment, identifier, spent_all)
    finally:
        server.stop()
    for result in results:
        keep_raw(folder / 'steps' / run_name(result['change'], language, number), label, result)
    # The whole conversation, beside its steps' results.
    raw = RESULTS / label / 'sessions' / run_name(name, language, number)
    raw.mkdir(parents=True, exist_ok=True)
    for source in ['transcript.jsonl', 'rollout.jsonl', 'debrief.jsonl', 'agent.stderr']:
        if (folder / source).is_file():
            shutil.copy2(folder / source, raw / source)
    if (folder / 'claude-session').is_dir():
        shutil.copytree(folder / 'claude-session', raw / 'claude-session', dirs_exist_ok=True)
    if not keep:
        for heavy in ['app/node_modules', 'app/build', 'hidden', 'hidden-final', 'postgres', 'bin', 'compiler', 'docs', 'tmp',
                      'home']:
            shutil.rmtree(folder / heavy, ignore_errors=True)
    return results


def task_prompt(change):
    """The same prompt in both languages; tuning runs add the THINKING.md ask."""
    return PROMPT.format(app=APPS[app_of(change)]['noun'], request=change.request) + (THINKING if change.id in TUNING else '')


DEBRIEF_MINUTES = 10
DEBRIEF_BUDGET_USD = 1.0


def session_id(transcript, codex, language):
    lines = transcript.read_text().splitlines() if transcript.is_file() else []
    parsed = scoring.parse_codex(lines) if codex else scoring.parse_transcript(lines, language)
    return parsed['sessions'][-1]['session_id'] if parsed['sessions'] else None


def debrief(agent, settings, app, folder, environment, identifier, tokens):
    """One more turn of the same session, its own transcript and tokens."""
    transcript = folder / 'debrief.jsonl'
    budget = min(DEBRIEF_BUDGET_USD, settings['budget_usd']) if settings.get('budget_usd') is not None else None
    timing = session(agent, settings, DEBRIEF, app, folder, environment, transcript, resume=identifier, budget=budget,
                     minutes=DEBRIEF_MINUTES)
    lines = transcript.read_text().splitlines() if transcript.is_file() else []
    if agent == 'codex':
        parsed = scoring.parse_codex(lines)
        # Codex reports the thread's usage so far: the debrief is what it added.
        usage = parsed['sessions'][-1]['usage'] if parsed['sessions'] else {}
        spent = {key: max(0, usage.get(key, 0)) for key in tokens}
        last = [json.loads(line) for line in lines if '"turn.completed"' in line]
        if last:
            cumulative = scoring.codex_usage(last[-1].get('usage'))
            spent = {key: max(0, cumulative[key] - tokens.get(key, 0)) for key in cumulative}
    else:
        parsed = scoring.parse_transcript(lines, 'jadpo')
        spent = parsed['tokens']
    text = parsed['sessions'][-1]['text'] if parsed['sessions'] else ''
    return {'text': text, 'tokens': spent, 'seconds': timing['seconds'], 'timed_out': timing['timed_out']}


def keep_raw(folder, label, result):
    """The run's JSON, and beside it a folder of its raw material: the
    transcripts, Codex's record, the jadpo call log, the snapshots before
    each check and test, THINKING.md and the stumbles report."""
    name = folder.name
    destination = RESULTS / label / f'{name}.json'
    raw = RESULTS / label / name
    raw.mkdir(parents=True, exist_ok=True)
    for source in ['transcript.jsonl', 'rollout.jsonl', 'debrief.jsonl', 'agent.stderr', 'tmp/jadpo-calls/calls.jsonl',
                   f'app/{THINKING_FILE}']:
        if (folder / source).is_file():
            shutil.copy2(folder / source, raw / Path(source).name)
    snapshots = folder / 'tmp/jadpo-calls/snapshots.git'
    if snapshots.is_dir() and subprocess.run([real_git(), '--git-dir', str(snapshots), 'rev-parse', '-q', '--verify', 'HEAD'],
                                             capture_output=True).returncode == 0:
        subprocess.run([real_git(), '--git-dir', str(snapshots), 'bundle', 'create', '-q', str(raw / 'snapshots.bundle'),
                        '--all'], check=False, capture_output=True)
    # The docs read, in order: built from the transcript, one JSON line each.
    (raw / 'docs-reads.jsonl').write_text(''.join(json.dumps(read) + '\n' for read in result.get('docs_reads') or []))
    (raw / 'stumbles.md').write_text(scoring.stumbles(result))
    destination.write_text(json.dumps(result, indent=2) + '\n')


def contamination_markers():
    """What a tool call names only when it reaches for the repository or
    the contract. Not the bare `golden-todo`: both apps use it themselves."""
    return [str(ROOT), 'LLM-coding', 'benchmarks/golden-todo', 'acceptance-v0.2', 'adversarial-changes',
            'benchmarks/helpdesk', 'desk_changes', 'parity-reference']


def score(change, language, number, label, settings, transcript, diff, visible, reports, asked, timings, folder):
    forbidden = contamination_markers()
    lines = transcript.read_text().splitlines()
    rollout = folder / 'rollout.jsonl'
    log = folder / 'tmp/jadpo-calls/calls.jsonl'
    jadpo_calls = [json.loads(line) for line in log.read_text(errors='replace').splitlines() if line.strip()] if log.is_file() else []
    if rollout.is_file():
        parsed = scoring.parse_codex(lines, rollout.read_text().splitlines(), language, forbidden)
        parsed = scoring.with_wrapped_calls(parsed, jadpo_calls, language)
    else:
        parsed = scoring.parse_transcript(lines, language, folder, forbidden)
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
        'stream_cutoffs': parsed.get('stream_cutoffs', 0),
        'review': scoring.review_metrics(diff, language), 'visible': visible, 'hidden': reports,
        'contamination': parsed['contamination'], 'subagents': parsed.get('subagents', 0), 'diff': diff,
        'calls': parsed['calls'], 'docs_reads': scoring.docs_reads(parsed['calls'], docs_listing(folder)), 'jadpo_calls': jadpo_calls,
        'reasoning': parsed['reasoning'], 'thinking_tokens_estimate': parsed['thinking_tokens_estimate'],
        'harness_failure': harness_failure(parsed, timings),
    }


def docs_listing(folder):
    """The docs bundle's files, as paths under it, for settling which reads
    found a file."""
    bundle = folder / 'docs'
    return {str(path.relative_to(bundle)) for path in bundle.rglob('*') if path.is_file()} if bundle.is_dir() else None


def harness_failure(parsed, timings):
    """Why a run says nothing about the agent: the session never reached the
    model (signed out, an API error) or never started. Such a run is kept and
    left out of the summary, never scored."""
    if not parsed['sessions']:
        return 'the agent wrote no session result' + (' (timed out)' if any(t['timed_out'] for t in timings) else '')
    first = parsed['sessions'][0]
    if first['is_error'] and not first['cost_usd'] and first['turns'] <= 1 and not parsed['verifications'] and not parsed['edits']:
        return 'the session did not reach the model: ' + first['text'][:200]
    return None


# Commands ----------------------------------------------------------------------

def command_list(_):
    for change in change_set.CHANGES:
        print(f'{change.id}  {change.kind:<11} {change.title}')


def selected(values, which=None):
    every = change_set.CHANGES + desk_set.CHANGES
    pool = [c for c in every if (c.id in SETS[which] if which else c.id in change_set.BY_ID)]
    if not values:
        return pool
    unknown = set(values) - set(change_set.BY_ID) - set(desk_set.BY_ID)
    if unknown:
        raise SystemExit('unknown change: ' + ', '.join(sorted(unknown)))
    outside = [v for v in values if v not in {c.id for c in pool}]
    if outside:
        raise SystemExit(f'not in the {which} set: ' + ', '.join(outside))
    return [change_by_id(v) for v in values]


def check_frozen(changes, frozen):
    """A held-out change runs only against a frozen Jadpo: the revision named
    with --frozen, checked out here."""
    held = [c.id for c in changes if c.id not in TUNING]
    if not held:
        return
    if not frozen:
        raise SystemExit(f'{", ".join(held)} {"is" if len(held) == 1 else "are"} held out until Jadpo is frozen; '
                         'pass --frozen <jadpo revision>')
    named = subprocess.run(['git', 'rev-parse', '--verify', '-q', f'{frozen}^{{commit}}'], cwd=ROOT,
                           capture_output=True, text=True).stdout.strip()
    here = git(ROOT, 'rev-parse', 'HEAD').strip()
    if not named or named != here:
        raise SystemExit(f'--frozen {frozen} is not the revision checked out here ({here[:12]})')


MIN_FREE_GB = 20


def capacity():
    """Free disk where the copies go and the machine's load, checked before
    each run; a run refuses to start without room."""
    WORK.mkdir(parents=True, exist_ok=True)
    free = shutil.disk_usage(WORK).free / 1e9
    load = os.getloadavg()[0]
    print(f'capacity: {free:,.0f} GB free, load {load:.1f} on {os.cpu_count()} cores', file=sys.stderr)
    if free < MIN_FREE_GB:
        raise SystemExit(f'only {free:.0f} GB free under {WORK}; a run needs {MIN_FREE_GB} GB')


def settings_for(args):
    if args.agent == 'codex':
        base = DRY_CODEX if args.dry else PINNED_CODEX
    else:
        base = DRY if args.dry else PINNED
    return {key: getattr(args, key) if getattr(args, key, None) is not None else value for key, value in base.items()}


def command_run(args):
    if args.agent in AGENTS and not args.spend:
        raise SystemExit('agent sessions spend tokens; pass --spend once a maintainer has approved the runs')
    settings = settings_for(args)
    label = args.label or time.strftime('%Y%m%d-%H%M') + ('-dry' if args.dry else '')
    languages = [args.language] if args.language else list(LANGUAGES)
    numbers = [args.run] if args.run else list(range(1, args.runs + 1))
    chosen = selected(args.change, args.set)
    if args.agent in AGENTS:
        check_frozen(chosen, args.frozen)
    for number in numbers:
        for change in chosen:
            for language in languages:
                path = RESULTS / label / f'{run_name(change.id, language, number)}.json'
                if path.exists() and not args.again:
                    print(f'kept: {path.name} (already run; --again reruns it)')
                    continue
                capacity()
                result = run(change, language, number, label, settings, args.agent, args.keep, args.frozen)
                if result['set'] == 'tuning' and args.agent in AGENTS:
                    print((RESULTS / label / path.stem / 'stumbles.md').read_text())
                state = 'correct' if result['outcome']['correct'] else 'incorrect'
                cost = 'subscription' if result['cost_usd'] is None else f'${result["cost_usd"]:.2f}'
                print(f'{state}: {change.id} {language} r{number} · {cost} · '
                      f'{scoring.total(result["tokens"]):,} tokens · results/{label}/{path.name}')
    command_summary(argparse.Namespace(label=label))


def command_session(args):
    """The helpdesk's sessions: each runs its five changes in one conversation."""
    if args.agent in AGENTS and not args.spend:
        raise SystemExit('agent sessions spend tokens; pass --spend once a maintainer has approved the runs')
    settings = settings_for(args)
    label = args.label or time.strftime('%Y%m%d-%H%M') + ('-dry' if args.dry else '')
    languages = [args.language] if args.language else list(LANGUAGES)
    numbers = [args.run] if args.run else list(range(1, args.runs + 1))
    names = args.session or sorted(desk_set.SESSIONS)
    unknown = set(names) - set(desk_set.SESSIONS)
    if unknown:
        raise SystemExit('unknown session: ' + ', '.join(sorted(unknown)))
    if args.agent in AGENTS:
        check_frozen([desk_set.BY_ID[i] for n in names for i in desk_set.SESSIONS[n]], args.frozen)
    for number in numbers:
        for name in names:
            for language in languages:
                path = RESULTS / label / 'sessions' / run_name(name, language, number)
                if path.exists() and not args.again:
                    print(f'kept: {path.name} (already run; --again reruns it)')
                    continue
                capacity()
                for result in run_session(name, language, number, label, settings, args.agent, args.keep, args.frozen):
                    state = 'correct' if result['outcome']['correct'] else 'incorrect'
                    kept = 'kept' if result['kept']['correct'] else 'not kept'
                    print(f'{state} ({kept}): {name} {result["change"]} {language} r{number} · '
                          f'{scoring.total(result["tokens"]):,} tokens')
    command_summary(argparse.Namespace(label=label))


def command_stumbles(args):
    """The stumbles report of each run under a label, or of one run."""
    paths = [RESULTS / args.label / f'{args.run}.json'] if args.run else [
        path for path in sorted((RESULTS / args.label).glob('*.json')) if path.name not in {'summary.json', 'calibration.json'}]
    if not paths or not all(path.is_file() for path in paths):
        raise SystemExit(f'no run results under results/{args.label}' + (f' named {args.run}' if args.run else ''))
    for path in paths:
        print(scoring.stumbles(json.loads(path.read_text())))


KNOWLEDGE_PROMPT = 'What is the Jadpo programming language? Write a small Jadpo program.'


def command_knowledge_probe(args):
    """What the model knows of Jadpo from its training alone: one session in
    an empty folder, no docs, no tools, no network."""
    if not args.spend:
        raise SystemExit('the probe is an agent session; pass --spend')
    settings = dict(PINNED_CODEX if args.agent == 'codex' else PINNED)
    if args.model:
        settings['model'] = args.model
    folder = WORK / args.label / 'knowledge-probe'
    if folder.exists():
        shutil.rmtree(folder)
    app = folder / 'app'
    app.mkdir(parents=True)
    git(app, 'init', '-q', '-b', 'main')
    environment = agent_environment(folder, 'postgresql://unavailable/none') | agent_extra_environment(args.agent, folder)
    transcript = folder / 'transcript.jsonl'
    codex = args.agent == 'codex'
    with agent_home(args.agent, folder, settings, environment, tools=False):
        # Thirty minutes: an API stream that goes quiet mid-answer is retried
        # by Claude Code only after its own ten-minute request timeout.
        timing = session(args.agent, settings, KNOWLEDGE_PROMPT, app, folder, environment, transcript, minutes=30,
                         tools='')
        save_record(args.agent, folder)
    lines = transcript.read_text().splitlines()
    parsed = scoring.parse_codex(lines, (folder / 'rollout.jsonl').read_text().splitlines()) if codex \
        else scoring.parse_transcript(lines, 'jadpo')
    offered = sorted(set(stream_tools(lines))) if not codex else probe_tools((folder / 'rollout.jsonl').read_text().splitlines())
    answer = parsed['sessions'][-1]['text'] if parsed['sessions'] else None
    result = {'prompt': KNOWLEDGE_PROMPT, 'answer': answer,
              'agent': agent_version(args.agent), 'settings': settings, 'tokens': parsed['tokens'],
              'tool_calls': [call['input'][:300] for call in parsed['calls']], 'tools_offered': offered,
              'reasoning': parsed['reasoning'], 'seconds': timing['seconds'],
              # Passes when the model answered with nothing to look anything up
              # with; whether the answer knows Jadpo is read, not scored.
              'passed': bool(answer) and not parsed['calls'] and (codex or not offered)}
    destination = RESULTS / args.label / 'knowledge-probe.json'
    destination.parent.mkdir(parents=True, exist_ok=True)
    for name in ('transcript.jsonl', 'rollout.jsonl'):
        if (folder / name).is_file():
            shutil.copy2(folder / name, destination.parent / f'knowledge-probe.{name}')
    destination.write_text(json.dumps(result, indent=2) + '\n')
    print(result['answer'] or '(no answer)')
    print(f'\n{"passed" if result["passed"] else "FAILED"}: tools offered {offered or "none"}, '
          f'tool calls {len(parsed["calls"])}', file=sys.stderr)
    print(f'results/{args.label}/knowledge-probe.json', file=sys.stderr)
    shutil.rmtree(folder, ignore_errors=True)
    return 0 if result['passed'] else 1


# Claude Code lists its own subagent types in every session's init event. A
# session can start one only through the Agent tool, so they count as offered
# only with it; any other agent name means the run picked up someone's config.
BUILT_IN_AGENTS = ('claude', 'Explore', 'general-purpose', 'Plan')


def stream_tools(lines):
    """The tools, MCP servers, plugins, skills and agents a Claude Code session
    was given, from its stream's init event."""
    for event in events(lines):
        if event.get('type') == 'system' and event.get('subtype') == 'init':
            named = list(event.get('tools') or [])
            named += ['mcp:' + str((server or {}).get('name', server)) for server in event.get('mcp_servers') or []]
            named += ['plugin:' + str((plugin or {}).get('name', plugin)) for plugin in event.get('plugins') or []]
            named += ['skill:' + str(skill) for skill in event.get('skills') or []]
            delegates = {'Agent', 'Task'} & set(named)
            named += ['agent:' + str(agent) for agent in event.get('agents') or []
                      if delegates or agent not in BUILT_IN_AGENTS]
            named += ['command:' + str(command) for command in event.get('slash_commands') or []]
            return named
    return []


# The isolation probe: a Codex session in a prepared copy, asked to try what it
# must not reach. The verdict comes from each command's own result, not from
# what the agent says about it.

def probe_checks(agent='codex', server=None):
    """(name, shell command, what the sandbox must do, what shows it) for the
    probe. Reads of real files outside the copy must fail with the sandbox's
    refusal; writes outside it must leave no file."""
    home = Path.home()
    sessions = next(iter(sorted((home / '.codex/sessions').rglob('*.jsonl'))), None)
    reads = [('repository README', ROOT / 'README.md'), ('main checkout AGENTS.md', ROOT.parent / 'LLM-coding/AGENTS.md'),
             ('another worktree', ROOT.parent / 'LLM-coding-chunk-A17/README.md'),
             ('owner Codex config', home / '.codex/config.toml'), ('owner Codex AGENTS.md', home / '.codex/AGENTS.md'),
             ('owner Codex login', home / '.codex/auth.json'), ('owner Codex session', sessions),
             ('owner Claude instructions', home / '.claude/CLAUDE.md'), ('owner Claude settings', home / '.claude/settings.json'),
             ('owner Claude memory', next(iter(sorted((home / '.claude/projects').glob('*/memory/MEMORY.md'))), None)),
             ('the Claude run token', CLAUDE_TOKEN)]
    checks = [(name, f'head -c 100 {shlex.quote(str(path))}', 'refused', str(path)) for name, path in reads if path and path.exists()]
    own = ([("this run's own login", 'head -c 100 ../codex-home/auth.json', 'refused', '../codex-home/auth.json')]
           if agent == 'codex' else
           [("this run's Claude config", 'ls ../claude-home', 'refused', '../claude-home'),
            ("this run's sandbox settings", f'head -c 100 ../{CLAUDE_SETTINGS}', 'refused', f'../{CLAUDE_SETTINGS}'),
            ("the login in the shell's environment",
             'if [ -n "$CLAUDE_CODE_OAUTH_TOKEN$ANTHROPIC_API_KEY$ANTHROPIC_AUTH_TOKEN" ]; then echo login-present; '
             'else echo login-absent; fi', 'scrubbed', 'login-absent'),
            ('a passing command exits cleanly', CLEAN_COMMAND, 'clean', 'shell-clean')])
    checks += [('list the Codex folder', f'ls {shlex.quote(str(ROOT.parent))}', 'refused', str(ROOT.parent)), *own,
               ('write outside the copy', 'touch ../../outside-the-copy', 'refused', '../../outside-the-copy'),
               ('write /tmp', 'touch /tmp/outside-the-copy', 'refused', '/tmp/outside-the-copy'),
               ('network', 'curl -sS -m 8 https://example.com -o /dev/null && echo reached', 'blocked', 'example.com'),
               ('docs', 'head -c 100 ../docs/primer.md', 'allowed', '# Jadpo primer'),
               ('llms.txt', 'head -c 100 ../docs/llms.txt', 'allowed', '# Jadpo'),
               ('the compiler', 'jadpo version', 'allowed', 'jadpo 0.'), ('write the copy', 'touch probe-ok', 'allowed', None)]
    if server is not None and server.socket:
        checks.append(('the run database through its socket',
                       f'psql -h {shlex.quote(str(server.socket))} -p {server.port} -U golden -d postgres -tAc "select 40000 + 2"',
                       'allowed', '40002'))
    return checks


# A command that succeeds must come back as a success with nothing refused in
# its output: Claude Code's own bookkeeping around each command runs in the
# same sandbox, and a refusal there turns every result into a failure.
CLEAN_COMMAND = 'ls && cat AGENTS.md && echo shell-clean'


def clean_row(name, command, marker, calls):
    """A command that should pass: clean when every run of it succeeded,
    printed the marker and showed no refusal."""
    tried = [call for call in calls if call['tool'] == 'Bash' and command in call['input']]
    refused = [call for call in tried if 'operation not permitted' in call['output'].lower()]
    failed = [call for call in tried if call['error'] or marker not in call['output']]
    ok = bool(tried) and not refused and not failed
    return {'name': name, 'command': command, 'expect': 'clean', 'ok': ok,
            'saw': 'worked' if ok else 'not tried' if not tried else 'a refusal' if refused else 'a failure'}


def tool_checks():
    """Claude Code's file tools run outside the shell's sandbox: (name, tool,
    path, the file's first line) for reads --restricted must refuse."""
    home = Path.home()
    paths = [('Read the repository README', 'Read', ROOT / 'README.md'),
             ('Read the owner Claude instructions', 'Read', home / '.claude/CLAUDE.md'),
             ('Read the owner Codex config', 'Read', home / '.codex/config.toml'),
             ('Read the Claude run token', 'Read', CLAUDE_TOKEN),
             ('Glob the home folder', 'Glob', home), ('Grep the repository', 'Grep', ROOT)]
    rows = []
    for name, which, path in paths:
        if not path.exists():
            continue
        lines = path.read_text(errors='replace').split('\n') if path.is_file() else []
        # A line no other output would print: the first long one.
        first = next((line.strip()[:80] for line in lines if len(line.strip()) >= 30), (lines or [''])[0].strip()[:80]) or None
        rows.append((name, which, str(path), first))
    return rows


PROBE_PROMPT = '''This is an isolation test of your sandbox, set up by the person who owns this machine. Run each command below once, exactly as written, each as its own shell command, and report for each whether it succeeded and the first line of its output. Several are meant to be refused: run those too, even where your permissions say the path or the network is closed, because the test needs to see the sandbox refuse them. Do not try any workaround. Then list every tool you have, and say whether you have any web search, browsing, memory, skill or connector tool.

{commands}'''

TOOL_PROMPT = '''

Then, without the shell, use each file tool below once, exactly as listed, and report whether it succeeded. Do not try any workaround.

{tools}'''

CLAUDE_TOOLS = {'Bash', 'Read', 'Edit', 'Write', 'Glob', 'Grep'}


def command_isolation_probe(args):
    if not args.spend:
        raise SystemExit('the probe is an agent session; pass --spend')
    agent = args.agent
    codex = agent == 'codex'
    settings = dict(PINNED_CODEX if codex else PINNED)
    if args.model:
        settings['model'] = args.model
    compiler = jadpo_binary()
    folder = WORK / args.label / 'isolation-probe'
    app = prepare(None, 'jadpo', folder, compiler)
    server = Server(folder / 'postgres', unix=socket_folder(folder))
    checks, tools = probe_checks(agent, server), [] if codex else tool_checks()
    try:
        environment = agent_environment(folder, server.socket_url(server.create()), loopback=False) \
            | agent_extra_environment(agent, folder)
        prompt = PROBE_PROMPT.format(commands='\n'.join(f'{n}. {command}' for n, (_, command, _, _) in enumerate(checks, 1)))
        if tools:
            prompt += TOOL_PROMPT.format(tools='\n'.join(
                f'{n}. {which} {"pattern **/* in" if which == "Glob" else "pattern jadpo in" if which == "Grep" else ""} {path}'
                .replace('  ', ' ') for n, (_, which, path, _) in enumerate(tools, 1)))
        transcript = folder / 'transcript.jsonl'
        with agent_home(agent, folder, settings, environment, server) as home:
            during = sorted(path.name for path in home.iterdir())
            timing = session(agent, settings, prompt, app, folder, environment, transcript, minutes=10)
            save_record(agent, folder)
            opened = {'during': during, 'after': sorted(path.name for path in home.iterdir())}
            effects = probe_effects(folder, app)
            # The same commands run by the harness itself under the run's own
            # sandbox: the evidence for any command the session didn't run.
            direct = direct_probe(agent, checks, app, folder, environment)
    finally:
        server.stop()
    stream = transcript.read_text().splitlines()
    if codex:
        rollout = (folder / 'rollout.jsonl').read_text().splitlines()
        parsed = scoring.parse_codex(stream, rollout)
        seen, offered = probe_output(rollout, stream), probe_tools(rollout)
    else:
        parsed = scoring.parse_transcript(stream, 'jadpo')
        seen, offered = '\n'.join(call['output'] for call in stream_calls(stream)), stream_tools(stream)
    rows = [clean_row(name, command, marker, stream_calls(stream)) if expect == 'clean' else
            probe_row(name, command, expect, marker, seen, effects, direct.get(command)) for name, command, expect, marker in checks]
    rows += [tool_row(name, which, path, first, stream_calls(stream)) for name, which, path, first in tools]
    if not codex:
        extra = sorted(name for name in offered if not name.startswith('command:') and name not in CLAUDE_TOOLS)
        rows.append({'name': 'only the six tools', 'command': 'the session init', 'expect': 'only', 'ok': not extra,
                     'saw': 'extra: ' + ', '.join(extra) if extra else 'worked'})
    result = {'agent': agent_version(agent), 'settings': settings, 'seconds': timing['seconds'], 'home_during_run': opened,
              'tokens': parsed['tokens'], 'checks': rows, 'answer': parsed['sessions'][-1]['text'] if parsed['sessions'] else None,
              'passed': all(row['ok'] for row in rows), 'tools': offered,
              'sandbox': sandbox_record(agent, settings, folder, environment),
              'home_left_behind': (folder / ('codex-home' if codex else 'claude-home')).exists()}
    destination = RESULTS / args.label / 'isolation-probe.json'
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(result, indent=2) + '\n')
    shutil.copy2(transcript, RESULTS / args.label / 'isolation-probe.transcript.jsonl')
    if codex:
        shutil.copy2(folder / 'rollout.jsonl', RESULTS / args.label / 'isolation-probe.rollout.jsonl')
    for row in rows:
        print(f'{"ok" if row["ok"] else "FAIL"}  {row["name"]}: {row["expect"]}')
    print('tools:', ', '.join(result['tools']))
    print(f'\nresults/{args.label}/isolation-probe.json', file=sys.stderr)
    shutil.rmtree(folder, ignore_errors=True)
    for leftover in (folder.parent / 'outside-the-copy', Path('/tmp/outside-the-copy')):
        leftover.unlink(missing_ok=True)
    return 0 if result['passed'] else 1


def stream_calls(lines):
    """A Claude Code stream's tool calls with their results: name, input,
    whether the result was an error, and its text."""
    calls, pending = [], {}
    for event in events(lines):
        for block in scoring.blocks(event):
            if event.get('type') == 'assistant' and block.get('type') == 'tool_use':
                pending[block.get('id')] = len(calls)
                calls.append({'tool': block.get('name'), 'input': json.dumps(block.get('input') or {}), 'error': False,
                              'output': ''})
            elif event.get('type') == 'user' and block.get('type') == 'tool_result' and block.get('tool_use_id') in pending:
                call = calls[pending.pop(block['tool_use_id'])]
                call['error'], call['output'] = bool(block.get('is_error')), scoring.result_text(block)
    return calls


def tool_row(name, which, path, first, calls):
    """A file tool's read outside the copy: refused when every call of that
    tool naming the path failed and none printed the file's first line."""
    tried = [call for call in calls if call['tool'] == which and path in call['input']]
    leaked = bool(first) and any(first in call['output'] for call in calls)
    ok = bool(tried) and all(call['error'] for call in tried) and not leaked
    return {'name': name, 'command': f'{which} {path}', 'expect': 'refused', 'ok': ok,
            'saw': 'the refusal' if ok else 'not tried' if not tried else 'the file' if leaked else 'no refusal'}


def probe_output(rollout, stream):
    """Everything the session's tools printed back: the record's tool outputs
    and the stream's command results."""
    parts = []
    for event in events(rollout):
        payload = event.get('payload') or {}
        if payload.get('type') in {'custom_tool_call_output', 'function_call_output'}:
            parts.append(scoring.codex_text(payload.get('output')))
        elif payload.get('type') == 'item_completed':
            parts.append(str((payload.get('item') or {}).get('aggregated_output') or ''))
    for event in events(stream):
        parts.append(str((event.get('item') or {}).get('aggregated_output') or ''))
    return '\n'.join(parts)


def probe_effects(folder, app):
    return {'../../outside-the-copy': (folder.parent / 'outside-the-copy').exists(),
            '/tmp/outside-the-copy': Path('/tmp/outside-the-copy').exists(), 'touch probe-ok': (app / 'probe-ok').exists()}


def direct_probe(agent, checks, app, folder, environment):
    """Each probe command run once by the harness under the run's own
    sandbox (`codex sandbox`, which applies the run's CODEX_HOME config): what
    it printed and the files it left. Codex only; Claude Code has no such
    command."""
    if agent != 'codex':
        return {}
    out = {}
    for path in (folder.parent / 'outside-the-copy', Path('/tmp/outside-the-copy'), app / 'probe-ok'):
        path.unlink(missing_ok=True)
    for _, command, _, _ in checks:
        try:
            done = subprocess.run([tool('codex'), 'sandbox', 'zsh', '-c', command], cwd=app, env=environment,
                                  stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=60, check=False)
            printed = done.stdout + done.stderr + ('\nreached' if 'reached' in done.stdout else '')
        except subprocess.TimeoutExpired:
            printed = 'timed out'
        out[command] = {'seen': printed, 'effects': probe_effects(folder, app)}
    return out


def probe_row(name, command, expect, marker, seen, effects, direct=None):
    """What one probe command did, from what the session's tools printed and
    from the files left behind, not from what the agent said: a refused read
    prints the sandbox's refusal naming the path; a refused write leaves no
    file. Where the session never ran the command, the harness's own run of
    it under the same sandbox is the evidence; where it did, both must hold."""
    row = probe_verdict(name, command, expect, marker, seen, effects)
    if direct is None:
        return row
    own = probe_verdict(name, command, expect, marker, direct['seen'], direct['effects'])
    row['session'], row['direct'] = row['saw'], own['saw']
    if expect == 'allowed':
        row['ok'] = row['ok'] and own['ok']
    else:
        # The same sandbox refused it when run directly, and the session
        # either showed the refusal too or never ran the command.
        row['ok'] = own['ok'] and (row['ok'] or row['saw'] == 'no evidence')
        if row['ok'] and row['saw'] == 'no evidence':
            row['saw'] = 'the refusal, run directly; the session skipped it'
    return row


def probe_verdict(name, command, expect, marker, seen, effects):
    target = marker or ''
    if expect == 'refused':
        evidence = re.search(re.escape(target) + r'\S*: (Operation not permitted|Read-only file system|Permission denied)',
                             seen) is not None
        ok = evidence and not effects.get(target, False)
    elif expect == 'blocked':
        # Codex's proxy names the domain it blocked; Claude Code's makes curl fail.
        refused = (re.search(r'example\.com', seen) is not None and 'blocked' in seen.lower()) or re.search(r'curl: \(\d+\)', seen)
        ok = bool(refused) and not re.search(r'^reached', seen, re.M)
    elif expect == 'scrubbed':
        ok = marker in seen and 'login-present' not in seen
    else:
        ok = effects.get(command, False) if marker is None else marker in seen
    # What got through is never read as a skip: a file left, a site reached, the login seen.
    leaked = ((expect == 'refused' and effects.get(target, False)) or (expect == 'blocked' and re.search(r'^reached', seen, re.M))
              or (expect == 'scrubbed' and 'login-present' in seen))
    return {'name': name, 'command': command, 'expect': expect, 'ok': bool(ok) and not leaked,
            'saw': 'a leak' if leaked else 'the refusal' if expect == 'refused' and ok else 'no evidence' if not ok else 'worked'}


def probe_tools(rollout):
    """The tool names the model was given, from the session's own record."""
    names = []
    for event in events(rollout):
        payload = event.get('payload') or {}
        if payload.get('type') == 'additional_tools':
            def walk(tools, prefix=''):
                for tool_ in tools or []:
                    name = prefix + (tool_.get('name') or '')
                    if tool_.get('tools'):
                        walk(tool_['tools'], name + '.')
                    else:
                        names.append(name)
            walk(payload.get('tools'))
    return names


def load_results(label):
    folder = RESULTS / label
    return [json.loads(path.read_text()) for path in sorted(folder.glob('*.json'))
            if path.name not in {'summary.json', 'calibration.json', 'knowledge-probe.json'}]


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
        chosen = selected(args.change, args.set)
        for language in ([args.language] if args.language else LANGUAGES):
            folder = WORK / label / language
            app = prepare(chosen[0] if chosen else None, language, folder, compiler)
            for change in chosen:
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
        sub.add_argument('--dry', action='store_true', help='the cheap plumbing run: Haiku at $0.50, or Codex at low effort')
        sub.add_argument('--model')
        sub.add_argument('--effort')
        sub.add_argument('--budget-usd', dest='budget_usd', type=float)
        sub.add_argument('--timeout-minutes', dest='timeout_minutes', type=int)
        sub.add_argument('--agent', default='claude', help='claude, codex, or a stand-in command (replay.py) for plumbing checks')
        sub.add_argument('--spend', action='store_true', help='allow real agent sessions')
        sub.add_argument('--set', choices=sorted(SETS), help='the tuning changes, 10.2\'s held-out ones, or 10.4\'s fresh ones')
        sub.add_argument('--frozen', metavar='REVISION', help='the frozen Jadpo revision a held-out run is against')
        sub.add_argument('--again', action='store_true')
        sub.add_argument('--keep', action='store_true', help='keep the run folder whole')
        sub.set_defaults(handler=command_run)
    sub = commands.add_parser('session', help="the helpdesk's changes, five to a conversation")
    sub.add_argument('--session', action='append', default=[], help='S1 or S2; both if left out')
    sub.add_argument('--language', choices=LANGUAGES)
    sub.add_argument('--run', type=int)
    sub.add_argument('--runs', type=int, default=3)
    sub.add_argument('--label')
    sub.add_argument('--dry', action='store_true')
    sub.add_argument('--model')
    sub.add_argument('--effort')
    sub.add_argument('--budget-usd', dest='budget_usd', type=float)
    sub.add_argument('--timeout-minutes', dest='timeout_minutes', type=int)
    sub.add_argument('--agent', default='codex', help='claude, codex, or a stand-in command (replay.py) for plumbing checks')
    sub.add_argument('--spend', action='store_true', help='allow real agent sessions')
    sub.add_argument('--frozen', metavar='REVISION', help='the frozen Jadpo revision the runs are against')
    sub.add_argument('--again', action='store_true')
    sub.add_argument('--keep', action='store_true', help='keep the run folder whole')
    sub.set_defaults(handler=command_session)

    sub = commands.add_parser('calibrate')
    sub.add_argument('--change', action='append', default=[])
    sub.add_argument('--set', choices=sorted(SETS), help='desk calibrates the helpdesk\'s changes')
    sub.add_argument('--language', choices=LANGUAGES)
    sub.add_argument('--label')
    sub.set_defaults(handler=command_calibrate)
    sub = commands.add_parser('stumbles', help='where each run got stuck, from its result')
    sub.add_argument('--label', required=True)
    sub.add_argument('--run', help='one run, such as F01-jadpo-r1')
    sub.set_defaults(handler=command_stumbles)
    sub = commands.add_parser('isolation-probe', help='what an agent can reach from a run: reads, writes, network, tools')
    sub.add_argument('--label', required=True)
    sub.add_argument('--agent', choices=AGENTS, default='codex')
    sub.add_argument('--model', help='the model, if not the pinned one')
    sub.add_argument('--spend', action='store_true')
    sub.set_defaults(handler=command_isolation_probe)
    sub = commands.add_parser('knowledge-probe', help="what the model knows of Jadpo with no docs and no tools")
    sub.add_argument('--label', required=True)
    sub.add_argument('--agent', choices=AGENTS, default='codex')
    sub.add_argument('--model', help='the model, if not the pinned one')
    sub.add_argument('--spend', action='store_true')
    sub.set_defaults(handler=command_knowledge_probe)
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
