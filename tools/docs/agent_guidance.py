#!/usr/bin/env python3
"""Write what a coding agent reads first, from the docs it summarises.

Four files carry it (roadmap chunks A6 and A12):

- the AGENTS.md that `jadpo new` writes, from
  crates/jadpo-cli/scaffold/AGENTS.md.tmpl: its working method is written
  by hand, and everything from its "The closest example for each feature"
  heading on is generated here, its examples linked at `{release}`, which
  `jadpo new` fills with its own version;
- llms.txt, whose "Complete application examples" section is generated
  here;
- docs/primer.md, whose final "Not built yet" section is generated here;
- docs/learn/01-setting-up.md, whose "Install it" section is generated
  here.

What they generate from, so none can drift from what it summarises:

- each area page's "What is built" topic: the rows of its first table
  whose status isn't "Built", one line per area, linked to that topic in
  the docs bundle (docs/documentation.md, "The bundle");
- examples/README.md: its numbered reading order, every teaching example
  with what it teaches, and its "The closest example for each feature"
  table;
- what a release ships: dist-workspace.toml's installers and tap, the npm
  package and formula in crates/jadpo-cli/Cargo.toml, and the repository
  in Cargo.toml.

No other line of the primer or the Learn path says how to get `jadpo` by
hand. Every teaching example (a folder of examples/ holding app.jadpo) must be
in the reading order, and every example the feature table names must
exist.

  python3 tools/docs/agent_guidance.py           # write both files
  python3 tools/docs/agent_guidance.py --check   # fail if either is stale
"""
from __future__ import annotations

import argparse
import re
import sys
import textwrap
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / 'crates/jadpo-cli/scaffold/AGENTS.md.tmpl'
LLMS = ROOT / 'llms.txt'
PRIMER = ROOT / 'docs/primer.md'
LEARN_SETUP = ROOT / 'docs/learn/01-setting-up.md'

TOPICS = 'https://jadpo.dev/docs/'
# `{release}` is left for `jadpo new`, which writes its own version there,
# so a project's examples are the ones its release was tested with.
GITHUB_TREE = 'https://github.com/jadpo-dev/jadpo/tree/v{{release}}/examples/{folder}'
RAW_README = 'https://raw.githubusercontent.com/jadpo-dev/jadpo/main/examples/{folder}/README.md'

FEATURES_HEADING = '## The closest example for each feature'
LLMS_HEADING = '## Complete application examples'
NOT_BUILT_HEADING = '## Not built yet'
INSTALL_HEADING = '## Install it'

# What the primer names as not built: the row of an area's "What is built"
# table (the start of its feature, as `plain` gives it), and the words the
# primer uses. A row that comes to say "Built" leaves the list by itself.
# These are the features an agent is likeliest to reach for; the area
# tables have the rest.
PRIMER_NOT_BUILT = (
    ('05-data', '`jadpo rename`', '`jadpo rename`'),
    ('05-data', 'Entities in Redis', 'entities and caches in Redis'),
    ('04-failures', 'Logging', 'failure logging'),
    ('08-identity', '`Driver.github`', 'sign-in with GitHub, Apple or your own OAuth provider'),
    ('08-identity', 'OAuth grants', 'OAuth grants'),
    ('08-identity', 'Scope declarations', 'token scopes'),
    ('15-personal-data', 'Everything else on this page', 'most of personal data'),
    ('17-debugging-and-review', 'The developer console', 'the developer console and MCP'),
    ('13-testing', 'Data builders', 'data builders and `assert emitted` in tests'),
    ('16-built-in-services', 'Documents', 'documents'),
)

# Getting `jadpo` by hand, which a first page says only through a link to
# the generated install section.
BY_HAND = re.compile(r'cargo build|target/release|no installer|\bbuilt? .*\bfrom (?:its )?source')

LINK = re.compile(r'\[([^\]]*)\]\([^)]*\)')
# A parenthesis holding nothing but a link: the cross-reference it was.
LINKED_ASIDE = re.compile(r'\s*\(\[[^\]]*\]\([^)]*\)\)')
TOPIC = re.compile(r'^<!-- topic: (\S+?)(?: \(was [^)]*\))? -->$')
ITEM = re.compile(r'^\d+\. \[([^\]/]+)/?\]\(([^)]*)\): (.*)$')


def plain(cell: str) -> str:
    """A table cell without its links: a link's text stays, a
    parenthesis holding only a link goes."""
    return LINK.sub(r'\1', LINKED_ASIDE.sub('', cell)).strip()


def cells(line: str) -> list[str]:
    return [c.strip() for c in line.strip().strip('|').split('|')]


def section(text: str, heading: str) -> str | None:
    """The text under a `## ` heading, to the next `## ` heading."""
    start = text.find(f'\n{heading}\n')
    if start < 0:
        return None
    body = text[start + len(heading) + 2:]
    end = body.find('\n## ')
    return body if end < 0 else body[:end]


def first_table(text: str) -> list[list[str]]:
    """The body rows of the first Markdown table in `text`."""
    rows, started = [], False
    for line in text.split('\n'):
        if line.startswith('|'):
            started = True
            if not line.startswith('|---'):
                rows.append(cells(line))
        elif started:
            break
    return rows[1:]


def not_built(root: Path = ROOT) -> list[tuple[str, str, list[str]]]:
    """Each area's title, its "What is built" topic ID, and the features
    its table says aren't built, a partly built one marked so."""
    areas = []
    for path in sorted((root / 'docs/language').glob('[0-9][0-9]-*.md')):
        text = path.read_text()
        title = next(line[2:].strip() for line in text.split('\n') if line.startswith('# '))
        built = section(text, '## What is built')
        if built is None:
            continue
        topic = TOPIC.match(built.lstrip('\n').split('\n')[0])
        features = []
        for row in first_table(built):
            feature, status = row[0], row[-1]
            if status.startswith('Built'):
                continue
            name = plain(feature)
            if name == 'Everything else on this page':
                name = 'everything else in the area'
            features.append(f'{name} (partly)' if status.startswith('Partly built') else name)
        areas.append((title, topic.group(1) if topic else '', features))
    return areas


def area_rows(path: Path) -> list[tuple[str, str]]:
    """An area page's "What is built" table: each feature and its status."""
    built = section(path.read_text(), '## What is built') if path.is_file() else None
    return [(plain(row[0]), row[-1]) for row in first_table(built or '')]


def primer_not_built(root: Path = ROOT, picks=PRIMER_NOT_BUILT) -> str:
    """The primer's final section: the picks whose rows aren't "Built"."""
    named = []
    for page, start, words in picks:
        rows = area_rows(root / f'docs/language/{page}.md')
        if any(name.startswith(start) and not status.startswith('Built') for name, status in rows):
            named.append(words)
    text = "A feature that isn't built is the error `toolchain.not_built_yet` where you use it."
    if named:
        listed = '; '.join([*named[:-1], f'and {named[-1]}'] if len(named) > 1 else named)
        text += f" Not built yet, among others: {listed}. Each area's `What is built` topic has the rest."
    return f'{NOT_BUILT_HEADING}\n\n{textwrap.fill(text, 72, break_on_hyphens=False)}\n'


def primer_pick_problems(root: Path = ROOT, picks=PRIMER_NOT_BUILT) -> list[str]:
    found = []
    for page, start, _ in picks:
        if not any(name.startswith(start) for name, _ in area_rows(root / f'docs/language/{page}.md')):
            found.append(f'the primer\'s list reads "{start}" in docs/language/{page}.md, whose "What is built" '
                         'table has no such row: change PRIMER_NOT_BUILT in tools/docs/agent_guidance.py')
    return found


def install_facts(root: Path = ROOT) -> dict:
    def read(name: str) -> dict:
        return tomllib.loads((root / name).read_text())
    return {
        'dist': read('dist-workspace.toml')['dist'],
        'cli': read('crates/jadpo-cli/Cargo.toml')['package']['metadata']['dist'],
        'repository': read('Cargo.toml')['workspace']['package']['repository'],
    }


def install_commands(root: Path = ROOT) -> list[tuple[str, str]]:
    """How to install a release, one (where, command) per installer dist
    ships, in the order the site's install page gives them. dist names its
    installer scripts after the package with the binary."""
    facts = install_facts(root)
    dist, cli = facts['dist'], facts['cli']
    latest = f"{facts['repository']}/releases/latest/download/jadpo-cli-installer"
    owner = dist.get('tap', '').split('/')[0]
    known = {
        'shell': ('On macOS or Linux:', f"curl --proto '=https' --tlsv1.2 -LsSf {latest}.sh | sh"),
        'powershell': ('On Windows, in PowerShell:', f'powershell -c "irm {latest}.ps1 | iex"'),
        'homebrew': ('With Homebrew:', f"brew install {owner}/tap/{cli.get('formula')}"),
        'npm': ('With npm:', f"npm install -g {dist.get('npm-scope')}/{cli.get('npm-package')}"),
    }
    return [known[name] for name in known if name in dist['installers']]


def install_problems(root: Path = ROOT) -> list[str]:
    known = {'shell', 'powershell', 'homebrew', 'npm'}
    return [f'dist-workspace.toml ships the installer "{name}", which tools/docs/agent_guidance.py has no command for'
            for name in install_facts(root)['dist']['installers'] if name not in known]


def learn_install(root: Path = ROOT) -> str:
    """The first Learn page's "Install it" section."""
    ways = '\n\n'.join(f'{where}\n\n```sh\n{command}\n```' for where, command in install_commands(root))
    return f"""{INSTALL_HEADING}

Pick one of these. Each puts `jadpo` on your `PATH`, and you install it
once, not once per project.

{ways}

Open a new terminal, and check that `jadpo` runs:

```sh
jadpo --version
```

```text
jadpo X.Y.Z
```

Each project names the Jadpo release line it is written for, and whichever
`jadpo` you run fetches and runs that release for it, so projects on
different releases work side by side.
"""


def install_claim_problems(root: Path = ROOT) -> list[str]:
    """A first page saying how to get `jadpo` by hand, outside the section
    written from what a release ships."""
    found = []
    for path in [*sorted((root / 'docs/learn').glob('*.md')), root / 'docs/primer.md']:
        if not path.is_file():
            continue
        for number, line in enumerate(path.read_text().split('\n'), 1):
            claim = BY_HAND.search(line)
            if claim:
                found.append(f'{path.relative_to(root)}:{number}: says how to get `jadpo` by hand (`{claim.group(0)}`); '
                             "the first Learn page's `## Install it` is generated from what a release ships, so link it")
    return found


def with_section(text: str, heading: str, body: str) -> str:
    """`text` with its `## ` section under `heading` replaced by `body`,
    which holds the heading."""
    start = text.index(f'\n{heading}\n') + 1
    end = text.find('\n## ', start + len(heading))
    return text[:start] + body + ('' if end < 0 else text[end:])


def built_table(areas: list[tuple[str, str, list[str]]]) -> str:
    lines = ['| Area | Not built yet, or only partly |', '|---|---|']
    for title, topic, features in areas:
        lines.append(f'| [{title}]({TOPICS}{topic}.md) | {"; ".join(features) if features else "All of it"} |')
    return '\n'.join(lines)


def examples(root: Path = ROOT) -> list[tuple[str, str]]:
    """examples/README.md's reading order: each example's folder and what
    it teaches, its lines joined."""
    listed, current = [], None
    for line in (root / 'examples/README.md').read_text().split('\n'):
        item = ITEM.match(line)
        if item:
            current = [item.group(1), item.group(3)]
            listed.append(current)
        elif current and line.startswith('   ') and line.strip():
            current[1] += ' ' + line.strip()
        else:
            current = None
    return [(folder, text) for folder, text in listed]


def closest_examples(root: Path = ROOT) -> list[tuple[str, list[str]]]:
    """examples/README.md's feature table: each feature and the folders of
    the examples it names."""
    table = section((root / 'examples/README.md').read_text(), FEATURES_HEADING) or ''
    return [(row[0], re.findall(r'\[([^\]]+)\]\(', row[1])) for row in first_table(table)]


def feature_table(features: list[tuple[str, list[str]]], address: str) -> str:
    lines = ['| Feature | Example |', '|---|---|']
    for feature, folders in features:
        links = ', '.join(f'[{folder}]({address.format(folder=folder)})' for folder in folders)
        lines.append(f'| {feature} | {links} |')
    return '\n'.join(lines)


def agents_tail(root: Path = ROOT) -> str:
    return f"""{FEATURES_HEADING}

Each is a complete project in Jadpo's repository, commented throughout,
with tests that pass. Read the closest one's README and the files it
names before you write the feature yourself.

{feature_table(closest_examples(root), GITHUB_TREE)}

## What isn't built yet

Every area of the language is built but for what this table lists. A
feature not built yet is `toolchain.not_built_yet` where it is written.
Each area links to its "What is built" topic, which says more.

{built_table(not_built(root))}
"""


def llms_section(root: Path = ROOT) -> str:
    listed = '\n'.join(f'- [{folder}]({RAW_README.format(folder=folder)}): {text}'
                       for folder, text in examples(root))
    return f"""{LLMS_HEADING}

Every teaching project in examples/, in reading order. Read each README, then the source files and tests it lists.

{listed}

The closest example for each feature:

{feature_table(closest_examples(root), RAW_README)}
"""


def written(root: Path = ROOT) -> dict[Path, str]:
    """Each file this writes, as it should read."""
    template = (root / TEMPLATE.relative_to(ROOT)).read_text()
    head = template.split(f'\n{FEATURES_HEADING}\n')[0].rstrip('\n')
    llms = (root / LLMS.relative_to(ROOT)).read_text()
    start = llms.index(f'\n{LLMS_HEADING}\n')
    rest = llms[start + 1:]
    end = rest.find('\n## ')
    after = '' if end < 0 else rest[end:]
    files = {
        root / TEMPLATE.relative_to(ROOT): f'{head}\n\n{agents_tail(root)}',
        root / LLMS.relative_to(ROOT): f'{llms[:start + 1]}{llms_section(root)}{after}',
    }
    primer = root / PRIMER.relative_to(ROOT)
    files[primer] = with_section(primer.read_text(), NOT_BUILT_HEADING, primer_not_built(root))
    setup = root / LEARN_SETUP.relative_to(ROOT)
    files[setup] = with_section(setup.read_text(), INSTALL_HEADING, learn_install(root))
    return files


def problems(root: Path = ROOT) -> list[str]:
    found = []
    listed = {folder for folder, _ in examples(root)}
    teaching = sorted(p.name for p in (root / 'examples').iterdir() if (p / 'app.jadpo').is_file())
    for folder in teaching:
        if folder not in listed:
            found.append(f"examples/{folder} is in no line of examples/README.md's reading order")
    for feature, folders in closest_examples(root):
        for folder in folders:
            if folder not in teaching:
                found.append(f'the feature table names examples/{folder} for "{feature}", which isn\'t a teaching example')
    found += install_claim_problems(root)
    # A folder holding only the examples and the areas, as a test builds.
    if all((root / path.relative_to(ROOT)).is_file() for path in (TEMPLATE, LLMS, PRIMER, LEARN_SETUP)):
        found += primer_pick_problems(root) + install_problems(root)
        for path, text in written(root).items():
            if path.read_text() != text:
                found.append(f'{path.relative_to(root)} is out of date: run python3 tools/docs/agent_guidance.py')
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='fail if a file is stale, writing nothing')
    args = parser.parse_args()
    if not args.check:
        for path, text in written().items():
            path.write_text(text)
    found = problems()
    for problem in found:
        print(problem, file=sys.stderr)
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
