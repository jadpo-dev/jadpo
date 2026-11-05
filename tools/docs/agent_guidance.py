#!/usr/bin/env python3
"""Write what a coding agent reads first, from the docs it summarises.

Four files carry it (roadmap chunks A6 and A12):

- the AGENTS.md that `jadpo new` writes, from
  crates/jadpo-cli/scaffold/AGENTS.md.tmpl: its working method is written
  by hand, and everything from its "The closest example for each feature"
  heading on is generated here, its examples linked at `{release}`, which
  `jadpo new` fills with its own version;
- llms.txt, whose "Reference" section is generated here;
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
  in Cargo.toml;
- docs/reference.toml: the reference's groups, which llms.txt lists.

A status in those tables starts Built, Partly built or Not built, and a row
that says Built says nothing of its own isn't built: what it lacks makes it
Partly built, or has a row of its own.

What an agent has to read before its first lookup, the primer and llms.txt,
stays inside READ_ONCE_TOKENS, and nothing that tells an agent what to read
sends it to the whole of index.json (docs/documentation.md, "How an agent uses
it"). No other line of the primer or the Learn path says how to get `jadpo` by
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
REFERENCE = ROOT / 'docs/reference.toml'

TOPICS = 'https://jadpo.dev/docs/'
# `{release}` is left for `jadpo new`, which writes its own version there,
# so a project's examples are the ones its release was tested with.
GITHUB_TREE = 'https://github.com/jadpo-dev/jadpo/tree/v{{release}}/examples/{folder}'

FEATURES_HEADING = '## The closest example for each feature'
REFERENCE_HEADING = '## Reference'
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

# What an agent reads once before its first lookup, the primer as the bundle
# gives it and llms.txt, in the bundle's tokens: a quarter of the characters.
READ_ONCE_TOKENS = 9_000
# Every place that tells an agent what to read, which never sends it to the
# whole of index.json. Of the contract, only what it says an agent does.
AGENT_GUIDANCE = ('llms.txt', 'README.md', 'AGENTS.md', 'crates/jadpo-cli/scaffold/AGENTS.md.tmpl',
                  'docs/agent-skill/SKILL.md', 'docs/primer.md', 'docs/writing.md', 'docs/language/README.md',
                  'benchmarks/agent-changes/agents/jadpo-AGENTS.md')
AGENT_GUIDANCE_GLOBS = ('docs/learn/*.md', 'docs/guides/*.md', 'docs/language/[0-9][0-9]-*.md')
CONTRACT = 'docs/documentation.md'
CONTRACT_HEADINGS = ('## The agent skill', '## How an agent uses it')
WHOLE_INDEX = 'index.json'
# The agent skills' own small index isn't the bundle's.
WHOLE_INDEX_MENTION = re.compile(r'(?<!agent-skills/)index\.json')
# Where a sentence ends: not the dot in "index.json". A blank line or a new
# list item ends one too, and a row of a table.
SENTENCE_END = re.compile(r'(?<=[.!?])\s+|\n\s*\n|\n(?=\s*(?:[-*|]|\d+\.)\s)')
NOT_WHOLE = re.compile(r"\b(?:never|not|don't)\b.*\b(?:whole|all of)\b", re.IGNORECASE | re.DOTALL)

# The words a status starts with, and what a row that says Built can't also
# say. An error's name (`toolchain.not_built_yet`) and a link's address aren't
# words, and "not deployed yet" is about what runs where, not what is built.
STATUS_WORDS = ('Built', 'Partly built', 'Not built')
SAYS_UNBUILT = re.compile(r"(?:\bnot|n't) (?:\w+ )?built\b|(?:\bnot|n't) yet\b"
                          r"|\bcomes? (?:with|later)\b|\buntil\b.*\bbuilt\b", re.IGNORECASE)

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


def tokens(text: str) -> int:
    """A page's length as the bundle's index gives it: a quarter of its
    characters."""
    return (len(text) + 3) // 4


def read_once_problems(primer: str, llms: str) -> list[str]:
    """What an agent reads once, over its budget."""
    total = tokens(primer) + tokens(llms)
    if total <= READ_ONCE_TOKENS:
        return []
    return [f'the primer and llms.txt, which an agent has to read once, are {total:,} tokens, over {READ_ONCE_TOKENS:,} '
            f'(the primer {tokens(primer):,}, llms.txt {tokens(llms):,}): shorten one, since an agent pays this '
            'before its first lookup']


def whole_index_in(text: str, name: str, first_line: int = 1) -> list[str]:
    """Each sentence of `text` naming index.json without telling the reader
    not to read it whole."""
    found = []
    ends = [(m.start(), m.end()) for m in SENTENCE_END.finditer(text)]
    for mention in WHOLE_INDEX_MENTION.finditer(text):
        start = max((end for _, end in ends if end <= mention.start()), default=0)
        stop = min((begin for begin, _ in ends if begin >= mention.end()), default=len(text))
        if not NOT_WHOLE.search(text[start:stop]):
            line = first_line + text.count('\n', 0, mention.start())
            found.append(f'{name}:{line}: sends an agent to {WHOLE_INDEX}, which is far too long to read: send it to '
                         "llms.txt's groups, or say never to read the file whole")
    return found


def whole_index_problems(root: Path = ROOT) -> list[str]:
    """What tells an agent to read the whole of index.json."""
    paths = [root / name for name in AGENT_GUIDANCE]
    for pattern in AGENT_GUIDANCE_GLOBS:
        paths += sorted(root.glob(pattern))
    found = []
    for path in paths:
        if path.is_file():
            found += whole_index_in(path.read_text(), str(path.relative_to(root)))
    contract = root / CONTRACT
    text = contract.read_text() if contract.is_file() else ''
    for heading in CONTRACT_HEADINGS:
        body = section(text, heading)
        if body is not None:
            line = 1 + text.count('\n', 0, text.index(f'\n{heading}\n') + len(heading) + 2)
            found += whole_index_in(body, CONTRACT, line)
    return found


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


def status_word_problems(root: Path = ROOT) -> list[str]:
    """A "What is built" row whose status overstates what it goes on to say."""
    found = []
    for path in sorted((root / 'docs/language').glob('[0-9][0-9]-*.md')):
        for feature, status in area_rows(path):
            where = f'docs/language/{path.name}, "What is built", the row "{feature}"'
            if not status.startswith(STATUS_WORDS):
                found.append(f'{where} says "{status[:40]}": a status starts Built, Partly built or Not built')
            elif status.startswith('Built') and (said := SAYS_UNBUILT.search(plain(status))):
                found.append(f'{where} says Built and also "{said.group()}": make it "Partly built: ...", '
                             'or give what it lacks a row of its own')
    return found


# What only someone building Jadpo knows, which a reference page never says.
# Each is looked for outside code: `chat.completion.chunk` is a provider's
# word, and a line of an example is the example's. "chunk" is held to no
# meaning, so a page says "piece" for a piece of text; "chunked" is another
# word, as in HTTP's chunked encoding.
BUILDER_WORDS = re.compile(r'\b(?:chunks?|roadmap|Richard)\b|\bdecided\s+(?:on\s+)?20\d\d\b', re.IGNORECASE)
ROADMAP = 'docs/roadmap.md'
ROADMAP_ID = re.compile(r'\*\*([A-Z0-9][A-Za-z0-9.]*\d[A-Za-z0-9.]*)\s')
# A roadmap's id that is also the name of something a page means: products,
# paper sizes, the numbers `jadpo suggest` gives its findings.
NOT_CHUNK_IDS = {'A0', 'A1', 'A4', 'A10', 'B2', 'B10', 'D1', 'S1', 'S2', 'S3'}


def chunk_ids(root: Path) -> set[str]:
    """The ids docs/roadmap.md gives its chunks. A number with no letter
    (8.3, 10.1) is read as the number it looks like."""
    path = root / ROADMAP
    found = set(ROADMAP_ID.findall(path.read_text())) if path.is_file() else set()
    return {name for name in found if re.search('[A-Za-z]', name)} - NOT_CHUNK_IDS


def outside_code(text: str) -> str:
    """The text with code and HTML comments blanked, its lines kept."""
    def blank(match: re.Match) -> str:
        return re.sub(r'[^\n]', ' ', match.group())
    lines, fenced = [], False
    for line in text.split('\n'):
        if line.lstrip(' >').startswith('```'):
            fenced = not fenced
            lines.append('')
        else:
            lines.append('' if fenced else line)
    return re.sub(r'<!--.*?-->|`[^`]*`', blank, '\n'.join(lines), flags=re.DOTALL)


def builder_history_problems(root: Path = ROOT) -> list[str]:
    """A reference page that says what only someone building Jadpo knows: a
    chunk, a chunk's id, the roadmap, or who decided something and when."""
    ids = chunk_ids(root)
    named = re.compile(r'(?<![\w.-])(?:' + '|'.join(map(re.escape, sorted(ids, key=len, reverse=True))) + r')(?![\w-])') if ids else None
    found = []
    for path in sorted((root / 'docs/language').glob('*.md')):
        text = outside_code(path.read_text())
        hits = list(BUILDER_WORDS.finditer(text)) + (list(named.finditer(text)) if named else [])
        for hit in sorted(hits, key=lambda hit: hit.start()):
            line = 1 + text.count('\n', 0, hit.start())
            said = ' '.join(hit.group().split())
            found.append(f'{path.relative_to(root)}:{line}: says "{said}", which only someone building Jadpo knows: '
                         'say what is true now (docs/writing.md, "How a reference topic reads")')
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


def first_topic(node: dict, topics: set[str]) -> str | None:
    """The topic a node of the manifest opens on: its own, or its first."""
    if node.get('id') in topics:
        return node['id']
    listed = [*node.get('topics', []), *node.get('details', [])]
    if listed:
        return listed[0]
    for child in [*node.get('page', []), *node.get('section', [])]:
        found = first_topic(child, topics)
        if found:
            return found
    return None


def reference_groups(root: Path = ROOT) -> list[tuple[str, str, str]]:
    """Each group of the reference with a Markdown page: its title, the page's
    address and its summary. A group the site writes itself has none, and a
    group with no ID opens on its first topic."""
    topics = set()
    for path in sorted((root / 'docs/language').glob('[0-9][0-9]-*.md')):
        topics |= {m.group(1) for m in map(TOPIC.match, path.read_text().split('\n')) if m}
    groups = []
    for group in tomllib.loads((root / REFERENCE.relative_to(ROOT)).read_text()).get('group', []):
        topic = None if group.get('href') else group.get('id') or first_topic(group, topics)
        if topic:
            groups.append((group['title'], f'{TOPICS}{topic}.md', group.get('summary', '')))
    return groups


def llms_reference(root: Path = ROOT) -> str:
    listed = '\n'.join(f'- [{title}]({address}): {summary}' for title, address, summary in reference_groups(root))
    return f"""{REFERENCE_HEADING}

The reference is on the site, one page per topic, in these groups. Open the page of the group a task belongs to, which lists what is in the group. Then fetch only the `.md` pages the task needs.

{listed}

Each compiler error has its own page, `{TOPICS}errors/<rule.id>.md`, such as {TOPICS}errors/failure.kind_required.md, and the error's `help:` line prints its address. Each standard type has one too, such as {TOPICS}types/type-money.md, and `jadpo help <Type>` prints the same page.
"""


def written(root: Path = ROOT) -> dict[Path, str]:
    """Each file this writes, as it should read."""
    template = (root / TEMPLATE.relative_to(ROOT)).read_text()
    head = template.split(f'\n{FEATURES_HEADING}\n')[0].rstrip('\n')
    llms = (root / LLMS.relative_to(ROOT)).read_text()
    files = {
        root / TEMPLATE.relative_to(ROOT): f'{head}\n\n{agents_tail(root)}',
        root / LLMS.relative_to(ROOT): with_section(llms, REFERENCE_HEADING, llms_reference(root)),
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
    found += (install_claim_problems(root) + status_word_problems(root) + whole_index_problems(root)
              + builder_history_problems(root))
    # A folder holding only the examples and the areas, as a test builds.
    if all((root / path.relative_to(ROOT)).is_file() for path in (TEMPLATE, LLMS, PRIMER, LEARN_SETUP, REFERENCE)):
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
