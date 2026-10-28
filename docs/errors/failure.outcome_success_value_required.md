# `failure.outcome_success_value_required`

```jadpo check project=e-outcome-success-value file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-success-value expect=failure.outcome_success_value_required
function invalid_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> propagate
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: The success arm has no failure to pass on
 --> main.jadpo:3:9
  |
3 |         success(value) -> propagate
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ `propagate` on success
  reason: `propagate` passes a failure arm's failure on to the caller; the success arm has a value, which it gives, or a failure it `reject`s.
  next: Give the success value, or `reject` a failure (agent)
  rule: failure.outcome_success_value_required
  help: docs/language/03-logic.html#failure-outcome-success-value-required

check failed: 1 error, 2 files
```
