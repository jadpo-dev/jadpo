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
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def step(name: str, command: list[str]) -> bool:
    print(f'== {name}', flush=True)
    result = subprocess.run(command, cwd=ROOT)
    ok = result.returncode == 0
    print(f'{"ok" if ok else "FAILED"}: {name}\n', flush=True)
    return ok


def main() -> int:
    checks = [('language specification', [sys.executable, 'tools/language_docs.py', '--check'])]
    if (ROOT / 'Cargo.toml').exists():
        checks.append(('compiler tests', ['cargo', 'test', '--locked', '--workspace']))
    results = [step(name, command) for name, command in checks]
    return 0 if all(results) else 1


if __name__ == '__main__':
    sys.exit(main())
