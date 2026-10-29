#!/usr/bin/env python3
"""Check a pull request's Conventional Commit type against what it changes
(docs/documentation.md, "Versions and releases").

release-please sets each release's version from the Conventional Commit
types it reads: `fix` a patch, `feat` a minor, and `feat!` (or a
`BREAKING CHANGE:` footer) a breaking change, which raises the minor number
until 1.0. Pull requests merge with a merge commit, so the commits
release-please reads are the branch's own; the pull request's title says
which one of those the change is. Three things fail:

- a title that isn't a Conventional Commit;
- a change to the language's behaviour under a title other than `feat` or
  `feat!`. The language changes when a spec example gives a different result
  for the same source (a `jadpo check` block's `expect=` or the output block
  after it, on any page under docs/ or in README.md), or when a rule enters
  or leaves the compiler's catalogue (`jadpo help --json`, declared in
  crates/jadpo-diag/src/lib.rs);
- a title whose release (none, patch, minor or breaking) isn't the
  strongest of the branch's commits, since that is the one release-please
  makes.

  python3 tools/commit_type.py --base <sha> --head <sha> --title "<title>"
  python3 tools/commit_type.py --catalogue   # the parse agrees with jadpo help --json

The first compares the merge base of `base` and `head` with `head`, and
reads the commits in `base..head` that aren't merges.
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOGUE = 'crates/jadpo-diag/src/lib.rs'
# One `rules!` entry: `NAME: "area.rule", area N, ...`.
RULE = re.compile(r'^\s+[A-Z][A-Z0-9_]*: "([a-z0-9_]+\.[a-z0-9_]+)", area \d+,', re.MULTILINE)
FENCE = re.compile(r'^```(\S*)(.*)$')
OUTPUT_FENCE = '```text output'
TITLE = re.compile(r'^(?P<type>[a-z]+)(?:\((?P<scope>[^()\s]+)\))?(?P<bang>!)?: (?P<subject>\S.*)$')
BREAKING_FOOTER = re.compile(r'^BREAKING[ -]CHANGE: ', re.MULTILINE)
# Each type and the release it makes, as release-please reads it.
TYPES = {
    'feat': 'minor', 'fix': 'patch', 'perf': 'patch', 'revert': 'patch', 'deps': 'patch',
    'build': 'none', 'chore': 'none', 'ci': 'none', 'docs': 'none', 'refactor': 'none',
    'style': 'none', 'test': 'none',
}
LEVELS = ['none', 'patch', 'minor', 'breaking']
SHOWN = 5


def git(*args, input=None):
    result = subprocess.run(['git', *args], cwd=ROOT, capture_output=True, text=True, input=input)
    if result.returncode:
        sys.exit(f'git {" ".join(args)} failed:\n{result.stderr}')
    return result.stdout


def files_at(rev, paths):
    """Each path's text at `rev`, read in one `git cat-file --batch`."""
    if not paths:
        return {}
    out = subprocess.run(['git', 'cat-file', '--batch'], cwd=ROOT, capture_output=True,
                         input=''.join(f'{rev}:{p}\n' for p in paths).encode()).stdout
    texts, at = {}, 0
    for path in paths:
        end = out.index(b'\n', at)
        header = out[at:end].split()
        at = end + 1
        if len(header) < 3 or header[1] == b'missing':
            continue
        size = int(header[2])
        texts[path] = out[at:at + size].decode()
        at += size + 1
    return texts


def doc_paths(rev):
    listed = git('ls-tree', '-r', '--name-only', rev, '--', 'docs', 'README.md').split('\n')
    return [p for p in listed if p.endswith('.md')]


def parse_attrs(text):
    attrs = {}
    for token in text.split():
        name, _, value = token.partition('=')
        attrs[name] = value if value else True
    return attrs


def strip_front_matter(text):
    if text.startswith('---\n') and '\n---\n' in text[4:]:
        return text[text.index('\n---\n', 4) + 5:]
    return text


def examples(path, text):
    """The page's `jadpo check` projects, as {source digest: (name, result)}:
    what the project compiles, and what it gives (each block's `expect=` and
    the output block after it)."""
    lines = strip_front_matter(text).splitlines()
    projects, i, n = {}, 0, 0
    while i < len(lines):
        match = FENCE.match(lines[i])
        if not (match and match.group(1) == 'jadpo'):
            i += 1
            continue
        attrs = parse_attrs(match.group(2))
        j = i + 1
        while j < len(lines) and not lines[j].startswith('```'):
            j += 1
        n += 1
        if attrs.get('check'):
            source = '\n'.join(lines[i + 1:j])
            k = j + 1
            while k < len(lines) and not lines[k].strip():
                k += 1
            output = None
            if k < len(lines) and lines[k].strip() == OUTPUT_FENCE:
                m = k + 1
                while m < len(lines) and not lines[m].startswith('```'):
                    m += 1
                output = '\n'.join(lines[k + 1:m])
            name = attrs.get('project', f'example-{n}')
            given = {key: value for key, value in attrs.items() if key not in ('expect', 'project')}
            projects.setdefault(name, []).append((json.dumps(given, sort_keys=True), source,
                                                  attrs.get('expect'), output))
        i = j + 1
    found = {}
    for name, blocks in projects.items():
        digest = hashlib.sha256(json.dumps([b[:2] for b in blocks]).encode()).hexdigest()
        found[digest] = (f'{path}: {name}', [b[2:] for b in blocks])
    return found


def spec_results(rev):
    """{source digest: (a name, the set of results)}: the same source may be
    shown in more than one place."""
    found = {}
    for path, text in sorted(files_at(rev, doc_paths(rev)).items()):
        for digest, (name, result) in examples(path, text).items():
            entry = found.setdefault(digest, (name, set()))
            entry[1].add(json.dumps(result))
    return found


def catalogue(text):
    return set(RULE.findall(text))


def language_changes(before, after):
    """What changed in the language between two revisions: each spec example
    whose result changed for the same source, and each rule that entered or
    left the catalogue."""
    changes = []
    old, new = spec_results(before), spec_results(after)
    for digest in sorted(old.keys() & new.keys(), key=lambda d: new[d][0]):
        if old[digest][1] != new[digest][1]:
            changes.append(f'{new[digest][0]} gives a different result')
    rules_before = catalogue(files_at(before, [CATALOGUE]).get(CATALOGUE, ''))
    rules_after = catalogue(files_at(after, [CATALOGUE]).get(CATALOGUE, ''))
    changes += [f'rule {rule} entered the catalogue' for rule in sorted(rules_after - rules_before)]
    changes += [f'rule {rule} left the catalogue' for rule in sorted(rules_before - rules_after)]
    return changes


def level(message):
    """The release a commit message makes, or None when it isn't a
    Conventional Commit, which release-please skips."""
    header, _, body = message.strip().partition('\n')
    match = TITLE.match(header.strip())
    if not match or match.group('type') not in TYPES:
        return None
    if match.group('bang') or BREAKING_FOOTER.search(body):
        return 'breaking'
    return TYPES[match.group('type')]


def commits(base, head):
    """The branch's own commits, without merges: (sha, message)."""
    out = git('log', '--format=%H%x00%P%x00%B%x1e', f'{base}..{head}')
    found = []
    for entry in out.split('\x1e'):
        if not entry.strip():
            continue
        sha, parents, message = entry.strip('\n').split('\x00', 2)
        if len(parents.split()) < 2:
            found.append((sha, message))
    return found


def problems(base, head, title):
    title_level = level(title)
    if title_level is None:
        return [(f'The title `{title}` isn\'t a Conventional Commit',
                 'release-please reads Conventional Commits: `type(scope): subject`, the type one of '
                 + ', '.join(f'`{t}`' for t in TYPES) + ', and `!` after it for a breaking change.',
                 'Start the title with its type, such as `fix: ` or `feat: `')]
    found = []
    changes = language_changes(git('merge-base', base, head).strip(), head)
    if changes and title_level not in ('minor', 'breaking'):
        shown = '; '.join(changes[:SHOWN]) + (f'; and {len(changes) - SHOWN} more' if len(changes) > SHOWN else '')
        found.append(('The language changes under a type that says it doesn\'t',
                      f'The title says `{TITLE.match(title).group("type")}`, but {shown}.',
                      'Retitle the pull request `feat: ...`, or `feat!: ...` if an app that checked before no '
                      'longer does, and give a branch commit the same type'))
    branch = commits(base, head)
    levels = [(level(message), sha) for sha, message in branch]
    strongest = max((lv for lv, _ in levels if lv), key=LEVELS.index, default='none')
    if branch and strongest != title_level:
        if LEVELS.index(strongest) > LEVELS.index(title_level):
            by = [sha[:8] for lv, sha in levels if lv == strongest]
            reason = (f'The title makes {describe(title_level)}, but release-please reads the branch\'s '
                      f'commits, and {", ".join(by[:SHOWN])} make{"s" if len(by) == 1 else ""} '
                      f'{describe(strongest)}.')
            next_step = 'Retitle the pull request with the type of its strongest commit, or reword that commit'
        else:
            reason = (f'The title makes {describe(title_level)}, but release-please reads the branch\'s '
                      f'commits, and the strongest of them makes {describe(strongest)}.')
            next_step = 'Give a branch commit the title\'s type, so the release does what the title says'
        found.append(('The title and the branch\'s commits make different releases', reason, next_step))
    return found


def describe(lv):
    return {'none': 'no release', 'patch': 'a patch release', 'minor': 'a minor release',
            'breaking': 'a breaking release'}[lv]


def check_catalogue():
    """The rules this tool reads from the source are `jadpo help --json`'s."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import language_docs
    listed = set(language_docs.load_catalogue())
    parsed = catalogue((ROOT / CATALOGUE).read_text())
    if listed != parsed:
        missing, extra = sorted(listed - parsed), sorted(parsed - listed)
        sys.exit(f'{CATALOGUE} parses to rules jadpo help --json doesn\'t list: {extra[:SHOWN]}; '
                 f'and misses {missing[:SHOWN]}')
    print(f'commit types: the catalogue\'s {len(parsed)} rules read the same from {CATALOGUE} '
          'and jadpo help --json')


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--base', help='the base branch\'s commit')
    parser.add_argument('--head', help='the pull request\'s commit')
    parser.add_argument('--title', help='the pull request\'s title')
    parser.add_argument('--catalogue', action='store_true',
                        help='check that the catalogue parse agrees with jadpo help --json')
    args = parser.parse_args()
    if args.catalogue:
        check_catalogue()
        return
    if not (args.base and args.head and args.title is not None):
        parser.error('--base, --head and --title are needed')
    found = problems(args.base, args.head, args.title.strip())
    for summary, reason, next_step in found:
        print(f'error: {summary}\n  reason: {reason}\n  next: {next_step}\n')
    if found:
        sys.exit(f'commit type check failed: {len(found)} error{"s" if len(found) > 1 else ""}')
    print(f'commit type check passed: `{args.title.strip()}` makes {describe(level(args.title))}')


if __name__ == '__main__':
    main()
