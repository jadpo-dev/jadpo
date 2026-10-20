#!/usr/bin/env python3
"""The example-name ban (docs/compiler-runtime.md §2.7).

The compiler must never special-case an example application, so the names
declared in the language pages' code blocks and in `examples/` may not
appear in the compiler's source. This script extracts every declared name,
each time it runs, minus the generic words listed in the committed
tools/example-identifiers-allowed.txt, and checks that no such name appears,
in any case style, as an identifier or string in crates/*/src/ or in the
standard library's stubs (crates/*/std/). Comments are not checked, and tests
may use the names freely, as may a file of an authority's data generated
into the source, whose first line is `// @generated` (a subdivision called
Baker Island special-cases no example).

An example entity whose name is an allowed word (`Order`, `Session`) is still
a sign of special-casing when it stands as a string, so those names (less
the language's own, tools/example-type-names-allowed.txt) may not appear
exactly, as a name, in a
string literal without spaces (`"Order"`, `"Order.get"`). Inline
`#[cfg(test)] mod` blocks count as tests; the attribute mentioned in a
comment does not.

  python3 tools/example_names.py --check   # fail if a name is used
  python3 tools/example_names.py --list    # print the names it bans

The lists are worked out from the pages and examples each run, never
committed (chunk 5.8): only the allow-lists are.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = ROOT / 'tools/example-identifiers-allowed.txt'
TYPE_ALLOWED = ROOT / 'tools/example-type-names-allowed.txt'
SOURCES = [ROOT / 'docs/language', ROOT / 'examples']


def source_files(root: Path, pattern: str) -> list[Path]:
    """Committed sources only: an example's `build/` output is generated and
    ignored, and so is an example's installed copy of one of this
    repository's own packages (`packages/<name>/`, such as Resend), whose
    names are that package's, which the root `packages/` keeps and the
    compiler may name, not the example's."""
    return [p for p in sorted(root.rglob(pattern))
            if 'build' not in p.relative_to(root).parts and not first_party_copy(root, p)]


def first_party_copy(root: Path, path: Path) -> bool:
    """Whether `path` is in an example's `packages/<name>/` for a package
    this repository holds in its own `packages/<name>/`."""
    parts = path.relative_to(root).parts
    return (root == ROOT / 'examples' and len(parts) > 3 and parts[1] == 'packages'
            and (ROOT / 'packages' / parts[2] / 'package.jadpo').is_file())
FENCE = re.compile(r'^```(\S*)(.*)$')
IDENT = r'[A-Za-z_][A-Za-z0-9_]*'

# Keywords that introduce a declaration; the next identifier is the declared name.
DECLARATIONS = {
    'action', 'api', 'application', 'authentication', 'cache', 'config', 'connection', 'constant',
    'constraint',
    'entity', 'enum', 'event', 'failure', 'fixture', 'flag', 'function', 'input', 'inverse', 'library',
    'operation', 'output', 'package', 'principal', 'provider', 'query', 'references', 'resource',
    'routes', 'schedule', 'seed', 'service', 'site', 'subscriber', 'type', 'value', 'var',
}
DECLARATION = re.compile(rf'^\s*(?:public\s+)?({"|".join(sorted(DECLARATIONS))})\s+({IDENT})')
# Declarations whose body lists fields (`name: Type`) or, for enums, variants.
FIELD_OWNER = re.compile(rf'^\s*(?:public\s+)?(?:entity|input|output|event|failure|value|config|principal)\s+{IDENT}\s*\{{|'
                         rf'^\s*(?:public\s+)?type\s+{IDENT}\s*=\s*(?:Object|Input|Output|Event|Failure|EmailMessage)\b.*\{{')
ENUM_OWNER = re.compile(rf'^\s*(?:public\s+)?(?:enum\s+{IDENT}|type\s+{IDENT}\s*=\s*Enum)\s*\{{')
FIELD = re.compile(rf'^\s*({IDENT})\??\s*:\s*[A-Z]')
VARIANT = re.compile(rf'^\s*({IDENT})\s*(?::?\s*\{{)?\s*,?\s*(?://.*)?$')
CALLABLE = re.compile(rf'^\s*(?:public\s+)?(?:function|action|query|fixture|seed)\s+{IDENT}\s*\(([^)]*)\)')
PARAMETER = re.compile(rf'({IDENT})\??\s*:')
# Example types and entities, whose names may not stand alone in a string.
TYPE_NAME = re.compile(rf'^\s*(?:public\s+)?(?:entity|type)\s+([A-Z][A-Za-z0-9_]*)', re.MULTILINE)
DOMAIN_PATH = re.compile(r'(?:^|/)domains/([a-z_][a-z0-9_]*)/')
RECORD_FILE = re.compile(r'\bfile=\S*(?:\.jadpo_record|jadpo\.lock)\b')


def code_blocks(path: Path):
    """Yield (attributes, lines) for every ```jadpo block in a Markdown file."""
    lines = path.read_text().splitlines()
    i = 0
    while i < len(lines):
        match = FENCE.match(lines[i])
        if match:
            j = i + 1
            while j < len(lines) and not lines[j].startswith('```'):
                j += 1
            if match.group(1) == 'jadpo':
                yield match.group(2), lines[i + 1:j]
            i = j
        i += 1


def declared_names(attrs: str, lines: list[str]) -> set[str]:
    names = set()
    for match in DOMAIN_PATH.finditer(attrs):
        names.add(match.group(1))
    depth = 0
    owner = None  # ('fields' | 'variants', depth at which its members sit)
    for raw in lines:
        line = raw.split('//', 1)[0]
        match = DECLARATION.match(line)
        if match:
            names.add(match.group(2))
        match = CALLABLE.match(line)
        if match:
            names.update(PARAMETER.findall(match.group(1)))
        if owner and depth == owner[1]:
            member = (FIELD if owner[0] == 'fields' else VARIANT).match(line)
            if member:
                names.add(member.group(1))
        if owner is None:
            if FIELD_OWNER.match(line):
                owner = ('fields', depth + 1)
            elif ENUM_OWNER.match(line):
                owner = ('variants', depth + 1)
        depth += line.count('{') - line.count('}')
        if owner and depth < owner[1]:
            owner = None
    return names


def extract() -> set[str]:
    names = set()
    for root in SOURCES:
        for path in source_files(root, '*.md'):
            for attrs, lines in code_blocks(path):
                # A generated record's words (`authentication none`) are
                # the record format's, not declarations (area 18).
                if RECORD_FILE.search(attrs):
                    continue
                names |= declared_names(attrs, lines)
        for path in source_files(root, '*.jadpo'):
            names |= declared_names(str(path.relative_to(root)), path.read_text().splitlines())
            names |= {m.group(1) for m in DOMAIN_PATH.finditer(str(path.relative_to(root)))}
    return names


def words(name: str) -> tuple[str, ...]:
    """Split a name in any case style into lower-case words."""
    spaced = re.sub(r'([a-z0-9])([A-Z])', r'\1 \2', name)
    spaced = re.sub(r'([A-Z]+)([A-Z][a-z])', r'\1 \2', spaced)
    return tuple(w.lower() for w in re.split(r'[\s_\-]+', spaced) if w)


def load_allowed() -> set[tuple[str, ...]]:
    allowed = set()
    for line in ALLOWED.read_text().splitlines():
        line = line.split('#', 1)[0].strip()
        if line:
            allowed.add(words(line))
    return allowed


def banned_list() -> list[str]:
    allowed = load_allowed()
    keep = {}
    for name in extract():
        key = words(name)
        if len(''.join(key)) < 3 or key in allowed:
            continue
        keep.setdefault(key, name)
    return sorted({'_'.join(key) for key in keep})


def banned_type_names() -> list[str]:
    """Example type and entity names left out of the main list because their words are allowed."""
    allowed = load_allowed()
    types = set()
    for root in SOURCES:
        for path in source_files(root, '*.md'):
            for _, lines in code_blocks(path):
                types.update(TYPE_NAME.findall('\n'.join(lines)))
        for path in source_files(root, '*.jadpo'):
            types.update(TYPE_NAME.findall(path.read_text()))
    language = {line.split('#', 1)[0].strip() for line in TYPE_ALLOWED.read_text().splitlines()}
    return sorted(name for name in types if words(name) in allowed and name not in language)


# --- scanning Rust source --------------------------------------------------

RUST_TOKEN = re.compile(r'''
    (?P<line_comment>//[^\n]*)
  | (?P<block_comment>/\*)
  | (?P<raw_string>b?r(?P<hashes>\#*)")
  | (?P<string>b?"(?:\\.|[^"\\])*")
  | (?P<char>b?'(?:\\.|[^'\\])')
  | (?P<lifetime>'[A-Za-z_][A-Za-z0-9_]*)
  | (?P<ident>[A-Za-z_][A-Za-z0-9_]*)
''', re.VERBOSE | re.DOTALL)


def rust_tokens(text: str):
    """Yield (line, kind, text) for identifiers and string contents, skipping comments."""
    pos = 0
    # Lines are counted from the previous token, not from the start.
    line, counted = 1, 0
    while True:
        match = RUST_TOKEN.search(text, pos)
        if not match:
            return
        line += text.count('\n', counted, match.start())
        counted = match.start()
        kind = match.lastgroup
        if kind == 'block_comment':
            depth, i = 1, match.end()
            while depth and i < len(text):
                if text.startswith('/*', i):
                    depth, i = depth + 1, i + 2
                elif text.startswith('*/', i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            pos = i
            continue
        if kind in ('raw_string', 'hashes'):
            closing = '"' + match.group('hashes')
            end = text.find(closing, match.end())
            end = len(text) if end < 0 else end
            yield line, 'string', text[match.end():end]
            pos = end + len(closing)
            continue
        if kind == 'string':
            yield line, 'string', match.group(0)
        elif kind == 'ident':
            yield line, 'identifier', match.group(0)
        pos = match.end()


def contains(haystack: tuple[str, ...], needle: tuple[str, ...]) -> bool:
    n = len(needle)
    return any(haystack[i:i + n] == needle for i in range(len(haystack) - n + 1))


TEST_MODULE = re.compile(r'^[ \t]*#\[cfg\(test\)\]\s*(?:pub(?:\(crate\))?\s+)?mod\s+\w+\s*\{', re.MULTILINE)


def without_test_modules(text: str) -> str:
    """`text` with each inline `#[cfg(test)] mod ... { ... }` blanked out,
    lines kept, so tests may use example names. Only the attribute on a
    module counts: a comment that mentions it exempts nothing."""
    out = text
    for match in reversed(list(TEST_MODULE.finditer(text))):
        depth, i = 1, match.end()
        while depth and i < len(text):
            token = RUST_TOKEN.match(text, i)
            if token and token.lastgroup in ('line_comment', 'string', 'char', 'lifetime', 'ident'):
                i = token.end()
                continue
            if token and token.lastgroup == 'raw_string':
                closing = '"' + token.group('hashes')
                end = text.find(closing, token.end())
                i = len(text) if end < 0 else end + len(closing)
                continue
            if token and token.lastgroup == 'block_comment':
                end = text.find('*/', token.end())
                i = len(text) if end < 0 else end + 2
                continue
            depth += {'{': 1, '}': -1}.get(text[i], 0)
            i += 1
        blank = re.sub(r'[^\n]', ' ', text[match.start():i])
        out = out[:match.start()] + blank + out[i:]
    return out


def jadpo_tokens(text: str):
    """Yield (line, kind, text) for names and strings in Jadpo source (the
    standard library's stubs), skipping `//` comments."""
    for number, raw in enumerate(text.splitlines(), 1):
        code = raw.split('//', 1)[0]
        for match in re.finditer(r'"(?:\\.|[^"\\])*"|' + IDENT, code):
            kind = 'string' if match.group(0).startswith('"') else 'identifier'
            yield number, kind, match.group(0)


def sources_to_scan():
    """Each file the compiler is built from, with its names and strings:
    Rust source without its test modules, the standard library's Jadpo
    stubs and its names list."""
    for path in sorted(ROOT.glob('crates/*/src/**/*.rs')):
        text = path.read_text()
        if text.startswith('// @generated'):
            continue
        yield path, rust_tokens(without_test_modules(text))
    for path in sorted(ROOT.glob('crates/*/std/*.jadpo')):
        yield path, jadpo_tokens(path.read_text())
    for path in sorted(ROOT.glob('crates/*/std/*.txt')):
        text = '\n'.join(line.split('#', 1)[0] for line in path.read_text().splitlines())
        yield path, jadpo_tokens(text)


def scan(banned: list[str], type_names: list[str]) -> list[str]:
    # Each banned name, by its first word, so a token is only compared with
    # the names that could start at each of its words.
    by_first: dict[str, list[tuple[int, tuple[str, ...]]]] = {}
    for index, name in enumerate(banned):
        needle = tuple(name.split('_'))
        by_first.setdefault(needle[0], []).append((index, needle))
    exact = set(type_names)
    uses: dict[str, list[str]] = {}

    def names_used(piece: str) -> list[str]:
        """The banned names `piece` contains, in the list's order."""
        if piece not in uses:
            key = words(piece)
            hits = {index
                    for i, word in enumerate(key)
                    for index, needle in by_first.get(word, ())
                    if key[i:i + len(needle)] == needle}
            uses[piece] = [banned[index] for index in sorted(hits)]
        return uses[piece]

    found = []
    for path, tokens in sources_to_scan():
        for line, kind, text in tokens:
            if kind == 'string' and not re.search(r'\s', text):
                for piece in sorted(exact.intersection(re.findall(IDENT, text))):
                    found.append(f'{path.relative_to(ROOT)}:{line}: string {text} names the example type `{piece}`')
            pieces = re.findall(IDENT, text) if kind == 'string' else [text]
            for piece in pieces:
                for name in names_used(piece):
                    found.append(f'{path.relative_to(ROOT)}:{line}: {kind} `{piece}` uses the example name `{name}`')
    return found


def self_test() -> None:
    """The matching rules this check relies on, checked before it runs."""
    assert words('TodoTitle') == words('todo_title') == words('TODO_TITLE') == words('todo-title') == ('todo', 'title')
    assert words('HTTPServer') == ('http', 'server')
    assert contains(words('parse_todo_title'), ('todo', 'title'))
    assert not contains(words('todos'), ('todo',))
    tokens = list(rust_tokens('// Todo\n/* Todo /* nested */ */ let a = "Todo"; let b = r#"x"#; fn c<\'a>() {}'))
    assert ('string', '"Todo"') in [(kind, text) for _, kind, text in tokens]
    assert all(text != 'Todo' for _, kind, text in tokens if kind == 'identifier')
    assert ('identifier', 'c') in [(kind, text) for _, kind, text in tokens]
    assert declared_names('', ['type Pair = Object {', '    left_side: Int', '}', 'function f(amount: Int) -> Int {']) == {
        'Pair', 'left_side', 'f', 'amount'}
    assert TYPE_NAME.findall('entity Order {\n    public entity Session {\ntype Plan = Object {') == ['Order', 'Session', 'Plan']
    hidden = without_test_modules('fn a() {}\n/// See #[cfg(test)] below.\nfn b() { let x = "Todo"; }\n#[cfg(test)]\nmod tests {\n    fn c() { let s = "}"; }\n}\nfn d() {}\n')
    assert '"Todo"' in hidden and 'fn d()' in hidden and '"}"' not in hidden
    assert hidden.count('\n') == 8


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='fail if a name is used (the default)')
    parser.add_argument('--list', action='store_true', help='print the banned names and type names')
    args = parser.parse_args()
    self_test()
    banned = banned_list()
    type_names = banned_type_names()
    if args.list:
        print(''.join(f'{name}\n' for name in banned), end='')
        print(''.join(f'type {name}\n' for name in type_names), end='')
        return 0
    failed = False
    for problem in scan(banned, type_names):
        print(problem)
        failed = True
    if not failed:
        print(f'{len(banned)} example names and {len(type_names)} example type names, none used in crates/*/src')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
