# `failure.outcome_subject_required`

```jadpo check project=e-outcome-subject expect=failure.outcome_subject_required
type LookupKey = Text {}

function not_a_call(key: LookupKey) -> LookupKey {
    return match key {
        success(value) -> value
    }
}
```

```text output
error: `key` isn't a call, so it has no outcome to match
 --> main.jadpo:4:12
  |
4 |     return match key {
  |            ^^^^^^^^^ not a call
  reason: `success(...)` and failure arms match the outcome of a call that can fail; a value is matched by its own cases, such as an enum's variants.
  next: Match `key`'s cases, or match on the call that can fail (agent)
  rule: failure.outcome_subject_required
  help: docs/language/03-logic.html#failure-outcome-subject-required

check failed: 1 error, 1 file
```
