# RM-503 targeted mutation findings

**Status:** partial campaign; five selected mutations killed, with additional
obligation areas still to review. This is not a broad mutation score.

## Method

Each mutant ran in an isolated temporary copy of Jadpo. The unmutated test was
run first, the source-only mutation was compiled and challenged, and the source
was restored before rerunning the same test. The primary workspace's production
sources and tests were not modified. The current six source/test files match
the SHA-256 fingerprints captured with the experiment.

| Obligation | Deliberate regression | Test and observed result |
|---|---|---|
| Type-name lookup is case-sensitive | Case-fold semantic ID and kind lookup. | [`type_name_resolution_is_case_sensitive`](../../jadpo/crates/semantic/src/lib.rs) passed before and after restoration; the mutant compiled, then failed because it emitted no `SEM_UNKNOWN_NAME` for the wrong-case reference. |
| Runtime names reject repeated underscores | Remove the `__` guard. | [`semantic_name_shapes_separate_types_from_runtime_names`](../../jadpo/crates/semantic/src/lib.rs) passed before and after restoration; the mutant failed on `user__name`. |
| Runtime names reject trailing underscores | Remove the trailing-underscore guard. | [`semantic_name_shapes_separate_types_from_runtime_names`](../../jadpo/crates/semantic/src/lib.rs) passed before and after restoration; the mutant failed on `user_name_`. |
| Keywords reserve complete tokens only | Treat every identifier beginning with `type` as the `type` keyword. | [`keywords_only_reserve_the_complete_identifier`](../../jadpo/crates/syntax/tests/validation_syntax_boundaries.rs) passed before and after restoration; the mutant returned a `Type` token for `type_value` instead of an identifier. |
| Generated text length counts Unicode scalars | Emit JavaScript UTF-16 `.length` instead of scalar iteration. | [`validation-unicode-length.test.ts`](../../tests/runtime/validation-unicode-length.test.ts) passed before and after restoration (8 tests, 87 assertions); the mutant compiled, then failed 7 tests on supplementary-scalar length boundaries while one negative-literal test passed. |

All five selected mutants were killed by the intended existing assertions; no
survivor in this sample required an assertion repair. Raw diffs and logs are
retained locally under the ignored `build/validation/rm503-name-mutations/`
directory. The durable findings are recorded here; the local artifacts are not
a repository-wide score or a claim of reproducible CI evidence.

## Source fingerprints

These SHA-256 values matched the current workspace files during this review:

| File | SHA-256 |
|---|---|
| `jadpo/crates/semantic/src/lib.rs` | `b55c20ae174f21b2d18adb95ef16f44c5511b84225ac56fc7a7a4b6ec6c9c035` |
| `jadpo/crates/semantic/tests/validation_fuzz_semantic.rs` | `64cdeb9a3f500577ca0e38c61fd54d4520633a9009f0096e4a75ab4528075b90` |
| `jadpo/crates/syntax/src/lib.rs` | `0f8421e63d5aead132dd3e3df5dc2acf2699c0ce6244469dced9a3be9f27e6c1` |
| `jadpo/crates/syntax/tests/validation_syntax_boundaries.rs` | `2210f06985a76b8a59627c5b22f1703688efebf3174f2632389e167f074328bc` |
| `jadpo/crates/core/src/target.rs` | `96b39c79547f4301f00a6a96edca670a20a4d53a3066a10b37b9ea2e238e27d9` |
| `tests/runtime/validation-unicode-length.test.ts` | `025c21a46122274711e923260dd6b0094b83efc7f3edfb1f6056d9ef47ebd0b9` |

## Remaining work

Continue targeted mutations against additional named obligations, particularly
the other syntax, diagnostic, runtime, and cross-feature areas in the
[validation gap inventory](roadmap-gap-inventory.md). Keep owner-decided
semantics gated, preserve any survivors as findings, and repair only missing
assertions where the accepted contract already determines the expected result.
