# Seeded parser/type/failure fuzz campaign — RM-502

The integration test `jadpo/crates/semantic/tests/validation_fuzz_semantic.rs`
adds full semantic analysis to the existing parser-only malformed-input
campaign. It loads the sorted `tests/compile/pass` and `tests/compile/fail`
source corpus, applies 1–4 seeded token replacements/deletions/insertions per
case, and runs 2,048 generated inputs from fixed seed `0x7a4d93e1`.

For every input, the campaign checks deterministic parse trees, semantic graphs,
type-check results, and failure-analysis results, as well as UTF-8-aligned
token/diagnostic spans. Each run is isolated in a child process with a
90-second deadline. A detected failure triggers bounded delta reduction (up to
three seconds and 512 attempts); output includes the fixed seed, case index,
source-corpus index, failure kind, and minimized source.

## Retaining a regression

After fixing a discovered defect, add the minimized source to the matching
`tests/compile/pass` or `tests/compile/fail` corpus with its expected diagnostic
contract when applicable. The compile fixture runner checks the accepted result,
and this fuzz campaign automatically uses that fixture as a future mutation
seed. A reduced source printed only in a CI log is not a retained regression.

Run the focused campaign with:

```sh
cargo test --locked --manifest-path jadpo/Cargo.toml -p jadpo-semantic --test validation_fuzz_semantic
```

This is a reproducible bounded mutational campaign over the current fixture
corpus. It does not claim coverage-guided exploration, arbitrary-depth/resource
limits, a large language-law property proof, or an implementation mutation
score. Those broader claims remain explicitly out of scope for this RM-502
slice.
