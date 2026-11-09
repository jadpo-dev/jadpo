# `failure.outcome_infallible`

A `match` on a call's outcome is on a call that can't fail.

## Why Jadpo refuses it

`success(...)` and failure arms handle a call's outcome. A call whose `fails`
list is empty always gives its result, so the `match` has nothing to choose
between and hides that the call can't stop.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the call can't fail**, use its result directly:

```jadpo fragment
function needless(key: LookupKey) -> LookupResult {
    return plain(key)
}
```

```jadpo check project=e-outcome-infallible-fix-1 hidden
type LookupKey = Text {}

type LookupResult = Text {}

function plain(key: LookupKey) -> LookupResult {
    return "ok"
}

function needless(key: LookupKey) -> LookupResult {
    return plain(key)
}
```

The call reads as what it is, one that always gives its result.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
- [`failure.attempt_infallible`](failure.attempt_infallible.md)
