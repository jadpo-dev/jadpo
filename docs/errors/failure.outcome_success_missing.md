# `failure.outcome_success_missing`

```jadpo check project=e-outcome-no-success file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-no-success expect=failure.outcome_success_missing
function missing_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: This `match` doesn't handle `lookup(...)` succeeding
 --> main.jadpo:2:12
  |
2 |     return match lookup(key) {
  |            ^^^^^^^^^^^^^^^^^ no `success(...)` arm
  reason: A `match` on a call's outcome handles its value too, in a `success(name)` arm, as well as each failure.
  next: Add `success(value) -> ...` as the first arm (agent)
  rule: failure.outcome_success_missing
  help: docs/errors/failure.outcome_success_missing.md

check failed: 1 error, 2 files
```
