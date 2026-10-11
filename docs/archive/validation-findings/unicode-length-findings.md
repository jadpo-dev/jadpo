# Unicode length parity

## Expected contract

`docs/runtime-target-v0.1.md`, “Runtime boundaries”, requires HTTP validation
to apply the same length constraints as the compiler. `docs/type-system.md`
§4.3 requires the same constructor to validate constant literals at compile
time and dynamic inputs at runtime. Existing Rust literal and configuration
validation count Unicode scalar values with `chars().count()`.

The independent cases therefore expect a supplementary character such as
`😀` to count as one value; `e` plus a combining acute accent counts as two.
These tests assert parity with the established scalar rule; they do not define
grapheme counting or Unicode normalization. List bounds count elements.

## Reproduction and repair

Added `tests/runtime/validation-unicode-length.test.ts`, using generated code
behind a real temporary localhost HTTP server. Initial result: **1 pass,
7 fail**. With an exact-one-scalar type, an emoji input incorrectly returned
400; with an exact-two-scalar type, the same emoji incorrectly returned 200.
A literal that the compiler accepted returned 500 when its generated
constructor counted UTF-16 code units. Field-local, inherited, nullable and
collection-element cases reproduced the same disagreement.

Changed only `constraint_checks` in `jadpo/crates/core/src/target.rs`:
string length uses JavaScript string iteration, which counts code points for
these valid Unicode inputs. Non-string values retain `.length`, preserving
List element counts. No normalization, wire encoding or failure-model changes.

After rebuilding: **8 pass, 0 fail, 87 assertions**. The suite checks BMP and
supplementary characters, combining sequences, both length bounds, field-local
and inherited constraints, nullable values, List size versus element length,
authored literal constructors, dynamic constructors, and compile-time rejection
of literals exceeding the same bound. Invalid dynamic construction remains the
existing safe 500 `internal_fault`; HTTP boundary violations return 400.

```sh
cargo build --manifest-path jadpo/Cargo.toml -q
bun --no-install --env-file=/dev/null test tests/runtime/validation-unicode-length.test.ts
```

The historical focused runs used `--no-env-file`, which Bun 1.2.20 ignores.
The reproducible command above uses an explicit empty env file; the final common
gate independently passed all eight cases with that isolation.

Initial/final logs: `build/validation/unicode-length/before.log` and
`build/validation/unicode-length/after.log`. The reproduction source and exact
expectations are retained in the enabled test file. Logs include expected
operational fault events from the deliberate invalid-construction cases.

## Proof limits

This is bounded compiler/runtime length parity for valid Unicode strings,
not exhaustive Unicode conformance. It does not establish unpaired-surrogate,
normalization, grapheme, Set/Map HTTP encoding, or regex semantics. Configuration
already uses scalar counts but is not independently exercised by this suite.
The parent owns registration and the full integration gate.
