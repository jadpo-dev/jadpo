# `failure.outcome_pattern_invalid`

A `match` on a call's outcome has no patterns for values:

```jadpo check project=e-outcome-pattern file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-pattern expect=failure.outcome_pattern_invalid
function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
        none -> "nothing"
    }
}
```

```text output
error: `none` can't match a call's outcome
 --> main.jadpo:6:9
  |
6 |         none -> "nothing"
  |         ^^^^ a pattern for values
  reason: A `match` on `lookup(...)` has a `success(name)` arm for its value and an arm for each failure it can give; to look at the value's own cases, match on the success value.
  next: Remove this arm, or match on the success value inside `success(...)` (agent)
  rule: failure.outcome_pattern_invalid
  help: https://jadpo.dev/docs/errors/failure.outcome_pattern_invalid

check failed: 1 error, 2 files
```
