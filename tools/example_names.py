#!/usr/bin/env python3
"""The example-name ban (docs/compiler-runtime.md §2.7).

The compiler must never special-case an example application, so the names
declared in the language pages' code blocks and in `examples/` may not
appear in the compiler's source. This script extracts every declared name
into tools/example-identifiers.txt, minus the generic words listed in
tools/example-identifiers-allowed.txt, and checks that no listed name appears,
in any case style, as an identifier or string in crates/*/src/. Comments are
not checked, and tests may use the names freely.

  python3 tools/example_names.py           # rewrite tools/example-identifiers.txt
  python3 tools/example_names.py --check   # fail if the list is stale or a name is used
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIST = ROOT / 'tools/example-identifiers.txt'
ALLOWED = ROOT / 'tools/example-identifiers-allowed.txt'
SOURCES = [ROOT / 'docs/language', ROOT / 'examples']
FENCE = re.compile(r'^```(\S*)(.*)$')
IDENT = r'[A-Za-z_][A-Za-z0-9_]*'

# Keywords that introduce a declaration; the next identifier is the declared name.
DECLARATIONS = {
    'action', 'api', 'application', 'authentication', 'cache', 'config', 'connection', 'constraint',
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
DOMAIN_PATH = re.compile(r'(?:^|/)domains/([a-z_][a-z0-9_]*)/')


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
        for path in sorted(root.rglob('*.md')):
            for attrs, lines in code_blocks(path):
                names |= declared_names(attrs, lines)
        for path in sorted(root.rglob('*.jadpo')):
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
    while True:
        match = RUST_TOKEN.search(text, pos)
        if not match:
            return
        line = text.count('\n', 0, match.start()) + 1
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


def scan(banned: list[str]) -> list[str]:
    needles = [(name, tuple(name.split('_'))) for name in banned]
    found = []
    for path in sorted(ROOT.glob('crates/*/src/**/*.rs')):
        for line, kind, text in rust_tokens(path.read_text()):
            pieces = re.findall(IDENT, text) if kind == 'string' else [text]
            for piece in pieces:
                key = words(piece)
                for name, needle in needles:
                    if contains(key, needle):
                        found.append(f'{path.relative_to(ROOT)}:{line}: {kind} `{piece}` uses the example name `{name}`')
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='fail if the list is stale or a name is used')
    args = parser.parse_args()
    banned = banned_list()
    text = ''.join(f'{name}\n' for name in banned)
    if not args.check:
        LIST.write_text(text)
        print(f'wrote {len(banned)} names to {LIST.relative_to(ROOT)}')
        return 0
    failed = False
    if not LIST.exists() or LIST.read_text() != text:
        print(f'{LIST.relative_to(ROOT)} is stale: run python3 tools/example_names.py and commit it')
        failed = True
    for problem in scan(banned):
        print(problem)
        failed = True
    if not failed:
        print(f'{len(banned)} example names, none used in crates/*/src')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
