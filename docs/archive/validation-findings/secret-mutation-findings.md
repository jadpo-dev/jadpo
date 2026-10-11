# Independent secret-flow mutation review — 2026-09-30

**Both targeted mutants compiled and failed their intended executable test.**
Each unmodified baseline passed, and each restored baseline passed. No compiler
build failure was counted as a detected mutation. This establishes two specific
regression detectors, not comprehensive secret-flow or mutation coverage.

Evidence is retained under
[`build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/`](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/).
It includes all six raw logs, exact mutation diffs, exact commands and exit codes,
source/test SHA-256 hashes, the executable review script and a JSON summary.

## Isolation and method

The current compiler workspace was copied once to
`/private/tmp/jadpo-secret-mutations-_j6jk0b5/jadpo`. Only the workspace-root build
`target/` directory was excluded; the compiler's **source** submodule
`crates/core/src/target/first_party.rs` and checked-in
`data/iana-zones-2026c.txt` were retained and checked. Application databases were
excluded and no database was used. Cargo ran offline with a separate temporary
build directory. Production sources and the test expectations were never edited.

Each run selected `jadpo-core`, integration suite
`validation_config_time_contract`, one exact test, and `-- --exact --nocapture`.
For each mutation the sequence was: passing baseline, one production-code change
in the isolated copy, executable assertion failure, original source restoration,
and another passing run. Tests were hash-checked unchanged throughout.

Snapshot hashes:

- `crates/semantic/src/typecheck.rs`:
  `05f866365bf2148346528194e98eb803394382f2b31f8913a797c446f34b2992`
- `crates/core/tests/validation_config_time_contract.rs`:
  `33a3f13e24dfa102c7329734bf415a6d3a812cab31780a6ff22c38247d790d7b`

The snapshot is pinned evidence; later concurrent compiler work does not alter
which source these results describe. `source-hashes.json` records every copied
file, and `results.json` records each tested compiler/test hash.

## Constructor classification

Test: `explicit_construction_cannot_remove_secret_classification`.
Contract: CONFIG-D05 secret classification cannot be removed by ordinary
construction. A secret local alias wrapped in `ApiKey(alias)` must not become
an ordinary return value merely because its nominal type was reconstructed.

Mutation:

```diff
- result.secret = argument_type.as_ref().is_some_and(|value| value.secret);
+ result.secret = false;
```

The mutant compiled, then accepted the leak with **no diagnostics**. The test
failed specifically because `CONFIG_SECRET_FLOW` was missing. Cargo returned
101 with one executed failing test. Restoration produced one passing test.

[Baseline log](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/constructor-secret-baseline.log),
[mutant log](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/constructor-secret-mutant.log),
[restored log](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/constructor-secret-restored.log),
[exact diff](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/constructor-secret.diff).

## Temporal helper boundary

Test: `temporal_helpers_are_not_secret_sinks`.
Contract: ordinary temporal computation is not an approved secret-consuming
adapter. `temporal.in_zone(config.private_time, Zone.europe_london)` must not
expose secret configuration through a record's ordinary `Time` field.

Mutation: remove the `received.secret` guard and its `CONFIG_SECRET_FLOW`
diagnostic from `require_temporal_expected`, leaving temporal representation
compatibility checking intact.

The mutant compiled and accepted this leak with **no diagnostics**. The test
failed specifically because `CONFIG_SECRET_FLOW` was missing. Cargo returned
101 with one executed failing test. Restoration produced one passing test.

[Baseline log](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/temporal-secret-baseline.log),
[mutant log](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/temporal-secret-mutant.log),
[restored log](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/temporal-secret-restored.log),
[exact diff](../../build/validation/secret-mutation-review/2026-09-30-_j6jk0b5/temporal-secret.diff).

## Limits

These results cover the two concrete regressions above. They do not prove all
constructor forms, temporal operations, implicit flows, logs, generated target
boundaries, future optimizations or secret sinks. No live secret values or
application configuration were used. The parent owns final shared-workspace
verification; this isolated review does not substitute for that gate.
