# `failure.outcome_duplicate_arm`

```jadpo check project=e-outcome-dup file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-dup expect=failure.outcome_duplicate_arm
function duplicate_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupMissing -> "missing again"
        LookupRefused -> "refused"
    }
}
```

```text output
error: `LookupMissing` is handled twice
 --> main.jadpo:5:9
  |
4 |         LookupMissing -> "missing"
  |         ------------- first matched here
5 |         LookupMissing -> "missing again"
  |         ^^^^^^^^^^^^^ matched again
  reason: Each failure has one arm; the first takes every `LookupMissing`, so this one never runs.
  next: Remove this arm, or merge its body into the first (agent)
  rule: failure.outcome_duplicate_arm
  help: https://jadpo.dev/docs/errors/failure.outcome_duplicate_arm

check failed: 1 error, 2 files
```
