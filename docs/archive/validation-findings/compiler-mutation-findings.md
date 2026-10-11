# Independent compiler mutation review — 2026-09-29

Two targeted mutations were tested against independently authored contract
tests. **Both baseline tests passed, both mutants compiled and failed the
intended assertions, and both restored tests passed.** This is evidence for two
specific regression detectors, not an overall mutation score or a claim of
comprehensive compiler coverage.

Durable local evidence is in
[`build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/`](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/).
That ignored build directory contains all six successful experiment logs,
`summary.json`, exact commands/exit codes in `results.json`, source SHA-256
hashes and both mutation diffs. The excluded setup-error log is also retained.

## Method and isolation

The workspace was copied to
`/private/tmp/jadpo-compiler-mutations-jd1hlvr9/jadpo`. Compiler build artifacts
and application data were excluded. The checked-in
`jadpo/data/iana-zones-2026c.txt` was copied because compiler `include_str!`
requires it; no existing database was used.

Each mutation changed one condition in the isolated semantic compiler sources.
Tests and their expectations were unchanged. Each command used Cargo offline,
the `jadpo-core` package, the named integration test and
`-- --exact --nocapture`. A mutant counted only when the executable test
reported `FAILED`, a panic named the intended assertion and Cargo returned 101;
compiler build errors did not count. The original source was restored after
each mutation, followed by a passing rerun of each baseline.

One initial setup attempt incorrectly excluded the compiler's **source**
directory `crates/core/src/target/` along with build output. Its missing-module
build failure is retained as `setup-missing-source-target-directory.log` and
excluded from all mutation results. Restoring that source directory fixed the
copy harness before either successful baseline.

## Mutation 1: dotted constructors must not probe a value receiver

Contract: `docs/type-system.md` §4.3 permits explicit field construction such as
`Customer.label("valid")`. `Customer` is a type owner, not an authored local
value that must exist in the function environment.

In `jadpo/crates/semantic/src/typecheck.rs`, `infer_invocation`, remove only
the `!is_type_constructor` guard:

```diff
-        if !is_type_constructor
-            && !self.catalogue.callables.contains_key(&callee)
+        if !self.catalogue.callables.contains_key(&callee)
             && invocation.callee.path.len() >= 2
```

Test:
`validation_types_contract::explicit_construction_narrows_to_field_contract`.
The normal compiler accepts the field constructor. The mutant produces
`TYPE_UNKNOWN_VALUE` for the speculative `Customer` receiver, so the test's
`expected accepted source` assertion fails. This is the original behavioural
defect resurfacing, rather than an assertion about implementation text.

Raw logs:

- [Baseline: 1 passed](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/dotted_constructor-baseline.log)
- [Mutant: intended assertion failed](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/dotted_constructor-mutant.log)
- [Restored: 1 passed](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/dotted_constructor-restored.log)

The original raw log paths use the same filenames under
`/private/tmp/jadpo-compiler-mutations-jd1hlvr9/`.

## Mutation 2: failure propagation must respect statement reachability

Contract: `docs/failure-model.md` §5.2 requires the declared failure set to
equal reachable propagation. A rejection after an unconditional return cannot
expand that callable's reachable failure contract.

In `jadpo/crates/semantic/src/failurecheck.rs`, `collect_block`, disable
progression of the reachability state:

```diff
-        reachable &= statement_can_continue(statement);
+        reachable = true;
```

Test:
`validation_failures::exact_reachable_failures_exclude_statements_after_return`.
The normal compiler gives the function an empty failure set. The mutant treats
the post-return `reject Missing` as reachable and emits
`FAIL_UNDECLARED_PROPAGATION`. The test fails specifically at its
`unreachable rejection included` assertion.

Raw logs:

- [Baseline: 1 passed](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/failure_reachability-baseline.log)
- [Mutant: intended assertion failed](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/failure_reachability-mutant.log)
- [Restored: 1 passed](../../build/validation/compiler-mutation-review/2026-09-29-jd1hlvr9/failure_reachability-restored.log)

The original raw log paths use the same filenames under
`/private/tmp/jadpo-compiler-mutations-jd1hlvr9/`.

## Review limits and concrete follow-up gaps

- The constructor acceptance test cannot establish emitted runtime behaviour.
  The already identified target defect (`Customer is not defined`) needs its
  separate runtime regression; killing this semantic mutant does not close it.
- This mutation removes the constructor guard. It does not challenge an
  overbroad guard that also suppresses legitimate entity-operation receiver
  resolution. That needs a receiver-call negative control or another targeted
  mutation; it is not implied by this result.
- The reachability mutation challenges a linear return followed by rejection.
  Existing new tests cover conditional/match continuation and continued
  validation of dead source, but this experiment does not mutate those helper
  calculations or the effect/context checks. Those tests were reviewed, not
  independently mutation-proven here.
- No campaign over all diagnostics, operators, runtime code or database
  boundaries was run. These two tests do not prove generated-target parity,
  runtime failure disclosure or transaction rollback.

No repository production source was edited during this review.
