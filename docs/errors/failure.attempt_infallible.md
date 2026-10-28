# `failure.attempt_infallible`

```jadpo check project=e-attempt-infallible expect=failure.attempt_infallible
type Count = Int {}

function plain(n: Count) -> Count {
    return n
}

function wrapper(n: Count) -> Count {
    return attempt plain(n)
}
```

```text output
error: `plain(...)` can't fail, so it takes no `attempt`
 --> main.jadpo:8:12
  |
8 |     return attempt plain(n)
  |            ^^^^^^^ nothing to pass on
  reason: `plain(...)` lists no failures after `fails`, so it always gives its result; `attempt` marks only calls that can fail.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: docs/language/03-logic.html#failure-attempt-infallible

check failed: 1 error, 1 file
```

The outcome-match examples below share a hidden file declaring
`lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult`.
