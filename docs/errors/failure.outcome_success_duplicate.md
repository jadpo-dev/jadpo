# `failure.outcome_success_duplicate`

A `match` on a call's outcome has two `success(...)` arms.

## Why Jadpo refuses it

A call succeeds one way, so one `success(...)` arm takes its value and a second
never runs. Code that can't run reads as behaviour the app has.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

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

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the second arm is a leftover**, remove it:

```jadpo fragment
function duplicate_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-two-success-fix-1 hidden
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

function duplicate_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

A successful lookup now has one arm, and a reader knows which runs.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
