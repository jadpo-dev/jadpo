#!/usr/bin/env python3
"""Checks that the VS Code extension's highlighting is in step with the lexer.

  python3 tools/checks/editor_grammar.py --check

The TextMate grammar (editors/vscode/syntaxes/jadpo.tmLanguage.json) is
written by hand, because highlighting reads one line at a time and the
compiler's lexer does not. What must not drift is the word lists, so this
reads the real ones and fails when the grammar leaves one out or names one
the lexer does not know:

- the reserved words and the contextual keywords, from the generated
  `crates/jadpo-syntax/src/generated/kinds.rs`;
- the HTTP methods the lexer starts a route path after, from `lexer.rs`;
- the declarations that exist once per project (`application`, `config`,
  ...), from the comment on `SingletonDecl` in `jadpo.ungram`.

It also checks the extension's manifest points at the grammar, and pins the
language client to one exact version. It needs no Node.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SYNTAX = ROOT / 'crates' / 'jadpo-syntax'
EXTENSION = ROOT / 'editors' / 'vscode'

# Words the grammar highlights that the lexer does not reserve yet. `use`
# arrives with imports (roadmap 3.5), which makes it a keyword; the entry
# is harmless once it is one.
EXTRA = {'use'}


def match_arms(source: str, function: str) -> set[str]:
    """The string patterns of the `match text { ... }` in one function."""
    body = re.search(rf'fn {function}\(.*?\n    \}}\n', source, re.S)
    if not body:
        raise SystemExit(f'kinds.rs has no `{function}`; the generator changed?')
    return set(re.findall(r'^\s*"([a-z_]+)" =>', body.group(0), re.M))


def lexer_words() -> dict[str, set[str]]:
    kinds = (SYNTAX / 'src' / 'generated' / 'kinds.rs').read_text()
    lexer = (SYNTAX / 'src' / 'lexer.rs').read_text()
    methods = re.search(r'HTTP_METHODS: &\[&str\] =\s*&\[(.*?)\];', lexer, re.S)
    ungram = (SYNTAX / 'jadpo.ungram').read_text()
    singleton = re.search(r'((?://[^\n]*\n)+)SingletonDecl', ungram)
    if not methods or not singleton:
        raise SystemExit('lexer.rs or jadpo.ungram changed shape; update tools/checks/editor_grammar.py')
    return {
        'keyword': match_arms(kinds, 'from_keyword'),
        'contextual keyword': match_arms(kinds, 'from_contextual_keyword'),
        'HTTP method': set(re.findall(r'"([A-Z]+)"', methods.group(1))),
        'once-per-project declaration': set(
            re.findall(r'`([a-z_]+)`', singleton.group(1))),
    }


def regexes(node) -> list[str]:
    """Every regular expression in the grammar."""
    found: list[str] = []
    if isinstance(node, dict):
        for key, value in node.items():
            if key in ('match', 'begin', 'end') and isinstance(value, str):
                found.append(value)
            else:
                found += regexes(value)
    elif isinstance(node, list):
        for value in node:
            found += regexes(value)
    return found


def words_in_groups(patterns: list[str]) -> set[str]:
    """Words written as a group of plain alternatives: `(if|else|for)`."""
    words: set[str] = set()
    for pattern in patterns:
        for group in re.findall(r'\((?:\?:)?([A-Za-z_]+(?:\|[A-Za-z_]+)*)\)', pattern):
            words.update(group.split('|'))
    return words


def check() -> list[str]:
    problems: list[str] = []
    grammar_path = EXTENSION / 'syntaxes' / 'jadpo.tmLanguage.json'
    grammar = json.loads(grammar_path.read_text())
    package = json.loads((EXTENSION / 'package.json').read_text())
    json.loads((EXTENSION / 'language-configuration.json').read_text())

    in_grammar = words_in_groups(regexes(grammar))
    expected = lexer_words()
    known = set().union(*expected.values()) | EXTRA
    for kind, words in expected.items():
        if not words:
            problems.append(f'read no {kind}s from the compiler; update tools/checks/editor_grammar.py')
        for word in sorted(words - in_grammar):
            problems.append(f'the grammar does not highlight the {kind} `{word}`')
    for word in sorted(in_grammar - known):
        problems.append(f'the grammar highlights `{word}`, which the lexer does not reserve')

    grammars = package.get('contributes', {}).get('grammars', [])
    if not any(g.get('scopeName') == grammar.get('scopeName')
               and (EXTENSION / g.get('path', '')).resolve() == grammar_path.resolve()
               for g in grammars):
        problems.append('package.json does not contribute the grammar')
    version = package.get('dependencies', {}).get('vscode-languageclient', '')
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        problems.append(f'vscode-languageclient must be pinned exactly, not `{version}`')
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--check', action='store_true',
                        help='fail when the grammar has drifted (the only mode)')
    parser.parse_args()
    problems = check()
    for problem in problems:
        print(f'editor grammar: {problem}', file=sys.stderr)
    if not problems:
        print('editor grammar: in step with the lexer')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
