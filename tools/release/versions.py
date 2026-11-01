#!/usr/bin/env python3
"""Brings the files that repeat Jadpo's version into step with it
(docs/documentation.md, "Versions and releases").

release-please writes the next version into the files
release-please-config.json names: Cargo.toml's `[workspace.package]` and
editors/vscode/package.json. This writes it into the files that repeat
it: Cargo.lock's entry for each crate of the workspace, by
`cargo update --workspace`, and editors/vscode/package-lock.json's own
two. Nothing else in a lock file may change, or it fails.
release-please.yml runs it on the release pull request's branch, and
tools/release/rehearsal.py on a copy of the checkout at the next minor
release, so a change that would break the release pull request fails on
its own.

  python3 tools/release/versions.py           # write them
  python3 tools/release/versions.py --check   # fail if one is behind
  python3 tools/release/versions.py --root DIR
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXTENSION = Path('editors/vscode')


def workspace_version(root: Path) -> str:
    with open(root / 'Cargo.toml', 'rb') as manifest:
        return tomllib.load(manifest)['workspace']['package']['version']


def lock_entries(root: Path) -> dict[tuple[str, str | None], list[str]]:
    """Cargo.lock's packages: (name, source) to their versions. A crate of
    the workspace, or any path dependency, has no source."""
    with open(root / 'Cargo.lock', 'rb') as lock:
        packages = tomllib.load(lock)['package']
    entries: dict[tuple[str, str | None], list[str]] = {}
    for package in packages:
        entries.setdefault((package['name'], package.get('source')), []).append(package['version'])
    return entries


def members(root: Path) -> dict[str, str]:
    """The workspace's crates and their versions, as Cargo.toml gives them.
    `--no-deps` resolves nothing, so it needs neither the network nor the
    lock file."""
    process = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps'],
                             cwd=root, capture_output=True, text=True)
    if process.returncode:
        sys.exit(f'cargo metadata failed:\n{process.stderr}')
    return {package['name']: package['version'] for package in json.loads(process.stdout)['packages']}


def lock_problems(before: dict, after: dict, crates: dict[str, str]) -> list[str]:
    """What changed in Cargo.lock beyond the workspace crates' versions,
    and any workspace crate it still has at another version."""
    problems = []
    for key in sorted(set(before) | set(after), key=str):
        name, source = key
        if before.get(key) == after.get(key):
            continue
        if source is not None or name not in crates:
            problems.append(f'Cargo.lock: {name} {before.get(key)} became {after.get(key)}; only the workspace\'s '
                            'own crates may change')
    for name, version in sorted(crates.items()):
        if after.get((name, None)) != [version]:
            problems.append(f'Cargo.lock: {name} is {after.get((name, None))}, not {version}')
    return problems


def sync_lock(root: Path) -> list[str]:
    """`cargo update --workspace`: each workspace crate's entry takes its
    version from Cargo.toml and nothing else moves. It runs offline when
    cargo already holds the registry's index, as the gate's machines do,
    and fetches it otherwise, as a fresh CI runner must."""
    before = lock_entries(root)
    for offline in (['--offline'], []):
        process = subprocess.run(['cargo', 'update', '--workspace', *offline], cwd=root,
                                 capture_output=True, text=True)
        if process.returncode == 0:
            break
    else:
        return [f'cargo update --workspace failed:\n{process.stderr}']
    return lock_problems(before, lock_entries(root), members(root))


def sync_extension(root: Path, write: bool) -> list[str]:
    """package-lock.json's own version and its root package's, as
    package.json gives them."""
    folder = root / EXTENSION
    version = json.loads((folder / 'package.json').read_text())['version']
    path = folder / 'package-lock.json'
    text = path.read_text()
    lock = json.loads(text)
    if lock.get('version') == version and lock['packages'][''].get('version') == version:
        return []
    if not write:
        return [f'{EXTENSION}/package-lock.json is at {lock.get("version")}, package.json at {version}']
    lock['version'] = version
    lock['packages']['']['version'] = version
    path.write_text(json.dumps(lock, indent=2, ensure_ascii=False) + '\n')
    return []


def check(root: Path) -> list[str]:
    """Without writing anything: whether Cargo.lock and package-lock.json
    are at the workspace's version."""
    crates = members(root)
    entries = lock_entries(root)
    problems = [f'Cargo.lock: {name} is {entries.get((name, None))}, not {version}'
                for name, version in sorted(crates.items()) if entries.get((name, None)) != [version]]
    return problems + sync_extension(root, write=False)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='fail if a file is behind, writing nothing')
    parser.add_argument('--root', type=Path, default=ROOT, help='the checkout (default: this one)')
    args = parser.parse_args()
    root = args.root.resolve()
    problems = check(root) if args.check else sync_lock(root) + sync_extension(root, write=True)
    for problem in problems:
        print(f'FAIL {problem}')
    if not problems:
        print(f'every file repeating the version is at {workspace_version(root)}')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
