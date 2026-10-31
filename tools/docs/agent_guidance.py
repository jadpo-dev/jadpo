#!/usr/bin/env python3
"""Write what a coding agent reads first, from the docs it summarises.

Two files carry it (roadmap chunk A6):

- the AGENTS.md that `jadpo new` writes, from
  crates/jadpo-cli/scaffold/AGENTS.md.tmpl: its working method is written
  by hand, and everything from its "The closest example for each feature"
  heading on is generated here;
- llms.txt, whose "Complete application examples" section is generated
  here.

What they generate from, so neither can drift from what it summarises:

- each area page's "What is built" topic: the rows of its first table
  whose status isn't "Built", one line per area, linked to that topic in
  the docs bundle (docs/documentation.md, "The bundle");
- examples/README.md: its numbered reading order, every teaching example
  with what it teaches, and its "The closest example for each feature"
  table.

Every teaching example (a folder of examples/ holding app.jadpo) must be
in the reading order, and every example the feature table names must
exist.

  python3 tools/docs/agent_guidance.py           # write both files
  python3 tools/docs/agent_guidance.py --check   # fail if either is stale
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / 'crates/jadpo-cli/scaffold/AGENTS.md.tmpl'
LLMS = ROOT / 'llms.txt'

TOPICS = 'https://jadpo.dev/docs/topics/'
GITHUB_TREE = 'https://github.com/jadpo-dev/jadpo/tree/main/examples/{folder}'
RAW_README = 'https://raw.githubusercontent.com/jadpo-dev/jadpo/main/examples/{folder}/README.md'

FEATURES_HEADING = '## The closest example for each feature'
LLMS_HEADING = '## Complete application examples'

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
    return {
        root / TEMPLATE.relative_to(ROOT): f'{head}\n\n{agents_tail(root)}',
        root / LLMS.relative_to(ROOT): f'{llms[:start + 1]}{llms_section(root)}{after}',
    }


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
    if (root / TEMPLATE.relative_to(ROOT)).is_file() and (root / LLMS.relative_to(ROOT)).is_file():
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
