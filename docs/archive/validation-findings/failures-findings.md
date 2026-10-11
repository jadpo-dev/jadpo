# Failure/callable independent validation — 2026-09-29

## Contract and scope

The expected behaviour was stated before reading checker implementation:

- `docs/failure-model.md` §5.2 requires the exact **reachable, unhandled**
  failure set. Missing and stale declarations are both errors.
- `docs/grammar-v0.1.md` §§10–12 requires explicit acknowledgement of fallible
  calls and exhaustive outcome matches. Recovery removes a failure, mapping
  replaces it and propagation retains it. This executable grammar settles the
  older failure-model document's open handler-grammar question.
- Functions cannot hide action effects behind a handler.
- `docs/failure-model.md` §§4, 6 and 12 require complete, correctly typed context,
  disjoint public/internal fields and no internal schema in public contracts.
- `docs/grammar-v0.1.md` §8 configuration rules permit secret configuration only
  at approved adapter sinks, not in public or internal failure context.

The suite is `jadpo/crates/core/tests/validation_failures.rs`. It uses the public
compiler API and literal independent expected sets. Cargo automatically discovers
this integration test, so the existing workspace validation command includes it.
No fixture-count table or runtime manifest change is required.

## Evidence

Run:

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_failures
```

Initial independent run: **17 tests, 15 passed, 2 failed**. The two failures
below were retained contract expectations. After correcting reachability and
adding five regression tests, the focused run is **22 passed, 0 failed**.
Existing core and semantic library regressions also passed: **57 + 34 tests**.
The parent owns final workspace/runtime integration verification.

The tests cover:

- All nine combinations of recovery, propagation and mapping for two failures.
- Exact failure propagation through another callable and a route.
- Missing acknowledgement and missing declarations, with invocation spans and
  independently applied source repairs.
- Stale handled failures, and application of the compiler's stale-set edit while
  preserving the still-reachable member and rejection body.
- Missing/duplicate success and failure arms and unknown failure arms, each
  triggered separately rather than by one error-heavy fixture.
- Failures in invocation arguments remaining outside the outer outcome handler.
- Recovery expressions introducing their own acknowledged failures.
- Action effects behind outcome matches remaining forbidden in functions.
- One terminating branch preserving a reachable subsequent rejection.
- Fully terminating `if`/`else` and exhaustive statement matches excluding dead
  continuations; a match arm that can continue keeping the continuation live.
- Unconditional rejection terminating before later rejections and calls.
- Dead fallible calls and outcome propagation producing no invented failures,
  while dead source still checks effects, acknowledgement and rejection payloads.
- Missing/extra context fields and incompatible nominal context types.
- Public/internal context separation, field-name overlap and secret-config sinks.
- Public OpenAPI excluding internal failure fields while the audit retains them.

## Confirmed defect: unreachable rejections count as reachable

Minimal valid reproduction:

```jadpo
type ResultText = Text {}
failure Missing { kind: NotFound code: "missing" }
function done() -> ResultText {
    return ResultText("complete")
    reject Missing
}
```

The checker reports `FAIL_UNDECLARED_PROPAGATION`, recommending that `Missing`
be added to `fails` or handled, although it cannot occur. The same error appears
after an `if`/`else` whose two branches return. This violates the exact reachable
set contract and risks inaccurate route error inventories.

Tests `exact_reachable_failures_exclude_statements_after_return` and
`all_terminating_branches_exclude_a_later_rejection` permit a separate
unreachable-code diagnostic but reject this false propagation claim. They also
assert parsing succeeded, so malformed input cannot make the tests pass.

Existing fixture `tests/compile/fail/114_invalid_outcome_matches.jadpo` also uses
two sequential unconditional rejects to pretend two outcomes are reachable.
Its intended outcome-match tests need a real branching producer after the
production correction; removing their expected diagnostics would weaken coverage.

The production correction tags collected calls and rejections with structural
reachability. All source is still collected and validated; only unreachable
facts are excluded from propagation and stale-set calculations. It does not
introduce constant folding or interprocedural non-returning analysis. Fixture114
now uses a real conditional producer and its existing expectations are unchanged.

The parent agent owns final integrated verification.

## Limits and remaining work

This is one bounded compiler/contract area, **not** comprehensive failure proof.
The API-schema check is not evidence of runtime serialization. HTTP body/header
non-disclosure, operational-fault normalization, request IDs, semantic traces,
transaction rollback, async sequencing, persistence faults and concurrent effects
still need runtime tests and an independent review of those tests.

The suite does not claim full coverage of every standard failure kind, module
visibility combination, recursion/fixed-point behaviour, wildcard diagnostics,
constant-condition reachability, all return-path checks or target-specific
failure lowering. Existing tests may cover some of these; they were not counted
as new independent evidence here. No randomized fuzz campaign or mutation run
was performed in this work package.

Plausible mutations a separate reviewer should try: ignore outcome recovery,
swallow argument failures, omit recovery-expression calls, treat handlers as
purity barriers, delete internal-field exclusion from OpenAPI, or truncate
failure traversal before a still-live continuation. Each corresponds to a
specific assertion above; actually testing those mutations remains separate work.

## Integration follow-up: field constructors and HTTP validation

The parent found that a checked `Customer.label("valid")` constructor emitted a
raw JavaScript `Customer.label(...)` call and failed at runtime because `Customer`
is a compile-time type. Target review also showed that resolving field references
to their underlying scalar erased field-local refinements at HTTP boundaries.

The correction in `jadpo/crates/core/src/target.rs` generates dedicated field
validators, preserves local and inherited constraints for record/variant fields,
uses complete parent validators for named refinements, and reads inherited
nullability from the semantic graph. Nullable candidates return before applying
non-null constraints. Supplied field values are evaluated once. The obsolete
primitive-only validator emitter was removed after its final caller was replaced.

`tests/runtime/field-contracts.test.ts` compiles fresh temporary source with
`JADPO_BIN` and exercises the generated application through `handleRequest`:
**9 tests passed**. It covers literal and dynamic constructors, discarded
constructor results, local/chained/named refinements, inherited nullable fields,
nullable named refinements, optional omission and supplied-value validation,
and an accessor that detects repeated evaluation. Invalid dynamic construction
is also exercised when the output contains only an unrelated `ok` field, so
output validation cannot mask an omitted constructor check.

These HTTP expectations follow the specific supported target contract in
`docs/runtime-target-v0.1.md`, Runtime boundaries: malformed or constraint-invalid
input is `400 invalid_request`; validation after action invocation is contained
as `500 internal_fault`. The general failure model's proposed distinction for
`422 InvalidValue` remains a contract-reconciliation topic; this fix does not
change transport policy or claim that dynamic construction has a complete
language-level failure protocol.

Persistence nullability/DDL, synthetic entity references, cross-target parity
and a complete constraint-family matrix remain outside this follow-up. The
parent registers this runtime suite and owns the final shared validation gate.
