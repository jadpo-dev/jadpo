# `failure.outcome_missing_arm`

```jadpo check project=e-outcome-missing file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-missing expect=failure.outcome_missing_arm
function missing_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
    }
}
```

```text output
error: This `match` doesn't handle `LookupRefused`
 --> main.jadpo:2:12
  |
2 |     return match lookup(key) {
  |            ^^^^^^^^^^^^^^^^^ failures not handled
  reason: `lookup(...)` can fail with `LookupMissing` and `LookupRefused`, and a `match` on its outcome has an arm for every one: a value, `reject` or `propagate`.
  next: Add an arm for `LookupRefused` (agent)
  rule: failure.outcome_missing_arm
  help: docs/errors/failure.outcome_missing_arm.md

check failed: 1 error, 2 files
```
