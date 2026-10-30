# `failure.outcome_success_duplicate`

```jadpo check project=e-outcome-two-success file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-two-success expect=failure.outcome_success_duplicate
function duplicate_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        success(other) -> other
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: The success arm is written twice
 --> main.jadpo:4:9
  |
3 |         success(value) -> value
  |         -------------- first matched here
4 |         success(other) -> other
  |         ^^^^^^^^^^^^^^ matched again
  reason: A call succeeds one way, so one `success(...)` arm takes its value; this one would never run.
  next: Remove this arm, or merge its body into the first (agent)
  rule: failure.outcome_success_duplicate
  help: https://jadpo.dev/docs/errors/failure.outcome_success_duplicate

check failed: 1 error, 2 files
```
