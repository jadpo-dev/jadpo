# `failure.outcome_wildcard`

```jadpo check project=e-outcome-wildcard file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-wildcard expect=failure.outcome_wildcard
function wildcard_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        _ -> "fallback"
    }
}
```

```text output
error: `_` can't stand for a call's failures
 --> main.jadpo:4:9
  |
4 |         _ -> "fallback"
  |         ^ wildcard
  reason: Every failure a call can give has an arm of its own, so a failure added to the call later is caught at every `match` that has to consider it.
  next: Replace `_` with arms for `LookupMissing` and `LookupRefused` (agent)
  rule: failure.outcome_wildcard
  help: docs/errors/failure.outcome_wildcard.md

check failed: 1 error, 2 files
```
