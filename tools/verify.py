#!/usr/bin/env python3
"""The gate every change must pass.

As the new compiler is built (docs/roadmap.md), each phase adds its checks
here, so this script stays the single gate that CI and every session run
(docs/compiler-runtime.md §2.7).

  python3 tools/verify.py                  # everything
  python3 tools/verify.py --profile quick  # without live PostgreSQL or the wasm32 build
  python3 tools/verify.py --group lint     # one CI job's share (repeatable)
  python3 tools/verify.py --require-examples  # fail on a feature no example uses

It builds the workspace and its tests once, then runs every step that
needs nothing else concurrently: each test binary straight from that build,
cargo's own steps (clippy, documentation tests, wasm32) one after another
beside them, and the timing test last, alone. Each step prints `ok` or
`FAILED` as it finishes; failures' output follows, then a summary.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
import time
import tomllib
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WASM = 'wasm32-unknown-unknown'
# The CI jobs a step belongs to (.github/workflows/validation.yml).
GROUPS = ('lint', 'docs', 'tests', 'wasm32')
# Each test binary is a process of its own and runs its tests on threads;
# this many run at once.
JOBS = os.cpu_count() or 4


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
    (package, test target) pairs, `everything` for every binary, with
    `args` passed to each."""
    name: str
    group: str
    command: list[str] | None = None
    tests: list[tuple[str, str]] | str | None = None
    args: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)
    # Steps in the same lane run one after another, beside the rest:
    # `cargo` for those that take cargo's lock on the build folder,
    # `postgres` for the live tests when they share one server.
    lane: str | None = None
    # Runs `jadpo`, built from this checkout (JADPO_BIN).
    jadpo: bool = False
    # Measures time, so runs after everything else, by itself.
    alone: bool = False
    # Prints its output even when it passes (timings), or only the lines
    # that start with this text.
    show: bool | str = False
    # For `everything`: the binaries other steps run.
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


def steps(profile: str, require_examples: bool = False) -> list[Step]:
    python = sys.executable
    checks = [
        # Runs jadpo over the `check` blocks; projects that declare tests
        # also run under `jadpo test` (Phase 2) and are exported to
        # build/spec-corpus/ (docs/compiler-runtime.md §2.7).
        Step('language specification and jadpo test over the spec corpus', 'docs',
             [python, 'tools/language_docs.py', '--check', '--export-corpus'], jadpo=True),
        # Every teaching example in examples/ checks with no diagnostics,
        # is formatted, passes its own tests under `jadpo test`, has its
        # README, and is exported beside the pages' projects
        # (docs/compiler-runtime.md §2.7).
        Step('teaching examples: check, format and jadpo test', 'docs',
             [python, 'tools/examples.py', '--export-corpus'], jadpo=True),
        Step('example names', 'lint', [python, 'tools/example_names.py', '--check']),
        # The VS Code extension's highlighting names every word the lexer
        # reserves and no other (editors/vscode; needs no Node).
        Step('editor grammar in step with the lexer', 'lint',
             [python, 'tools/editor_grammar.py', '--check']),
    ]
    postgres = [
        # The driver, the test database's template and copies.
        ('live PostgreSQL', [('jadpo-test', 'postgres')], ['--nocapture']),
        # `jadpo migrate apply` and `status`.
        ('live PostgreSQL: jadpo migrate', [('jadpo-cli', 'migrate')], []),
        # Queries, `get` and `create` run through `jadpo test` (area 6).
        ('live PostgreSQL: queries', [('jadpo-cli', 'queries')], []),
        # Writes, their conflicts and counters through `jadpo test`, and a
        # concurrent change forced from a second connection, rerunning the
        # whole action (area 6).
        ('live PostgreSQL: writes and concurrent changes', [('jadpo-cli', 'writes')], []),
        # Lifecycles through `jadpo test`: hidden records, transitions and
        # their guards; then purge through the runtime's entry point, with
        # cascades, restricted references and batches (area 5).
        ('live PostgreSQL: lifecycles and purge', [('jadpo-cli', 'lifecycle')], []),
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
        *[Step(name, 'tests', tests=tests) for name, tests in own_steps],
        # Every other test binary in the workspace: the crate-graph test
        # (crates/jadpo-cli/tests/crate_graph.rs), the parser's
        # documentation, error and fuzz tests, the generated AST's drift
        # test, and the rest. The steps above and the live PostgreSQL steps
        # run their own binaries; under --profile quick the PostgreSQL
        # binaries run here, their live tests skipped. The watch tests run
        # here but for the timing test, which runs last, alone.
        Step('compiler tests', 'tests', tests='everything',
             args=['--exact', '--skip', latency_test], env={'JADPO_SKIP_POSTGRES': '1'}),
        # Examples in the crates' documentation comments.
        Step('documentation tests', 'tests',
             ['cargo', 'test', '--locked', '--workspace', '--doc'], lane='cargo'),
        # Phase 2 (docs/compiler-runtime.md §2.7): a save in a generated
        # 230-file project shows the tests it reaches, through the real
        # watch loop, within the 1-second target plus a CI allowance; the
        # times are printed.
        Step('watch-mode latency', 'tests', tests=[('jadpo-cli', 'watch')],
             args=['--exact', latency_test, '--nocapture'], alone=True, show=True),
    ]
    # Phase 3 (docs/compiler-runtime.md §2.7): the PostgreSQL driver against
    # the server JADPO_TEST_DATABASE_URL names, or a disposable one from the
    # machine's PostgreSQL tools.
    if profile == 'full':
        # Locally each binary starts a disposable server of its own; on the
        # one server JADPO_TEST_DATABASE_URL names (CI's), they take turns.
        shared = 'postgres' if os.environ.get('JADPO_TEST_DATABASE_URL') else None
        checks += [Step(name, 'tests', tests=tests, args=args, show='--nocapture' in args, lane=shared)
                   for name, tests, args in postgres]
        # rust-toolchain.toml lists the target, so rustup installs it with
        # the pinned toolchain. The drivers build with every driver they
        # have, so tokio-postgres is proven to build for wasm32 too.
        crates = wasm_crates()
        packages = [arg for name in crates for arg in ('-p', name)]
        features = (['--features', 'jadpo-drivers/postgres']
                    if 'jadpo-drivers' in crates else [])
        checks.append(Step('shared and runtime crates for wasm32', 'wasm32',
                           ['cargo', 'build', '--locked', '--target', WASM, *packages, *features],
                           lane='cargo'))
    coverage = next(step for step in checks if step.name == 'teaching examples cover every built feature')
    coverage.args, coverage.show = ['--nocapture'], 'note:'
    if require_examples:
        coverage.env['JADPO_REQUIRE_EXAMPLES'] = '1'
    # Binaries another step runs whole are left out of `compiler tests`.
    owned = {test for _, tests in own_steps for test in tests}
    if profile == 'full':
        owned |= {test for _, tests, _ in postgres for test in tests}
    for step in checks:
        if step.tests == 'everything':
            step.owned = owned
    return checks


def run(command: list[str], env: dict[str, str] | None = None, cwd: Path | str = ROOT) -> tuple[bool, str]:
    result = subprocess.run(command, cwd=cwd, env={**os.environ, **(env or {})},
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            stdin=subprocess.DEVNULL, text=True, errors='replace')
    return result.returncode == 0, result.stdout


def build_tests() -> tuple[bool, str, list[TestBinary], str | None]:
    """`cargo test --no-run` over the workspace: every test binary, and
    `jadpo` beside them, built once with one set of features."""
    process = subprocess.run(
        ['cargo', 'test', '--locked', '--workspace', '--no-run',
         '--message-format=json-render-diagnostics'],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, errors='replace')
    binaries, jadpo = [], None
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
        elif package == 'jadpo-cli' and target['name'] == 'jadpo':
            jadpo = message['executable']
    return process.returncode == 0, process.stderr, binaries, jadpo


def build_jadpo() -> tuple[bool, str, str | None]:
    """Only `jadpo`, for a run that needs no test binaries (CI's docs job)."""
    ok, output = run(['cargo', 'build', '--locked', '-p', 'jadpo-cli'])
    return ok, output, str(ROOT / 'target/debug/jadpo') if ok else None


def test_jobs(step: Step, binaries: list[TestBinary]):
    """The (label, command, cwd, env) of each binary `step` runs, as `cargo
    test` would run it: from its package's folder."""
    if step.tests == 'everything':
        chosen = [b for b in binaries if (b.package, b.target) not in step.owned or b.kind != 'test']
    else:
        chosen = [b for b in binaries if b.kind == 'test' and (b.package, b.target) in step.tests]
        missing = set(step.tests) - {(b.package, b.target) for b in chosen}
        if missing:
            raise LookupError(f'no test binary for {sorted(missing)}')
    for binary in chosen:
        # How to run it again on its own.
        selector = {'lib': '--lib', 'bin': f'--bin {binary.target}'}.get(binary.kind, f'--test {binary.target}')
        label = f'cargo test -p {binary.package} {selector}'
        env = {**step.env, 'CARGO_MANIFEST_DIR': binary.manifest_dir, 'CARGO_PKG_NAME': binary.package}
        yield label, [binary.executable, *step.args], binary.manifest_dir, env


class Report:
    """Prints each step as it finishes, one line, under a lock."""

    def __init__(self) -> None:
        self.lock = threading.Lock()

    def finished(self, step: Step) -> None:
        with self.lock:
            word = 'ok' if step.ok else 'FAILED'
            print(f'{word:<6} {step.seconds:6.1f} s  {step.name}', flush=True)
            shown = step.output.strip('\n')
            if isinstance(step.show, str):
                shown = '\n'.join(line for line in shown.splitlines() if line.startswith(step.show))
            if step.show and step.ok and shown.strip():
                print(indent(shown), flush=True)


def indent(text: str) -> str:
    return '\n'.join(f'    {line}' for line in text.rstrip().splitlines())


def run_step(step: Step, binaries: list[TestBinary], pool: ThreadPoolExecutor | None,
             report: Report, jadpo: str | None) -> None:
    start = time.monotonic()
    if step.tests is None:
        env = {**step.env, **({'JADPO_BIN': jadpo} if step.jadpo and jadpo else {})}
        step.ok, step.output = run(step.command, env)
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
            failed = [(label, output) for label, ok, output in results if not ok]
            step.ok = not failed
            if failed:
                step.output = '\n'.join(f'--- {label}\n{output}' for label, output in failed)
            elif step.show:
                step.output = '\n'.join(output for _, _, output in results)
    step.seconds = time.monotonic() - start
    report.finished(step)


def main() -> int:
    parser = argparse.ArgumentParser(description='The gate every change must pass.')
    parser.add_argument(
        '--profile', choices=['full', 'quick'], default='full',
        help='quick leaves out the live PostgreSQL tests and the wasm32 build (default: full)')
    parser.add_argument(
        '--group', choices=GROUPS, action='append',
        help='run only this CI job\'s steps; repeatable (default: every group)')
    parser.add_argument(
        '--require-examples', action='store_true',
        help='fail on a built feature no teaching example uses, as the phase\'s examples chunk does '
             '(default: note it)')
    args = parser.parse_args()
    groups = set(args.group or GROUPS)
    chosen = [step for step in steps(args.profile, args.require_examples) if step.group in groups]
    report = Report()
    started = time.monotonic()

    # One build first: every test binary and `jadpo` (or `jadpo` alone if no
    # test binary is needed), so no later step builds and none waits on
    # cargo's lock.
    binaries: list[TestBinary] = []
    jadpo = None
    built: Step | None = None
    begun = time.monotonic()
    if any(step.tests for step in chosen):
        built = Step('build the workspace and its tests', 'tests')
        built.ok, built.output, binaries, jadpo = build_tests()
        if not built.ok and not jadpo and any(step.jadpo for step in chosen):
            # The tests don't build, but the pages and examples may still
            # check with jadpo itself.
            jadpo = build_jadpo()[2]
    elif any(step.jadpo for step in chosen):
        built = Step('build jadpo', 'docs')
        built.ok, built.output, jadpo = build_jadpo()
    if built:
        built.seconds = time.monotonic() - begun
        report.finished(built)
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
                run_step(step, binaries, None, report, jadpo)
        futures = [steps_pool.submit(in_turn, lane) for lane in lanes.values()]
        futures += [steps_pool.submit(run_step, step, binaries, tests_pool, report, jadpo) for step in rest]
        for future in futures:
            future.result()
    for step in alone:
        run_step(step, binaries, None, report, jadpo)

    ran = ([built] if built else []) + pending
    failed = [step for step in ran if not step.ok]
    for step in failed:
        print(f'\n== FAILED: {step.name}\n{step.output.rstrip()}', flush=True)
    total = time.monotonic() - started
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
