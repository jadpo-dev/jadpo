# Independent diagnostics, source mapping and tooling validation

## Contract expectations recorded before test implementation

Authority: `docs/developer-tooling.md` §§3.1–3.3 and DX1;
`docs/diagnostic-presentation-contract.md` §§1–4; grammar §§2 and 15.

- Compiler-produced edits must apply to exact source spans, and repaired source
  must be checked again. Protected authentication choices remain human-owned;
  neither public access nor required authentication may be silently preferred.
- Stale revision-bound actions must not be offered as current edits.
- LSP coordinates are UTF-16, while compiler ranges are UTF-8 bytes. Navigation
  and rename must agree on semantic symbols across unsaved documents and files,
  without changing string/comment lookalikes.
- Formatting must be idempotent and preserve significant tokens, literal text,
  comments and the source's existing semantic meaning.
- Malformed source must terminate with valid source ranges; bounded generated
  cases and a subprocess deadline provide limited resilience evidence, not an
  unrestricted fuzz campaign or an invented resource-limit contract.

The work deliberately covers representative interactions and repair paths. It
does not claim a new expected contract for every diagnostic in the catalogue.

## Findings and regression coverage

### Semantic rename could capture an existing local (fixed)

The compiler-backed rename contract in `docs/developer-tooling.md` §1 and DX1
requires edits to follow semantic symbols and lexical scopes. This clean source
demonstrated an unsafe edit:

```jadpo
type Label = Text {}
function choose(input: Label) -> Label {
  var other = Label("fixed")
  return input
}
```

Requesting `textDocument/rename` at `input` with `newName: "other"` returned edits
renaming the parameter and its reference despite the existing `other` binding.
The pre-fix black-box protocol test failed its intended no-edit assertion.

`LanguageIndex::local_rename_conflicts` now detects same-name local definitions
whose lexical scopes overlap in the same source; LSP rejects such a rename.
Tests retain the direct reproduction and a nested-block capture case. Positive
controls prove that the same target name in another function or sibling branch
does not block rename: exactly the selected declaration/reference change, and
the edited unsaved source rechecks without diagnostics. A rename to the existing
name is not treated as a collision. The guard is conservative for overlapping
scopes; this is not a full proof of capture avoidance for every symbol category.

### Source positions, revision binding and repairs

`validation_tooling_protocol.rs` starts the actual CLI LSP process and sends
Content-Length-framed JSON-RPC. It checks definition navigation across files
(including a percent-encoded space in a URI), unsaved source, and local rename
after an emoji and accented character on the same line. Applying the UTF-16
edits changes only the parameter and semantic use, leaving a matching string
literal intact. A missing-colon Quick Fix after same-line Unicode has the exact
UTF-16 insertion range. A genuine earlier diagnostic revision is rejected after
`didChange`, while the new revision still yields a repair.

`validation_tooling_contract.rs` applies two compiler-issued colon edits to the
original UTF-8 source and checks the repaired program through analysis. It also
applies both authentication-value alternatives separately, proving that each
remains human-owned and unpreferred and that the alternatives produce distinct
public/required-auth settings without changing the route's action, output or
path. The tests do not choose a security policy for the user.

Formatter cases check idempotence, valid parsing, and token-kind/text preservation
for literals, escapes, comments, subtraction and modules under LF and CRLF.
Line-comment comparison normalizes the CR belonging to CRLF; it does not discard
comment contents or normalize literal contents.

### Deterministic malformed-input campaign

`validation_fuzz_bounded.rs` generates 384 deterministic fragment combinations
and deletes/replaces each significant token of a representative module with four
alternatives. Every case is parsed twice to check determinism. Token and primary
diagnostic ranges must be valid UTF-8 boundaries within the exact source, and
EOF must be at the source's byte length. A separate test process has a ten-second
deadline and is killed on timeout, preventing an infinite parser loop from
blocking the suite indefinitely.

This is a small fixed corpus, not coverage-guided fuzzing, arbitrary-depth stress
testing, or a specified compiler resource limit. It found no new parser defect.

### Diagnostic evidence metadata audit (root-owned follow-up)

The previous build-time discovery treated any diagnostic-looking substring in
a broad test region containing `assert` as evidence. Comments and neighbouring
helper functions could therefore supply metadata credit without an executed
assertion that the diagnostic triggered. The catalogue's presence/completeness
assertion did not independently establish diagnostic reachability.

This gap was reported to the root agent, which owns the syntax-aware discovery
and missing-family/catalogue work. Even AST-bounded test references are references;
they are not execution or assertion-dataflow proof. This package makes no claim
that every documented diagnostic has a real trigger test.

## Verification and limits

- New core tooling integration tests: **3 passed**.
- New black-box CLI protocol tests: **4 passed**.
- Bounded malformed-input integration test: **1 passed**.
- Existing LSP unit tests: **18 passed**.
- Existing LanguageIndex unit tests: **2 passed**.

Raw pre-fix/fixed logs and a summary are retained in
[`build/validation/tooling-review/2026-09-30-wave3/`](../../build/validation/tooling-review/2026-09-30-wave3/)
(ignored local evidence). The initial broad index-test command encountered an
unfinished concurrent integration fixture's Rust parse error; it is explicitly
recorded as a setup failure, not counted as a product finding. The focused
`--lib language_service::tests` rerun passed.

The shared full verification command remains root-owned. These representative
cases do not establish exhaustive formatter semantics, every declaration's
rename safety, editor-host compatibility, malformed-input memory bounds, every
diagnostic's trigger coverage, or the external comprehension/fresh-user gates.
