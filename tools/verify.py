#!/usr/bin/env python3
"""The gate every change must pass.

As the new compiler is built (docs/roadmap.md), each phase adds its checks
here, so this script stays the single gate that CI and every session run
(docs/compiler-runtime.md §2.7).

  python3 tools/verify.py                  # everything
  python3 tools/verify.py --profile quick  # without live PostgreSQL, compiled code or the wasm32 steps
  python3 tools/verify.py --group lint     # one CI job's share (repeatable)
  python3 tools/verify.py --group compiled # the differential run alone
  python3 tools/verify.py --require-examples  # fail on a feature no example uses

It builds the workspace and its tests once, then runs every step that
needs nothing else concurrently: each test binary straight from that build,
cargo's own steps (clippy, wasm32) one after another
beside them, and the timing test last, alone. The steps that use
PostgreSQL share one server: the one JADPO_TEST_DATABASE_URL names, or a
disposable one started for the run (jadpo-test-server) and named to every
step through that variable. Each step prints `ok` or `FAILED` as it
finishes; failures' output follows, then a summary.
"""
from __future__ import annotations

import argparse
import functools
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WASM = 'wasm32-unknown-unknown'
# The CI jobs a step belongs to (.github/workflows/validation.yml).
GROUPS = ('lint', 'docs', 'tests', 'compiled', 'wasm32')
# Each test binary is a process of its own and runs its tests on threads;
# this many run at once.
JOBS = os.cpu_count() or 4
# The live PostgreSQL steps, twenty-odd processes of one binary, each run
# their tests on this many threads (RUST_TEST_THREADS) rather than one per
# core, so together they keep about two per core (chunk 5.13). Every other
# binary keeps libtest's default: `compiler tests` is the gate's longest
# step and mostly waits on the processes its tests start, and capping it
# too made the gate slower.
LIVE_THREADS = str(max(2, 2 * (os.cpu_count() or 4) // JOBS))
# Each crate's integration tests are one binary, each test file a module of
# it (crates/*/tests/main.rs).
INTEGRATION = 'integration'


@functools.cache
def target_dir() -> Path:
    """Cargo's build folder: `target/` unless CARGO_TARGET_DIR or cargo's
    configuration names another, as cargo itself reads them."""
    try:
        process = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked'],
                                 cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        return Path(json.loads(process.stdout)['target_directory'])
    except (OSError, ValueError, KeyError):
        return ROOT / 'target'


def wasm_crates() -> list[str]:
    """The crates that must build for WebAssembly: the shared and runtime
    layers of the crate graph (docs/compiler-runtime.md §2.1), read from the
    table the crate-graph test checks, that exist so far, less the runtime
    crates that only build natively ([workspace.metadata.targets])."""
    with open(ROOT / 'Cargo.toml', 'rb') as manifest:
        metadata = tomllib.load(manifest)['workspace']['metadata']
    layers = metadata['layers']
    native_only = set(metadata.get('targets', {}).get('native_only', []))
    return [name for name in layers['shared'] + layers['runtime']
            if name not in native_only
            and (ROOT / 'crates' / name / 'Cargo.toml').exists()]


@dataclass
class Step:
    """One check. It runs `command`, or, when `tests` is set, the test
    binaries that selection names from the workspace build: a list of
    (package, test name filter) pairs, run from the package's integration
    binary, or `everything` for every binary, with `args` passed to each."""
    name: str
    group: str
    command: list[str] | None = None
    tests: list[tuple[str, str]] | str | None = None
    args: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)
    # Steps in the same lane run one after another, beside the rest:
    # `cargo` for those that take cargo's lock on the build folder.
    lane: str | None = None
    # Uses the run's PostgreSQL server (JADPO_TEST_DATABASE_URL).
    postgres: bool = False
    # Runs `jadpo`, built from this checkout (JADPO_BIN).
    jadpo: bool = False
    # Measures time, so runs after everything else, by itself.
    alone: bool = False
    # Prints its output even when it passes (timings), or only the lines
    # that start with this text, or with one of these.
    show: bool | str | tuple[str, ...] = False
    # For `everything`: the (package, filter) pairs other steps run, which
    # it skips.
    owned: set[tuple[str, str]] = field(default_factory=set)
    # Filled in as it runs.
    ok: bool | None = None
    output: str = ''
    seconds: float = 0.0


@dataclass
class TestBinary:
    package: str
    target: str
    kind: str
    executable: str
    manifest_dir: str


# The steps that run test files with live PostgreSQL tests, each file (a
# module of its crate's integration binary) with the run's server: (name,
# [(package, module)], args). A file that calls `jadpo_test::live()` and
# isn't listed here gets a step of its own (live_test_files), so leaving
# one out can't make its live tests skip.
POSTGRES = [
    # The driver, the test database's template and copies; and TLS to
    # servers of its own on this machine, one given a certificate the
    # test makes (chunk 8.1n).
    ('live PostgreSQL', [('jadpo-test', 'postgres'), ('jadpo-test', 'postgres_tls')], ['--nocapture']),
    # Each test's outcome-and-effect record: the corpus run twice with the
    # same records, and a perturbed run named where it differs (chunk 8.1c).
    ('live PostgreSQL: outcome-and-effect records', [('jadpo-test', 'records')], []),
    # `jadpo migrate apply` and `status`; computed backfills, in batches,
    # resumed, and worked out in Jadpo where SQL can't (area 5); stored
    # structured values checked or rewritten when their type reads less
    # (chunk 5.9c).
    ('live PostgreSQL: jadpo migrate and backfills',
     [('jadpo-cli', 'migrate'), ('jadpo-cli', 'backfills'), ('jadpo-cli', 'json_migrations')], []),
    # Every standard type stored and read back, its column laid out as its
    # entry says, and each literal in SQL the value the runtime binds
    # (area 19, chunks 5.5w and 5.5w3b); an application's structured
    # values stored as JSON, read back unchanged and held to their rules
    # (area 5, chunk 5.9b).
    ('live PostgreSQL: standard types stored and SQL literals',
     [('jadpo-cli', 'standard_storage'), ('jadpo-cli', 'sql_literals'),
      ('jadpo-cli', 'json_columns')], []),
    # Passkeys registered and signed in with through the generated routes,
    # and each refusal (area 8, chunk 5.5z); magic links asked for,
    # confirmed and limited (chunk 7.3b); configuration through the
    # commands, a database opened from a setting among them (area 10).
    ('live PostgreSQL: passkeys, magic links and configuration',
     [('jadpo-cli', 'passkeys'), ('jadpo-cli', 'magic_links'), ('jadpo-cli', 'config')], []),
    # Queries, `get` and `create` run through `jadpo test`, and
    # aggregate queries against what code works out (area 6).
    ('live PostgreSQL: queries', [('jadpo-cli', 'queries'), ('jadpo-cli', 'aggregates')], []),
    # Writes, their conflicts and counters through `jadpo test`, and a
    # concurrent change forced from a second connection, rerunning the
    # whole action (area 6); and no transaction open while a provider
    # holds a call, a change made meanwhile rerunning it (chunk 7.4).
    ('live PostgreSQL: writes and concurrent changes',
     [('jadpo-cli', 'writes'), ('jadpo-cli', 'call_snapshots')], []),
    # Lifecycles through `jadpo test`: hidden records, transitions and
    # their guards; then purge through the runtime's entry point, with
    # cascades, restricted references and batches (area 5).
    ('live PostgreSQL: lifecycles and purge', [('jadpo-cli', 'lifecycle')], []),
    # Every standard operation's SQL form gives what the runtime gives
    # on the same inputs (area 19, "SQL forms"; chunk 5.6).
    ('live PostgreSQL: SQL forms', [('jadpo-cli', 'sql_forms')], ['--nocapture']),
    # Routes over real HTTP: `jadpo dev` on the native host, requests
    # over TCP, the failure envelope, query and header input, rate
    # limits and reloading; and the golden OpenAPI document (area 7).
    # Rules across fields in a query string, a body named twice, and a
    # stored standard object breaking its rules (chunk 5.8c).
    ('live PostgreSQL: routes over HTTP', [('jadpo-cli', 'http'), ('jadpo-cli', 'rule_slice')], []),
    # Policy through `jadpo test`: tenancy, roles, owner fields, listed
    # actions and credentials; concealment through the runtime (area 9);
    # the system caller the runner gives background work (chunk 6.1a).
    ('live PostgreSQL: policy', [('jadpo-cli', 'policy'), ('jadpo-cli', 'system')], []),
    # Batch runs: pages committed with their position, problems, a crash
    # resumed, `key:`, a cancel and a deploy that changed the call (area
    # 12, chunk 6.4); and items whose action makes outside calls, each a
    # batch of its own, keyed calls repeated after a crash (chunk 7.4b).
    ('live PostgreSQL: batches', [('jadpo-cli', 'batches'), ('jadpo-cli', 'batch_calls')], []),
    # The runtime's request path counted in round trips (chunk 4.10),
    # and signed sessions: no read to sign in while a signed part
    # lasts, the delay's bound on revocation, key rotation, the key
    # exchange and the configured origin (area 8, chunk 5.1b).
    ('live PostgreSQL: round trips and signed sessions',
     [('jadpo-cli', 'round_trips'), ('jadpo-cli', 'signed')], []),
    # Vectors in pgvector: stored and read back unchanged, `ai.distance`
    # against pgvector's own operators, and a store without the
    # extension refused (area 21, chunk 5.5v1). The server needs
    # pgvector: CI's image has it (tools/postgres/Dockerfile).
    ('live PostgreSQL: vectors', [('jadpo-cli', 'vectors')], []),
    # Geometries in PostGIS: every geometry stored and read back
    # unchanged, each column's check, and a store without PostGIS, or a
    # role that can't create it, refused (area 20, chunk 5.5u4); each
    # `geo.` condition and the order by distance giving the code's
    # answer through its index (chunk 5.5u5); a query's map layer
    # fetched from a `GET` route as GeoJSON (chunk 5.5u8). The server
    # needs PostGIS: CI's image has it (tools/postgres/Dockerfile).
    ('live PostgreSQL: geography',
     [('jadpo-cli', 'geography'), ('jadpo-cli', 'geo_queries'), ('jadpo-cli', 'geo_layers')], []),
]

# Phase 8 (docs/compiler-runtime.md §2.5 and §2.7): the differential run.
# Every project of the corpus that checks, the pages' and the teaching
# examples', compiled as modules of one generated crate beside one
# harness binary, built with cargo, offline, in target/jadpo-compiled/
# (kept, so a later run compiles only what changed), and its tests run
# interpreted and compiled with the same records, against the run's
# server. The `compiled` group, a CI job of its own (chunk 8.1j); its
# summary lines are shown. Beside it, a native deployable (chunk 8.1o):
# teaching example 23 built by `jadpo build` in release, its build kept in
# target/jadpo-compiled/deployable/ too, then started against the run's
# server, refusing to start out of date, serving and draining on SIGTERM.
# Route tests on workerd (chunk 8.2e) share its test file but are a step
# of the `wasm32` group, whose job has the time: ROUTES_ON_WORKERD.
COMPILED = ('compiled code: the corpus interpreted and compiled with the same records, and a deployable',
            [('jadpo-cli', 'compiled'), ('jadpo-cli', 'deployable')],
            ['--skip', 'compiled::route_tests_on_workerd_give_the_interpreters_records'])
# Route tests on workerd (chunk 8.2e): area 7's todos and teaching example
# 23 built for wasm32 as a test Worker, in target/jadpo-compiled/workerd/,
# and each route test run inside it with the interpreter's records, workerd
# installed into tools/workers/ the first time.
ROUTES_ON_WORKERD = ('route tests on workerd with the interpreter\'s records',
                     [('jadpo-cli', 'compiled::route_tests_on_workerd_give_the_interpreters_records')])
COMPILED_SHOWN = ('compared ', 'teaching examples:', 'skipped:')
# The release profile the deployables a test builds with `jadpo build` are
# compiled in (chunk 8.4f): cargo's own codegen units and link-time
# optimisation, through cargo's variables, which override the profile
# `jadpo build` writes. That profile's fat link-time optimisation in one
# codegen unit takes minutes for each deployable, and several are built;
# what the tests prove doesn't depend on it, and the profile itself is
# checked as `jadpo build` writes it (jadpo-codegen's tests).
TEST_RELEASE_PROFILE = {'CARGO_PROFILE_RELEASE_LTO': 'false', 'CARGO_PROFILE_RELEASE_CODEGEN_UNITS': '16'}

# The Workers host under workerd, Cloudflare's own Workers runtime, on this
# machine (docs/compiler-runtime.md §2.6, "The Workers host"; chunk 8.2a):
# the probe Worker built for wasm32 by the test itself (so the step takes
# cargo's lane), bound by wasm-bindgen's library, and run by the workerd
# pinned in tools/workers/, which the test installs there with `npm ci` the
# first time, against servers it starts; since chunk 8.2j its six
# connections an invocation and `max_concurrent:`'s slots across an
# isolate's invocations too (tests/connections.rs). The `wasm32` group;
# `compiler tests` leaves it out in both profiles.
WORKERD = ('the Workers host under workerd', [('jadpo-host-workers', 'workerd'),
                                              ('jadpo-host-workers', 'connections')])
# A Workers deployable (chunks 8.2b and 8.2d): teaching example 23 declared
# as a kiosk built for `Target.wasm` by `jadpo build` in release for wasm32,
# its build kept in target/jadpo-compiled/workers/, bound by wasm-bindgen's
# library, written with Jadpo's module shim and a Wrangler file, and served
# by the same workerd against PostgreSQL over `connect()`: the run's server
# when it is named by a URL (CI's, on 127.0.0.1), else one the test starts
# on 127.0.0.1, since a local run's listens on a Unix socket, which a
# Worker can't open. Since chunk 8.2h a news desk of its own beside it,
# whose model is a server the test starts: a streamed answer, and a
# subscriber's deliveries and a batch run each longer than an invocation's
# 30 seconds, handed on to a fresh one; since chunk 8.2k a warehouse
# consuming Cloudflare Queues (workers/queues.rs), its Wrangler file checked
# by the pinned Wrangler's `deploy --dry-run` with the network refused. The
# `wasm32` group, in cargo's lane beside the step above, since both install
# workerd into tools/workers/ the first time; it prints the module's size.
WORKERS_DEPLOYABLE = ('a Workers deployable built and served by workerd', [('jadpo-cli', 'workers')])
WORKERS_SHOWN = ('skipped:', 'measured:')


# A test file with a live part calls the one helper that says whether it
# runs (crates/jadpo-test/src/live.rs).
LIVE_CALL = re.compile(r'(?<![\w])live\(\)')
# A module of an integration binary's main.rs, with the file `#[path]` names.
TEST_MODULE = re.compile(r'^[ \t]*(?:#\[path\s*=\s*"([^"]+)"\]\s*)?mod\s+(\w+)\s*;', re.MULTILINE)


def live_test_files(root: Path = ROOT) -> list[tuple[str, str]]:
    """(package, module) of every test file whose tests call
    `jadpo_test::live()`: each crate's tests are modules of one integration
    binary (crates/*/tests/main.rs)."""
    found = []
    for main in sorted(root.glob('crates/*/tests/main.rs')):
        for path, module in TEST_MODULE.findall(main.read_text()):
            file = main.parent / (path or f'{module}.rs')
            if not file.exists():
                file = main.parent / module / 'mod.rs'
            if file.exists() and LIVE_CALL.search(file.read_text()):
                found.append((main.parent.parent.name, module))
    return found


def postgres_steps(table: list | None = None, root: Path = ROOT) -> list[tuple[str, list[tuple[str, str]], list[str]]]:
    """The table's steps, then one for each live test file it and the
    differential run (COMPILED) leave out."""
    table = POSTGRES if table is None else table
    listed = {test for _, tests, _ in [*table, COMPILED] for test in tests} | set(WORKERS_DEPLOYABLE[1])
    return list(table) + [(f'live PostgreSQL: {module} (not listed in POSTGRES)', [(package, module)], [])
                          for package, module in live_test_files(root) if (package, module) not in listed]


def steps(profile: str, require_examples: bool = False, table: list | None = None) -> list[Step]:
    python = sys.executable
    full = profile == 'full'
    postgres = postgres_steps(table)
    # Under --profile quick nothing starts PostgreSQL: the pages' and the
    # examples' projects with migrations are checked and formatted, and
    # their `jadpo test` runs are reported as skipped.
    no_postgres = {} if full else {'JADPO_SKIP_POSTGRES': '1'}
    checks = [
        # Runs jadpo over the `check` blocks; projects that declare tests
        # also run under `jadpo test` (Phase 2) and are exported to
        # build/spec-corpus/ (docs/compiler-runtime.md §2.7).
        Step('language specification and jadpo test over the spec corpus', 'docs',
             [python, 'tools/language_docs.py', '--check', '--export-corpus'], jadpo=True,
             postgres=full, env=no_postgres, show='skipped:'),
        # Every teaching example in examples/ checks with no diagnostics,
        # is formatted, passes its own tests under `jadpo test`, has its
        # README, and is exported beside the pages' projects
        # (docs/compiler-runtime.md §2.7).
        Step('teaching examples: check, format and jadpo test', 'docs',
             [python, 'tools/examples.py', '--export-corpus'], jadpo=True,
             postgres=full, env=no_postgres, show='skipped:'),
        Step('focused TypeScript baseline types and Unicode contracts', 'lint', [python, 'tools/bench/check_typescript.py']),
        Step('benchmark verifier contracts', 'lint', [python, '-m', 'unittest', 'discover', '-s', 'tools/bench', '-p', 'test_contracts.py']),
        Step('benchmark load correctness', 'lint', ['cargo', 'test', '--locked', '--manifest-path', 'tools/bench/load/Cargo.toml', '--bin', 'load', '--test', 'protocol'], lane='cargo'),
        Step('benchmark load formatting', 'lint', ['rustfmt', '--edition', '2024', '--check', 'tools/bench/load/src/bin/load.rs', 'tools/bench/load/tests/protocol.rs'], lane='cargo'),
        Step('focused benchmark fixture', 'docs', [python, 'tools/bench/check_fixture.py'], jadpo=True),
        Step('example names', 'lint', [python, 'tools/example_names.py', '--check']),
        Step('golden contract verifier controls', 'lint', [python, 'tools/golden-todo/test_contract.py']),
        Step('golden shared database connection controls', 'lint', [python, 'tools/golden-todo/test_postgres.py']),
        # The docs renderer finishes every page, and its list parser's
        # cases, each under a time limit (tools/language_docs_test.py).
        Step('docs renderer', 'lint', [python, 'tools/language_docs_test.py']),
        # The maths on `Decimal`'s reference values are what their
        # generator writes (area 19, chunk 5.5e).
        Step('Decimal maths reference values in step with their generator', 'lint',
             [python, 'tools/decimal_maths_reference.py', '--check']),
        # The VS Code extension's highlighting names every word the lexer
        # reserves and no other (editors/vscode; needs no Node).
        Step('editor grammar in step with the lexer', 'lint',
             [python, 'tools/editor_grammar.py', '--check']),
    ]
    latency_test = 'a_save_shows_the_tests_it_reaches_within_the_target'
    own_steps = [
        # Every example in docs/language and examples/ formats idempotently
        # to the same tree (docs/compiler-runtime.md §2.7).
        ('format idempotence', [('jadpo-fmt', 'examples')]),
        # Same source, same bytes (docs/compiler-runtime.md §2.8): every
        # corpus project's manifest twice, its files read in two orders,
        # and no key or digest moved by formatting.
        ('manifest determinism', [('jadpo-check', 'determinism')]),
        # The examples use every feature the compiler builds, read from its
        # own tables; the exemptions are listed with their reasons in the
        # test (crates/jadpo-check/tests/examples.rs). A feature no example
        # uses yet is a note unless --require-examples (the phase's
        # examples chunk) makes it a failure.
        ('teaching examples cover every built feature', [('jadpo-check', 'examples')]),
        # Every construction the checker proved, across the spec corpus,
        # checked again as it runs (docs/compiler-runtime.md §2.7).
        ('proofs hold when they run', [('jadpo-interp', 'proofs')]),
    ]
    checks += [
        Step('rust formatting', 'lint', ['cargo', 'fmt', '--all', '--check']),
        Step('clippy', 'lint',
             ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'],
             lane='cargo'),
        *[Step(name, 'tests', tests=modules(tests)) for name, tests in own_steps],
        # Every other test in the workspace: the crate-graph test
        # (crates/jadpo-cli/tests/crate_graph.rs), the parser's
        # documentation, error and fuzz tests, the generated AST's drift
        # test, and the rest. The steps above and the live PostgreSQL steps
        # run their own test files, which it skips; under --profile quick
        # the PostgreSQL files run here, their live tests skipped. In the
        # full profile every live file has a step of its own, so a live test
        # that reaches this one fails (JADPO_REQUIRE_LIVE). The watch tests
        # run here but for the timing test, which runs last, alone.
        Step('compiler tests', 'tests', tests='everything',
             args=['--skip', f'watch::{latency_test}'],
             env={'JADPO_SKIP_POSTGRES': '1', **({'JADPO_REQUIRE_LIVE': '1'} if full else {})}),
        # Phase 2 (docs/compiler-runtime.md §2.7): a save in a generated
        # 230-file project shows the tests it reaches, through the real
        # watch loop, within the 1-second target plus a CI allowance; the
        # times are printed.
        Step('watch-mode latency', 'tests', tests=[('jadpo-cli', f'watch::{latency_test}')],
             args=['--exact', '--nocapture'], alone=True, show=True),
    ]
    # Phase 3 (docs/compiler-runtime.md §2.7): the PostgreSQL driver against
    # the run's server, all at once: each process names its databases with
    # a tag of its own, and templates are built under a name of their own
    # and renamed.
    if full:
        # Each prints its `skipped:` lines (a server too old for a test, on
        # a developer's machine), which libtest otherwise keeps to itself.
        checks += [Step(name, 'tests', tests=modules(tests),
                        args=args if '--nocapture' in args else [*args, '--show-output'],
                        show=True if '--nocapture' in args else 'skipped:', postgres=True)
                   for name, tests, args in postgres]
        name, tests, args = COMPILED
        checks.append(Step(name, 'compiled', tests=modules(tests), args=[*args, '--show-output'],
                           show=COMPILED_SHOWN, postgres=True, env=dict(TEST_RELEASE_PROFILE)))
        checks.append(Step('golden todo: all 44 native cases', 'compiled',
                           [python, 'tools/golden-todo/run.py', '--targets', 'native'], jadpo=True, postgres=True, lane='cargo',
                           env=dict(TEST_RELEASE_PROFILE), show=True))
        # rust-toolchain.toml lists the target, so rustup installs it with
        # the pinned toolchain. The drivers build with every driver they
        # have, so tokio-postgres is proven to build for wasm32 too.
        crates = wasm_crates()
        packages = [arg for name in crates for arg in ('-p', name)]
        features = (['--features', 'jadpo-drivers/postgres,jadpo-drivers/package,jadpo-drivers/smtp']
                    if 'jadpo-drivers' in crates else [])
        # A deployable holds no testing machinery (chunk 8.1e): the native
        # host's deployable example built without the runtime's `testing`
        # feature, its symbols naming none of the fakes, recordings or crash
        # points.
        checks.append(Step('a deployable built without testing', 'tests',
                           [python, 'tools/deployable.py'], lane='cargo'))
        checks.append(Step('golden todo: all 44 workerd cases', 'wasm32',
                           [python, 'tools/golden-todo/run.py', '--targets', 'workerd'],
                           jadpo=True, postgres=True, lane='cargo', env=dict(TEST_RELEASE_PROFILE), show=True))
        checks.append(Step('shared and runtime crates for wasm32', 'wasm32',
                           ['cargo', 'build', '--locked', '--target', WASM, *packages, *features],
                           lane='cargo'))
        # The maths on `Decimal` runs under WebAssembly, not only builds
        # (area 19, chunk 5.5e): its reference check, built for wasm32 and
        # run under Node, gives the digits the native test does.
        checks += [
            Step('Decimal maths check for wasm32', 'wasm32',
                 ['cargo', 'build', '--locked', '--target', WASM, '-p', 'jadpo-value',
                  '--example', 'decimal_maths'], lane='cargo'),
            Step('Decimal maths gives the reference digits under wasm32', 'wasm32',
                 ['node', 'tools/run_wasm.mjs',
                  str(target_dir() / WASM / 'debug/examples/decimal_maths.wasm')], lane='cargo'),
            # SHA-256, the encodings and `json.write`'s text (area 19,
            # chunk 5.5g): the same fixed answers the native test checks,
            # so `json.write` gives byte-identical text on both targets.
            Step('hashing, encodings and JSON check for wasm32', 'wasm32',
                 ['cargo', 'build', '--locked', '--target', WASM, '-p', 'jadpo-value',
                  '--example', 'formats'], lane='cargo'),
            Step('hashing, encodings and JSON give the fixed answers under wasm32', 'wasm32',
                 ['node', 'tools/run_wasm.mjs',
                  str(target_dir() / WASM / 'debug/examples/formats.wasm')], lane='cargo'),
            # The geodesic measures (area 20, chunk 5.5u2): GeographicLib's
            # test sample and the digest of every answer, as natively; and
            # the predicates and shapes against PostGIS's answers (chunk
            # 5.5u3), with the digest of theirs.
            Step('geodesic measures check for wasm32', 'wasm32',
                 ['cargo', 'build', '--locked', '--target', WASM, '-p', 'jadpo-value',
                  '--example', 'geodesics'], lane='cargo'),
            Step('geodesic measures give the native digits under wasm32', 'wasm32',
                 ['node', 'tools/run_wasm.mjs',
                  str(target_dir() / WASM / 'debug/examples/geodesics.wasm')], lane='cargo'),
            Step(WORKERD[0], 'wasm32', tests=modules(WORKERD[1]), lane='cargo'),
            Step(WORKERS_DEPLOYABLE[0], 'wasm32', tests=modules(WORKERS_DEPLOYABLE[1]), lane='cargo',
                 args=['--show-output'], show=WORKERS_SHOWN, postgres=True,
                 env=dict(TEST_RELEASE_PROFILE)),
            Step(ROUTES_ON_WORKERD[0], 'wasm32', tests=ROUTES_ON_WORKERD[1], lane='cargo',
                 args=['--exact', '--show-output'], show=COMPILED_SHOWN, postgres=True),
        ]
    coverage = next(step for step in checks if step.name == 'teaching examples cover every built feature')
    coverage.args, coverage.show = ['--nocapture'], 'note:'
    if require_examples:
        coverage.env['JADPO_REQUIRE_EXAMPLES'] = '1'
    # Test files another step runs are left out of `compiler tests`.
    owned = {test for _, tests in [*own_steps, WORKERD, WORKERS_DEPLOYABLE] for test in modules(tests)}
    if full:
        owned |= {test for _, tests, _ in [*postgres, COMPILED] for test in modules(tests)}
    for step in checks:
        if step.tests == 'everything':
            step.owned = owned
    return checks


def modules(tests: list[tuple[str, str]]) -> list[tuple[str, str]]:
    """(package, module) pairs as (package, filter): every test in that
    test file, whose names start `<module>::`."""
    return [(package, f'{module}::') for package, module in tests]


def run(command: list[str], env: dict[str, str] | None = None, cwd: Path | str = ROOT) -> tuple[bool, str]:
    result = subprocess.run(command, cwd=cwd, env={**os.environ, **(env or {})},
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            stdin=subprocess.DEVNULL, text=True, errors='replace')
    return result.returncode == 0, result.stdout


# The binary that starts the run's PostgreSQL server (crates/jadpo-test).
SERVER_BIN = 'jadpo-test-server'


def build_tests(packages: list[str] | None = None) -> tuple[bool, str, list[TestBinary], dict[str, str]]:
    """`cargo test --no-run` over the workspace, or over `packages` alone:
    every test binary, and `jadpo` and jadpo-test-server beside them, built
    once with one set of features."""
    chosen = ['--workspace'] if packages is None else [arg for package in packages for arg in ('-p', package)]
    return built(['cargo', 'test', '--locked', *chosen, '--no-run'])


def packages_for(chosen: list[Step], server: bool) -> list[str] | None:
    """The packages whose tests and tools `chosen` runs, when that is fewer
    than the workspace (a run of the `compiled` group alone, as its CI job
    is): those its test steps name, jadpo-cli for `jadpo` and jadpo-test for
    the server a run starts (`server`). None when a step runs every test."""
    if any(step.tests == 'everything' for step in chosen):
        return None
    packages = {package for step in chosen if isinstance(step.tests, list) for package, _ in step.tests}
    if any(step.jadpo for step in chosen):
        packages.add('jadpo-cli')
    if server and any(step.postgres for step in chosen):
        packages.add('jadpo-test')
    return sorted(packages)


def built(command: list[str]) -> tuple[bool, str, list[TestBinary], dict[str, str]]:
    """Runs a cargo build: whether it passed, its diagnostics, and the test
    binaries and tools it built, where cargo says it put them (so
    CARGO_TARGET_DIR is honoured)."""
    process = subprocess.run(
        [*command, '--message-format=json-render-diagnostics'],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, errors='replace')
    binaries, tools = [], {}
    for line in process.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get('reason') != 'compiler-artifact' or not message.get('executable'):
            continue
        target = message['target']
        manifest_dir = str(Path(message['manifest_path']).parent)
        package = Path(manifest_dir).name
        if message['profile']['test']:
            binaries.append(TestBinary(package, target['name'], target['kind'][0],
                                       message['executable'], manifest_dir))
        elif target['kind'] == ['bin'] and target['name'] in ('jadpo', SERVER_BIN):
            tools[target['name']] = message['executable']
    return process.returncode == 0, process.stderr, binaries, tools


def build_tools() -> tuple[bool, str, dict[str, str]]:
    """Only `jadpo` and jadpo-test-server, for a run that needs no test
    binaries (CI's docs job)."""
    ok, output, _, tools = built(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '-p', 'jadpo-test', '--bins'])
    return ok, output, tools


class PostgresServer:
    """The run's PostgreSQL server: jadpo-test-server, started on a thread
    of its own beside the steps that don't need it, its admin URL read,
    then stopped, with its folder removed, when the run ends."""

    def __init__(self, executable: str, report: 'Report') -> None:
        self.step = Step('start a PostgreSQL server for the run', 'tests')
        self.process: subprocess.Popen | None = None
        self.url = ''
        self.thread = threading.Thread(target=self.start, args=(executable, report))
        self.thread.start()

    def start(self, executable: str, report: 'Report') -> None:
        begun = time.monotonic()
        self.process = subprocess.Popen([executable], cwd=ROOT, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.url = self.process.stdout.readline().strip()
        self.step.ok = bool(self.url)
        self.step.output = '' if self.url else self.process.communicate()[1]
        self.step.seconds = time.monotonic() - begun
        report.finished(self.step)

    def ready(self) -> str:
        """The server's admin URL once it has started, or '' if it couldn't."""
        self.thread.join()
        return self.url

    def stop(self) -> None:
        self.thread.join()
        if self.process and self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=60)
            except subprocess.TimeoutExpired:
                self.process.kill()


def strays(binary: TestBinary, prefixes: list[str]) -> list[str]:
    """`--skip` arguments for the tests `prefixes` would match by substring
    but whose names don't start with one of them."""
    ok, out = run([binary.executable, '--list', '--format', 'terse'], cwd=binary.manifest_dir)
    if not ok:
        return []
    names = [line.rsplit(': ', 1)[0] for line in out.splitlines() if line.endswith(': test')]
    stray = sorted({name for name in names
                    if any(prefix in name for prefix in prefixes)
                    and not any(name.startswith(prefix) for prefix in prefixes)})
    return [arg for name in stray for arg in ('--skip', name)]


def test_jobs(step: Step, binaries: list[TestBinary]):
    """The (label, command, cwd, env) of each binary `step` runs, as `cargo
    test` would run it: from its package's folder."""
    if step.tests == 'everything':
        chosen = [(b, [arg for package, filter in sorted(step.owned) if package == b.package
                       and b.kind == 'test' for arg in ('--skip', filter)]) for b in binaries]
    else:
        packages = sorted({package for package, _ in step.tests})
        integration = {b.package: b for b in binaries if b.kind == 'test' and b.target == INTEGRATION}
        missing = [package for package in packages if package not in integration]
        if missing:
            raise LookupError(f'no integration test binary for {missing}')
        # libtest matches a filter anywhere in a test's name, so `deliveries::`
        # would also run `step_deliveries::`; skip every listed test whose
        # module only ends with the one asked for.
        chosen = [(integration[package], [*prefixes, *strays(integration[package], prefixes)])
                  for package in packages
                  for prefixes in [[filter for p, filter in step.tests if p == package]]]
    for binary, filters in chosen:
        # How to run it again on its own.
        selector = {'lib': '--lib', 'bin': f'--bin {binary.target}'}.get(binary.kind, f'--test {binary.target}')
        label = ' '.join([f'cargo test -p {binary.package} {selector}',
                          *(filters if step.tests != 'everything' else [])])
        env = {**step.env, 'CARGO_MANIFEST_DIR': binary.manifest_dir, 'CARGO_PKG_NAME': binary.package,
               **({'RUST_TEST_THREADS': os.environ.get('RUST_TEST_THREADS', LIVE_THREADS)} if step.postgres else {})}
        yield label, [binary.executable, *filters, *step.args], binary.manifest_dir, env


NOTHING_RAN = re.compile(r'^running 0 tests$', re.MULTILINE)
# A test or project that didn't run says so on a line of its own.
SKIPPED = re.compile(r'^skipped:.*$', re.MULTILINE)


class Report:
    """Prints each step as it finishes, one line, under a lock."""

    def __init__(self) -> None:
        self.lock = threading.Lock()

    def finished(self, step: Step) -> None:
        with self.lock:
            word = 'ok' if step.ok else 'FAILED'
            print(f'{word:<6} {step.seconds:6.1f} s  {step.name}', flush=True)
            shown = step.output.strip('\n')
            if isinstance(step.show, (str, tuple)):
                shown = '\n'.join(line for line in shown.splitlines() if line.startswith(step.show))
            if step.show and step.ok and shown.strip():
                print(indent(shown), flush=True)


def indent(text: str) -> str:
    return '\n'.join(f'    {line}' for line in text.rstrip().splitlines())


def run_step(step: Step, binaries: list[TestBinary], pool: ThreadPoolExecutor | None,
             report: Report, jadpo: str | None, server: PostgresServer | None) -> None:
    if step.postgres and server:
        url = server.ready()
        if not url:
            step.ok, step.output = False, f'no PostgreSQL server: see "{server.step.name}"'
            report.finished(step)
            return
        step.env = {**step.env, 'JADPO_TEST_DATABASE_URL': url}
    start = time.monotonic()
    seen = ''
    if step.tests is None:
        env = {**step.env, **({'JADPO_BIN': jadpo} if step.jadpo and jadpo else {})}
        step.ok, step.output = run(step.command, env)
        seen = step.output
    else:
        try:
            jobs = list(test_jobs(step, binaries))
        except LookupError as error:
            jobs, step.ok, step.output = [], False, str(error)
        if jobs:
            if pool:
                futures = [(label, pool.submit(run, command, env, cwd)) for label, command, cwd, env in jobs]
                results = [(label, *future.result()) for label, future in futures]
            else:
                results = [(label, *run(command, env, cwd)) for label, command, cwd, env in jobs]
            if step.tests != 'everything':
                # A step whose filter matches nothing would pass on nothing.
                results = [(label, ok and not NOTHING_RAN.search(output),
                            output if not NOTHING_RAN.search(output) else f'no test matches: {label}\n{output}')
                           for label, ok, output in results]
            failed = [(label, output) for label, ok, output in results if not ok]
            step.ok = not failed
            seen = '\n'.join(output for _, _, output in results)
            if failed:
                step.output = '\n'.join(f'--- {label}\n{output}' for label, output in failed)
            elif step.show:
                step.output = '\n'.join(output for _, _, output in results)
    # Under CI every test runs: its server has every extension and version
    # the tests need, so a skip there is a failure.
    skips = SKIPPED.findall(seen)
    if os.environ.get('CI') and step.ok and skips:
        step.ok = False
        step.output = 'a skip under CI, where every test must run:\n' + '\n'.join(skips)
    step.seconds = time.monotonic() - start
    report.finished(step)


def self_test() -> list[str]:
    """What the gate promises about live PostgreSQL tests (chunk 5.8), checked
    before it runs: the problems found, if any."""
    problems = []
    with tempfile.TemporaryDirectory() as tmp:
        tests = Path(tmp) / 'crates/jadpo-x/tests'
        tests.mkdir(parents=True)
        (tests / 'main.rs').write_text('mod support;\nmod plain;\n#[path = "test.rs"]\nmod test_command;\nmod stored;\n')
        (tests / 'plain.rs').write_text('fn deliver() {}\n')
        (tests / 'test.rs').write_text('if !jadpo_test::live() {\n    return;\n}\n')
        (tests / 'stored.rs').write_text('use jadpo_test::live;\nif !live() { return; }\n')
        found = live_test_files(Path(tmp))
        if found != [('jadpo-x', 'test_command'), ('jadpo-x', 'stored')]:
            problems.append(f'live test files found in a sample tree: {found}')
        # A live file the table leaves out gets a server all the same.
        named = [name for name, _, _ in postgres_steps([], Path(tmp))]
        if len(named) != 2:
            problems.append(f'steps for a sample tree\'s live files with an empty table: {named}')
    live = modules(live_test_files())
    if not live:
        problems.append('no test file calls jadpo_test::live()')
    for table in (POSTGRES, []):
        chosen = steps('full', table=table)
        served = {test for step in chosen if step.postgres and isinstance(step.tests, list)
                  for test in step.tests}
        missing = [f'{package} {filter}' for package, filter in live if (package, filter) not in served]
        if missing:
            problems.append(f'live test files with no server in the full profile: {", ".join(missing)}')
        compiler = next(step for step in chosen if step.tests == 'everything')
        if compiler.env.get('JADPO_REQUIRE_LIVE') != '1':
            problems.append('the full profile\'s compiler tests let a live test skip')
        unshown = [step.name for step in chosen if step.postgres and isinstance(step.tests, list)
                   and not (step.show is True or 'skipped:' in (step.show if isinstance(step.show, tuple)
                                                                 else (step.show,)))]
        if unshown:
            problems.append(f'live steps that hide their skipped: lines: {", ".join(unshown)}')
    # A `skipped:` line fails a step under CI, and only there.
    class Quiet(Report):
        def finished(self, step: Step) -> None:
            pass
    ci = os.environ.pop('CI', None)
    try:
        for under_ci in (False, True):
            if under_ci:
                os.environ['CI'] = 'true'
            step = Step('a skip', 'tests', [sys.executable, '-c', 'print("skipped: a sample")'])
            run_step(step, [], None, Quiet(), None, None)
            if step.ok == under_ci:
                problems.append(f'a step that prints a skipped: line {"passed" if step.ok else "failed"}'
                                f' {"under" if under_ci else "outside"} CI')
    finally:
        os.environ.pop('CI', None)
        if ci is not None:
            os.environ['CI'] = ci
    quick = next(step for step in steps('quick') if step.tests == 'everything')
    if quick.env.get('JADPO_REQUIRE_LIVE') or quick.env.get('JADPO_SKIP_POSTGRES') != '1':
        problems.append('the quick profile\'s compiler tests don\'t skip live tests')
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description='The gate every change must pass.')
    parser.add_argument(
        '--profile', choices=['full', 'quick'], default='full',
        help='quick leaves out the live PostgreSQL tests, compiled code and the wasm32 build (default: full)')
    parser.add_argument(
        '--group', choices=GROUPS, action='append',
        help='run only this CI job\'s steps; repeatable (default: every group)')
    parser.add_argument(
        '--require-examples', action='store_true',
        help='fail on a built feature no teaching example uses, as the phase\'s examples chunk does '
             '(default: note it)')
    args = parser.parse_args()
    problems = self_test()
    if problems:
        print('FAILED the gate\'s own check:\n' + indent('\n'.join(problems)))
        return 1
    groups = set(args.group or GROUPS)
    chosen = [step for step in steps(args.profile, args.require_examples) if step.group in groups]
    report = Report()
    started_at = time.monotonic()

    # One build first: every test binary and `jadpo` (or `jadpo` alone if no
    # test binary is needed), so no later step builds and none waits on
    # cargo's lock.
    binaries: list[TestBinary] = []
    tools: dict[str, str] = {}
    built: Step | None = None
    begun = time.monotonic()
    if any(step.tests for step in chosen):
        # The workspace, or only what the steps chosen need.
        packages = packages_for(chosen, server=not os.environ.get('JADPO_TEST_DATABASE_URL', '').strip())
        built = Step('build the workspace and its tests' if packages is None
                     else f'build the tests of {", ".join(packages)}', 'tests')
        built.ok, built.output, binaries, tools = build_tests(packages)
        if not built.ok and 'jadpo' not in tools and any(step.jadpo for step in chosen):
            # The tests don't build, but the pages and examples may still
            # check with jadpo itself.
            tools = build_tools()[2]
    elif any(step.jadpo for step in chosen):
        built = Step('build jadpo', 'docs')
        built.ok, built.output, tools = build_tools()
    if built:
        built.seconds = time.monotonic() - begun
        report.finished(built)
    jadpo = tools.get('jadpo')

    # One PostgreSQL server for every step that uses one, unless
    # JADPO_TEST_DATABASE_URL already names one (CI's).
    server = None
    if (any(step.postgres for step in chosen) and SERVER_BIN in tools
            and not os.environ.get('JADPO_TEST_DATABASE_URL', '').strip()):
        server = PostgresServer(tools[SERVER_BIN], report)
    try:
        return run_steps(chosen, built, server, binaries, jadpo, report, started_at)
    finally:
        if server:
            server.stop()


def run_steps(chosen: list[Step], built: Step | None, server: PostgresServer | None,
              binaries: list[TestBinary], jadpo: str | None, report: Report, started_at: float) -> int:
    not_run = [step for step in chosen
               if step.tests and not built.ok or step.jadpo and not jadpo]

    pending = [step for step in chosen if step not in not_run]
    alone = [step for step in pending if step.alone]
    lanes: dict[str, list[Step]] = {}
    for step in pending:
        if step.lane and not step.alone:
            lanes.setdefault(step.lane, []).append(step)
    rest = [step for step in pending if not step.lane and not step.alone]
    with ThreadPoolExecutor(JOBS) as tests_pool, ThreadPoolExecutor(len(rest) + len(lanes) + 1) as steps_pool:
        # A lane runs its own test binaries, so it never waits behind the
        # pool's queue.
        def in_turn(lane: list[Step]) -> None:
            for step in lane:
                run_step(step, binaries, None, report, jadpo, server)
        futures = [steps_pool.submit(in_turn, lane) for lane in lanes.values()]
        futures += [steps_pool.submit(run_step, step, binaries, tests_pool, report, jadpo, server)
                    for step in rest]
        for future in futures:
            future.result()
    for step in alone:
        run_step(step, binaries, None, report, jadpo, server)

    if server:
        server.ready()
    ran = [step for step in (built, server and server.step) if step] + pending
    failed = [step for step in ran if not step.ok]
    for step in failed:
        print(f'\n== FAILED: {step.name}\n{step.output.rstrip()}', flush=True)
    total = time.monotonic() - started_at
    if not_run:
        print(f'\nnot run, because the build failed: {", ".join(step.name for step in not_run)}')
    if failed or not_run:
        print(f'\nFAILED: {len(failed)} of {len(ran)} steps in {total:.0f} s: '
              + ', '.join(step.name for step in failed))
        return 1
    print(f'\nok: {len(ran)} steps in {total:.0f} s')
    return 0


if __name__ == '__main__':
    sys.exit(main())
