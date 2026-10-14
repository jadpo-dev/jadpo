#!/usr/bin/env python3
"""The gate every change must pass.

Today it checks the language specification. As the new compiler is built
(docs/roadmap.md), each phase adds its checks here, so this script stays the
single gate that CI and every session run.

  python3 tools/verify.py                  # everything
  python3 tools/verify.py --profile quick  # without live PostgreSQL
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WASM = 'wasm32-unknown-unknown'


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


def step(name: str, command: list[str], env: dict[str, str] | None = None) -> bool:
    print(f'== {name}', flush=True)
    result = subprocess.run(command, cwd=ROOT,
                            env={**os.environ, **env} if env else None)
    ok = result.returncode == 0
    print(f'{"ok" if ok else "FAILED"}: {name}\n', flush=True)
    return ok


def main() -> int:
    parser = argparse.ArgumentParser(description='The gate every change must pass.')
    parser.add_argument(
        '--profile', choices=['full', 'quick'], default='full',
        help='quick leaves out the live PostgreSQL tests (default: full)')
    args = parser.parse_args()
    # Each check is its name, its command and, optionally, what it adds to
    # the environment.
    checks: list[tuple] = [
        # Builds jadpo from this checkout and runs it over the `check` blocks;
        # projects that declare tests also run under `jadpo test` (Phase 2)
        # and are exported to build/spec-corpus/ (docs/compiler-runtime.md §2.7).
        ('language specification and jadpo test over the spec corpus',
         [sys.executable, 'tools/language_docs.py', '--check', '--export-corpus']),
        # Every teaching example in examples/ checks with no diagnostics,
        # is formatted, passes its own tests under `jadpo test`, has its
        # README, and is exported beside the pages' projects
        # (docs/compiler-runtime.md §2.7).
        ('teaching examples: check, format and jadpo test',
         [sys.executable, 'tools/examples.py', '--export-corpus']),
        ('example names', [sys.executable, 'tools/example_names.py', '--check']),
    ]
    if (ROOT / 'Cargo.toml').exists():
        checks += [
            ('rust formatting', ['cargo', 'fmt', '--all', '--check']),
            ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']),
            # Every example in docs/language and examples/ formats idempotently
            # to the same tree (docs/compiler-runtime.md §2.7).
            ('format idempotence', ['cargo', 'test', '--locked', '-p', 'jadpo-fmt', '--test', 'examples']),
            # Same source, same bytes (docs/compiler-runtime.md §2.8): every
            # corpus project's manifest twice, its files read in two orders,
            # and no key or digest moved by formatting.
            ('manifest determinism', ['cargo', 'test', '--locked', '-p', 'jadpo-check', '--test', 'determinism']),
            # The examples use every feature the compiler builds, read from
            # its own tables; the exemptions are listed with their reasons
            # in the test (crates/jadpo-check/tests/examples.rs).
            ('teaching examples cover every built feature',
             ['cargo', 'test', '--locked', '-p', 'jadpo-check', '--test', 'examples']),
            # Every construction the checker proved, across the spec corpus,
            # checked again as it runs (docs/compiler-runtime.md §2.7).
            ('proofs hold when they run',
             ['cargo', 'test', '--locked', '-p', 'jadpo-interp', '--test', 'proofs']),
            # Includes the crate-graph test (crates/jadpo-cli/tests/crate_graph.rs),
            # the parser's documentation, error and fuzz tests, and the
            # generated AST's drift test. The live PostgreSQL tests skip
            # here and run as their own step.
            ('compiler tests', ['cargo', 'test', '--locked', '--workspace'],
             {'JADPO_SKIP_POSTGRES': '1'}),
            # Phase 2 (docs/compiler-runtime.md §2.7): a save in a generated
            # 230-file project shows the tests it reaches, through the real
            # watch loop, within the 1-second target plus a CI allowance;
            # the times are printed.
            ('watch-mode latency',
             ['cargo', 'test', '--locked', '-p', 'jadpo-cli', '--test', 'watch',
              'a_save_shows_the_tests_it_reaches_within_the_target', '--', '--nocapture']),
        ]
        # Phase 3 (docs/compiler-runtime.md §2.7): the PostgreSQL driver
        # against the server JADPO_TEST_DATABASE_URL names, or a disposable
        # one from the machine's PostgreSQL tools.
        if args.profile == 'full':
            # The driver, the test database's template and copies, and
            # `jadpo migrate apply` and `status`.
            checks.append(('live PostgreSQL',
                           ['cargo', 'test', '--locked', '-p', 'jadpo-test',
                            '--test', 'postgres', '--', '--nocapture']))
            checks.append(('live PostgreSQL: jadpo migrate',
                           ['cargo', 'test', '--locked', '-p', 'jadpo-cli',
                            '--test', 'migrate']))
            # Queries, `get` and `create` run through `jadpo test` (area 6).
            checks.append(('live PostgreSQL: queries',
                           ['cargo', 'test', '--locked', '-p', 'jadpo-cli',
                            '--test', 'queries']))
        # rust-toolchain.toml lists the target, so rustup installs it with
        # the pinned toolchain. The drivers build with every driver they
        # have, so tokio-postgres is proven to build for wasm32 too.
        crates = wasm_crates()
        packages = [arg for name in crates for arg in ('-p', name)]
        features = (['--features', 'jadpo-drivers/postgres']
                    if 'jadpo-drivers' in crates else [])
        checks.append(('shared and runtime crates for wasm32',
                       ['cargo', 'build', '--locked', '--target', WASM, *packages, *features]))
    results = [step(*check) for check in checks]
    return 0 if all(results) else 1


if __name__ == '__main__':
    sys.exit(main())
