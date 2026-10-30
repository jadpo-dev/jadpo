# `failure.outcome_infallible`

```jadpo check project=e-outcome-infallible expect=failure.outcome_infallible
type LookupKey = Text {}

type LookupResult = Text {}

function plain(key: LookupKey) -> LookupResult {
    return "ok"
}

function needless(key: LookupKey) -> LookupResult {
    return match plain(key) {
        success(value) -> value
    }
}
```

```text output
error: `plain(...)` can't fail, so it has no outcome to match
 --> main.jadpo:10:12
   |
10 |     return match plain(key) {
   |            ^^^^^^^^^^^^^^^^ always succeeds
  reason: `plain(...)` lists no failures after `fails`, so it always gives its result and there is nothing for failure arms to handle.
  next: Use the call's result directly (agent)
  rule: failure.outcome_infallible
  help: https://jadpo.dev/docs/errors/failure.outcome_infallible

check failed: 1 error, 1 file
```
