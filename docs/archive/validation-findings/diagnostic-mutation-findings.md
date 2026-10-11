# Independent diagnostic discovery mutation review — 2026-09-30

Two targeted mutations were challenged against the root-authored catalogue
discovery regressions. **Both baseline runs passed, both mutants compiled and
failed the intended assertion, and both restored runs passed.** This establishes
two specific regression detectors, not an overall mutation score or proof that
every compiler diagnostic has executable trigger coverage.

## Isolation and reproducible evidence

Only an isolated copy at
`/private/tmp/jadpo-diagnostic-mutations-3hl8he7j/jadpo` was mutated. The copy
contains workspace manifests, lockfile, crate sources/tests and compile-fixture
inputs; it does not contain existing build output or application databases.
`CARGO_TARGET_DIR=/private/tmp/jadpo-diagnostic-mutations-3hl8he7j/cargo-target`
kept compilation separate from the parent's simultaneous full verification.
Cargo ran offline. Shared production sources and regression tests were unchanged.

Durable local evidence:
[`build/validation/diagnostic-mutation-review/3hl8he7j/`](../../build/validation/diagnostic-mutation-review/3hl8he7j/).
The ignored directory contains six raw logs, both exact mutation diffs,
`results.json` with commands/environment/exit codes/source hashes for every run,
`setup.json` with baseline input hashes, `summary.json`, and the driver script.
Each command has a 180-second subprocess deadline; no timeout or setup failure
occurred. Tests and their expectations remained unchanged in all six runs.

The exact command form was:

```sh
CARGO_TARGET_DIR=/private/tmp/jadpo-diagnostic-mutations-3hl8he7j/cargo-target \
cargo test --offline \
  --manifest-path /private/tmp/jadpo-diagnostic-mutations-3hl8he7j/jadpo/Cargo.toml \
  -p jadpo-diagnostics --test validation_catalogue_discovery \
  TEST_NAME -- --exact --nocapture
```

`TEST_NAME` was respectively
`discovery_ignores_comments_and_test_only_codes_but_includes_new_families` and
`references_are_bounded_to_real_test_bodies_and_string_literals`, with baseline,
mutated and restored runs for each. The working directory was the temporary
copy's repository root. Full expanded commands appear in each raw log and
`results.json`.

## Mutation 1: omit recently added diagnostic families

In `crates/diagnostics/build.rs`, remove `"CONFIG_", "POLICY_", "TEST_", ` from
the `PREFIXES` array used by `is_diagnostic_code`.

The mutant compiled and the family-discovery test failed at its equality
assertion: actual `[]`, expected
`["CONFIG_VALUE_MISSING", "POLICY_SCOPE_MISSING", "TEST_FAILED"]`. This verifies
that regression to the incomplete prefix list is detected through the production
discovery function, rather than only by checking catalogue metadata existence.

| Run | Exit code | Result | Raw log |
|---|---:|---|---|
| Baseline | 0 | 1 passed | `missing-families-baseline.log` |
| Mutated | 101 | Intended assertion failed | `missing-families-mutated.log` |
| Restored | 0 | 1 passed | `missing-families-restored.log` |

## Mutation 2: credit an unexecuted nested helper to its surrounding test

In `Codes::visit_item_fn`, change:

```rust
if self.production && !test_only(&item.attrs) {
```

to:

```rust
if !self.production || !test_only(&item.attrs) {
```

This retains production traversal but also traverses nested helper declarations
when collecting a test body's references. The mutant compiled and the bounded
references test failed at its equality assertion. Actual references gained
`("TYPE_NESTED_HELPER", "actual")`; expected references contained only
`CONFIG_VALUE_MISSING` and `POLICY_SCOPE_MISSING` from the real test body. Thus an
unused helper can no longer silently inflate that test's reference metadata.

| Run | Exit code | Result | Raw log |
|---|---:|---|---|
| Baseline | 0 | 1 passed | `nested-helper-leakage-baseline.log` |
| Mutated | 101 | Intended assertion failed | `nested-helper-leakage-mutated.log` |
| Restored | 0 | 1 passed | `nested-helper-leakage-restored.log` |

## Source identity and limits

SHA-256 values:

| Source | SHA-256 |
|---|---|
| Baseline/restored `build.rs` | `40b41559eb997773ae0c752d38a3fa4ec047d1d1ee682f2995d2c6310d3b7ded` |
| Family omission mutant | `62db80ec631f4f77d766e7c471ee7da288f6a176141cf572fe3253c431ee1c67` |
| Nested helper leakage mutant | `4cd8253b615e76ae67b20f5b31784975fbe625234a7c476e425d76dbddbf0202` |
| Unchanged discovery regression tests | `830e28d96c20c05cb880f0d5e93298529324c180434f48fdd54f938c446a4770` |

Manifest/lockfile hashes are retained in the machine-readable evidence.
A successful mutation result required a completed
build, the selected test's `FAILED` result, its intended equality assertion and
Cargo exit code 101; build failures would not count.

These tests inventory source literals and bounded references. They do not prove
assertion execution, diagnostic reachability, completeness of dynamically formed
codes, or that all discovered emitters can execute. This review did not mutate
comment handling, compound `cfg` evaluation, fixture JSON extraction or source
`target/` discovery. Those have separate regression tests, but this bounded
review makes no independent mutation claim about them. The parent owns the
shared full gate and the broader diagnostic coverage ledger.
