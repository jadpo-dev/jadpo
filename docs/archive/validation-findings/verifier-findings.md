# Verification gate review

Nine Python tests in `test_verifier.py` check evidence inventory and golden
obligation accounting. They run as the first step of `tools/verify.py`.

The initial run passed six and failed three. The failures demonstrated:

- Nested runtime test files were absent from the suite inventory.
- New examples containing only nested entity files escaped classification.
- An unexecuted golden obligation could be labelled passed without executable
  evidence.

The verifier now discovers runtime files recursively, discovers example source
recursively while excluding generated build directories, and allows only open
obligation statuses with explicit gaps. All nine tests pass.

## Targeted mutation evidence

The review copied only `tools/verify.py` and `test_verifier.py` into disposable
temporary directories and ran `python3 -m unittest discover -s tests/validation
-p 'test_*.py'`. No production source was mutated in place. The baseline passed;
each of these independent mutations failed its intended assertion:

| Mutation | Detecting test |
|---|---|
| Runtime `rglob('*.test.ts')` becomes `glob('*.test.ts')` | `test_new_nested_runtime_suite_cannot_be_silently_omitted` |
| Recursive example discovery becomes `any(p.glob('*.jadpo'))` | `test_example_with_only_entity_dossiers_still_requires_classification` |
| Add `passed` to allowed obligation statuses | `test_a_claimed_pass_without_an_executable_case_is_rejected` |

Raw local output: `build/validation/verifier-mutation-review.json`.
This is three targeted mutations of the verification harness, not a compiler
mutation score, exhaustive fuzz campaign or proof of runtime behaviour.
