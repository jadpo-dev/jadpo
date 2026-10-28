# `failure.outcome_unknown_arm`

```jadpo check project=e-outcome-unknown file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-unknown expect=failure.outcome_unknown_arm
type LookupTimedOut = Failure {
    kind: FailureKind.rejected
}

function unknown_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
        LookupTimedOut -> "timed out"
    }
}
```

```text output
error: `lookup(...)` can't fail with `LookupTimedOut`
 --> main.jadpo:10:9
   |
10 |         LookupTimedOut -> "timed out"
   |         ^^^^^^^^^^^^^^ never happens here
  reason: `lookup(...)` fails with `LookupMissing` and `LookupRefused`, so an arm for anything else never runs.
  next: Remove this arm (agent)
  rule: failure.outcome_unknown_arm
  help: docs/language/03-logic.html#failure-outcome-unknown-arm

check failed: 1 error, 2 files
```
