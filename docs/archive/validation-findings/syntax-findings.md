# Lexer, parser and module validation — 2026-09-29

## Contract and scope

Expected behaviour was derived from `docs/grammar-v0.1.md`:

- §2: ASCII identifier grammar, the five supported string escapes, lexical
  rejection of malformed strings, retained trivia/spans and non-semantic
  whitespace/comments.
- §3: naming shape checked after parsing; `input`, `output`, and `value`
  remain contextual names.
- §12: conventional arithmetic/boolean precedence and grouping.
- §13: route paths and typed placeholder binding.
- §15: explicit one-file module identities, private defaults, selective
  imports, rejected cycles/conflicts, no implicit re-export, ambient-project
  compatibility and manifest metadata.

The suites use the public lexer/parser and analysis APIs rather than matching
implementation source strings. Callable/type fixture scaffolding uses canonical
`type ... = Object` and named semantic signatures where full analysis is needed.
Parser-only arithmetic cases intentionally do not claim type-checking evidence.

## Added evidence

`jadpo/crates/syntax/tests/validation_syntax_boundaries.rs` adds 18 tests:

- Lossless UTF-8 token ranges, Unicode string/comment content, exact invalid
  escape spans, all documented escapes and a small rejected-escape corpus.
- Unterminated string boundaries at EOF/newlines, all character-boundary
  truncations of four representative source snippets, and 13 incomplete
  constructs that must fail instead of being silently accepted.
- Complete-identifier keyword reservation, operator token boundaries, literal
  comment markers, route-path capture and recovery after an invalid top-level
  token.
- Trivia insertion preserving significant tokens, contextual names,
  parse-time tolerance of names requiring later semantic case diagnostics,
  module-header/import ordering, nonempty import sets and rejected public
  route exports.
- Explicit expected expression trees for arithmetic/boolean precedence,
  grouping, associativity and subtraction whitespace variants.
- Subtraction after names, contextual names, keyword members, calls and grouped
  operands, with and without intervening comments. Signed literals remain
  intact in constraints and expression starts, including decimals and the
  minimum signed 64-bit integer.

`jadpo/crates/core/tests/validation_syntax_modules.rs` adds 11 tests:

- Explicit logical module identities despite unrelated directories; exact
  manifest exports/imports; deterministic results under source permutation.
- Private/unknown exports, missing imports and exact source spans after
  multibyte Unicode comments. Duplicate-import diagnostics must identify the
  second occurrence, within and across import declarations.
- Local-name conflicts, unknown module paths, duplicate module identities,
  self imports, ambient compatibility and mixed-project rejection.
- Three-node cycle rejection alongside a valid diamond dependency graph.
- Non-transitive visibility and rejected implicit re-export, plus callable
  visibility independently of type visibility.
- A missing-import repair that succeeds by adding the public import while
  leaving the provider's private declaration private.

Cargo discovers these integration tests automatically in the existing workspace
test gate; no manifest registration or skipped test attributes were added.

## Confirmed defect

Before the production fix, this valid expression failed:

```jadpo
function difference() -> Int { return 3-2 }
```

`3 - 2` parsed, whereas `3-2` emitted `SYN_EXPECTED_STATEMENT` on `-2`.
The lexer absorbed the minus into a signed numeric token after the first
operand. This violates non-semantic whitespace (§2) combined with subtraction
(§12). The regression includes `3 -2`, `3- 2`, `3 - -2` and `3--2`, and checks
the resulting tree, not just the absence of diagnostics.

The parent delegated the repair back to this work package. The lexer now emits
a subtraction token after an operand while retaining signed numeric tokens at
value starts, including declaration settings and constraints. Trivia does not
change this classification. Keyword-named qualified members are handled too.
Numeric scanning itself is unchanged; exponent syntax is not currently part of
the documented numeric literal grammar or supported scanner.

The expanded mutation campaign found a recovering-parser span defect in a
malformed nested call: after `parse_arguments` failed to find its closing
parenthesis, the caller substituted a default end offset of zero and produced
an inverted expression/diagnostic range. Incomplete invocation, object, and
construction expressions now return a bounded `Missing` range based on the
last consumed token instead of fabricating an AST node ending at byte zero.
The mutated compile-fail fixture remains part of the deterministic corpus
campaign.

## Verification

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-syntax
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_syntax_modules
```

Initial isolated result: 15 syntax tests passed, the subtraction regression
failed, and all 11 module tests passed. After the fix and two additional
regressions, all 36 existing syntax unit tests and all 18 new syntax integration
tests pass. All 11 module integration tests also pass on the post-fix tree.
No expected-failure exemption was added.

## Limits and remaining work

- The deterministic property test now applies at least 3,000 token deletions,
  structural-delimiter replacements, and multibyte unexpected-character
  replacements sampled across both compile-pass and compile-fail corpora. It
  checks deterministic parsing, token/diagnostic byte ranges, and EOF spans.
  It remains a bounded deterministic campaign, not sustained coverage-guided
  fuzzing.
  Deep nesting, very large graphs and resource-exhaustion limits need explicit
  budgets and separate process-level tests.
- The Rust `&str` lexer API cannot represent invalid UTF-8; file-loader/CLI
  invalid-byte behaviour needs its own test and diagnostic-contract decision.
- These tests exercise representative recovery, not every grammar production's
  synchronization path, nor every parser diagnostic's complete message/repair.
- The successful explicit-import repair demonstrates one repair boundary;
  automatic-edit correctness across the diagnostic catalogue remains separate.
- No implementation-code mutation campaign was run in this package. A separate
  reviewer can mutate keyword boundary handling, precedence, span byte counts
  and module visibility checks to challenge these assertions.
- Alias/package/relative-import features remain unsupported. These tests do
  not propose semantics for them or settle canonical-data-model migration.
