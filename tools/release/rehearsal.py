#!/usr/bin/env python3
"""The next release, rehearsed on a copy of this checkout
(docs/documentation.md, "Versions and releases").

A release pull request changes nothing but the version, so whatever breaks
on it breaks on any pull request that moves the version on. This makes
that move in a copy of the checkout (its tracked and new files, under
the build folder's `release-rehearsal/`): the version release-please
would write for the next minor release, in the files
release-please-config.json names, and tools/release/versions.py's update
of the files that repeat it, as release-please.yml runs it. Then the
copy must build `jadpo` with `--locked`, report the new version, and
pass the docs check and the teaching examples, without PostgreSQL. A
change that would leave the release pull request red fails here, on its
own pull request.

It builds in the gate's build folder, so it compiles only the
workspace's own crates again, and puts back the `jadpo` the gate built
there. tools/verify.py runs it in the `release` group, after every other
step.

  python3 tools/release/rehearsal.py
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import versions  # noqa: E402

# What release-please-config.json has release-please write the version
# into, each as this rehearsal writes it. A file added there needs a way
# to write it here too, or the rehearsal fails naming it.
EXTRA_FILES = [
    {'type': 'toml', 'path': 'Cargo.toml', 'jsonpath': '$.workspace.package.version'},
    {'type': 'json', 'path': 'editors/vscode/package.json', 'jsonpath': '$.version'},
]


def target_dir() -> Path:
    process = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps'],
                             cwd=ROOT, capture_output=True, text=True)
    if process.returncode:
        sys.exit(f'cargo metadata failed:\n{process.stderr}')
    return Path(json.loads(process.stdout)['target_directory'])


def next_minor(version: str, config: dict) -> str:
    """The version a `feat` makes next: the minor number up, also before
    1.0 (`bump-minor-pre-major`)."""
    major, minor, _ = (int(part) for part in re.match(r'(\d+)\.(\d+)\.(\d+)', version).groups())
    if major == 0 and not config.get('bump-minor-pre-major'):
        return f'{major + 1}.0.0'
    return f'{major}.{minor + 1}.0'


def copy_checkout(scratch: Path) -> None:
    """The checkout's tracked and new files, as they are on disk, in a
    repository of their own, so a tool that asks git which files there
    are reads the copy's."""
    if scratch.exists():
        shutil.rmtree(scratch)
    scratch.mkdir(parents=True)
    listed = subprocess.run(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'],
                            cwd=ROOT, capture_output=True, text=True, check=True).stdout
    for name in listed.split('\0'):
        source = ROOT / name
        if not name or not (source.exists() or source.is_symlink()):
            continue
        target = scratch / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_symlink():
            target.symlink_to(os.readlink(source))
        else:
            shutil.copy2(source, target)
    subprocess.run(['git', 'init', '--quiet'], cwd=scratch, check=True)


def bump(scratch: Path) -> tuple[str, list[str]]:
    """The next minor version, written as release-please writes it: the
    manifest and each of the config's extra files."""
    config = json.loads((scratch / 'release-please-config.json').read_text())
    package = config['packages']['.']
    if package.get('extra-files') != EXTRA_FILES:
        return '', ['release-please-config.json\'s extra-files are not the ones tools/release/rehearsal.py '
                    'writes (EXTRA_FILES): make each written here too']
    manifest_path = scratch / '.release-please-manifest.json'
    manifest = json.loads(manifest_path.read_text())
    version = next_minor(manifest['.'], config)
    manifest['.'] = version
    manifest_path.write_text(json.dumps(manifest, separators=(',', ':')) + '\n')

    problems = []
    cargo = scratch / 'Cargo.toml'
    text = cargo.read_text()
    table = text.index('\n[workspace.package]\n')
    written, count = re.subn(r'^version = "[^"]*"$', f'version = "{version}"', text[table:], count=1,
                             flags=re.MULTILINE)
    cargo.write_text(text[:table] + written)
    with open(cargo, 'rb') as manifest_file:
        if count != 1 or tomllib.load(manifest_file)['workspace']['package']['version'] != version:
            problems.append('Cargo.toml: no `version` in [workspace.package] to write')
    extension = scratch / 'editors/vscode/package.json'
    text = extension.read_text()
    written, count = re.subn(r'^  "version": "[^"]*",$', f'  "version": "{version}",', text, count=1,
                             flags=re.MULTILINE)
    extension.write_text(written)
    if count != 1 or json.loads(written)['version'] != version:
        problems.append('editors/vscode/package.json: no top-level `version` to write')
    return version, problems


def build(scratch: Path, target: Path) -> tuple[Path | None, str]:
    """`jadpo` built from the copy with `--locked`, in the gate's build
    folder, so the dependencies' builds are reused; cargo leaves it where
    the gate's `jadpo` is, so that is put back, and the copy's moved
    beside the copy."""
    gate = target / 'debug' / ('jadpo.exe' if os.name == 'nt' else 'jadpo')
    aside = scratch.parent / 'gate-jadpo'
    if gate.exists():
        os.replace(gate, aside)
    try:
        process = subprocess.run(['cargo', 'build', '--locked', '-p', 'jadpo-cli', '--bins',
                                  '--message-format=json-render-diagnostics'],
                                 cwd=scratch, env={**os.environ, 'CARGO_TARGET_DIR': str(target)},
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, errors='replace')
        executable = None
        for line in process.stdout.splitlines():
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if message.get('reason') == 'compiler-artifact' and message['target']['name'] == 'jadpo' \
                    and message.get('executable'):
                executable = Path(message['executable'])
        if process.returncode or executable is None:
            return None, process.stderr
        built = scratch.parent / 'jadpo'
        os.replace(executable, built)
        return built, ''
    finally:
        if aside.exists():
            os.replace(aside, gate)


def run(command: list[str], scratch: Path, jadpo: Path) -> tuple[bool, str]:
    env = {**os.environ, 'JADPO_BIN': str(jadpo), 'JADPO_SKIP_POSTGRES': '1', 'JADPO_TOOLCHAIN': 'local'}
    process = subprocess.run(command, cwd=scratch, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                             stdin=subprocess.DEVNULL, text=True, errors='replace')
    return process.returncode == 0, process.stdout


def main() -> int:
    target = target_dir()
    scratch = target / 'release-rehearsal' / 'checkout'
    copy_checkout(scratch)
    version, problems = bump(scratch)
    if not problems:
        problems = versions.sync_lock(scratch) + versions.sync_extension(scratch, write=True)
    if problems:
        print('\n'.join(f'FAIL {problem}' for problem in problems))
        return 1
    print(f'the next release, {version}: Cargo.lock and package-lock.json brought into step')
    jadpo, said = build(scratch, target)
    if jadpo is None:
        print(f'FAIL cargo build --locked at {version}:\n{said}')
        return 1
    shown = subprocess.run([str(jadpo), '--version'], capture_output=True, text=True,
                           env={**os.environ, 'JADPO_TOOLCHAIN': 'local'}).stdout.strip()
    if shown != f'jadpo {version}':
        print(f'FAIL jadpo --version says `{shown}`, not `jadpo {version}`')
        return 1
    print(f'built jadpo {version} with --locked')
    checks = {'the docs check': [sys.executable, 'tools/docs/language_docs.py', '--check'],
              'the teaching examples': [sys.executable, 'tools/docs/examples.py']}
    with ThreadPoolExecutor(len(checks)) as pool:
        results = dict(zip(checks, pool.map(lambda command: run(command, scratch, jadpo), checks.values())))
    failed = [name for name, (ok, _) in results.items() if not ok]
    for name, (ok, out) in results.items():
        if not ok:
            print(f'FAIL {name} at {version}:\n{out}')
    if failed:
        return 1
    print(f'the docs check and the teaching examples pass at {version}')
    shutil.rmtree(scratch.parent)
    return 0


if __name__ == '__main__':
    sys.exit(main())
