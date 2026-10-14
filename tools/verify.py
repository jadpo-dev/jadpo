#!/usr/bin/env python3
"""The gate every change must pass.

Today it checks the language specification. As the new compiler is built
(docs/roadmap.md), each phase adds its checks here, so this script stays the
single gate that CI and every session run.

  python3 tools/verify.py
"""
from __future__ import annotations

import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WASM = 'wasm32-unknown-unknown'


def wasm_crates() -> list[str]:
    """The crates that must build for WebAssembly: the shared and runtime
    layers of the crate graph (docs/compiler-runtime.md §2.1), read from the
    table the crate-graph test checks, that exist so far."""
    with open(ROOT / 'Cargo.toml', 'rb') as manifest:
        layers = tomllib.load(manifest)['workspace']['metadata']['layers']
    return [name for name in layers['shared'] + layers['runtime']
            if (ROOT / 'crates' / name / 'Cargo.toml').exists()]


def step(name: str, command: list[str]) -> bool:
    print(f'== {name}', flush=True)
    result = subprocess.run(command, cwd=ROOT)
    ok = result.returncode == 0
    print(f'{"ok" if ok else "FAILED"}: {name}\n', flush=True)
    return ok


def main() -> int:
    checks = [
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
            # generated AST's drift test.
            ('compiler tests', ['cargo', 'test', '--locked', '--workspace']),
            # Phase 2 (docs/compiler-runtime.md §2.7): a save in a generated
            # 230-file project shows the tests it reaches, through the real
            # watch loop, within the 1-second target plus a CI allowance;
            # the times are printed.
            ('watch-mode latency',
             ['cargo', 'test', '--locked', '-p', 'jadpo-cli', '--test', 'watch',
              'a_save_shows_the_tests_it_reaches_within_the_target', '--', '--nocapture']),
        ]
        # rust-toolchain.toml lists the target, so rustup installs it with
        # the pinned toolchain.
        packages = [arg for name in wasm_crates() for arg in ('-p', name)]
        checks.append(('shared and runtime crates for wasm32',
                       ['cargo', 'build', '--locked', '--target', WASM, *packages]))
    results = [step(name, command) for name, command in checks]
    return 0 if all(results) else 1


if __name__ == '__main__':
    sys.exit(main())
