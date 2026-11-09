# `failure.outcome_unknown_arm`

A `match` on a call's outcome has an arm for a failure the call can't give.

## Why Jadpo refuses it

Each arm of a `match` on a call answers one failure in the call's `fails` list.
An arm for anything else never runs, and tells a reader the call can fail in
a way it can't.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

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

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.outcome_unknown_arm

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the call can't give that failure**, remove the arm:

```jadpo fragment
function unknown_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-unknown-fix-1 hidden
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

function unknown_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

The arms now match what the call can give.

**If it should be able to**, add it to the call's `fails` list and raise it
there:

```jadpo fragment
function lookup(key: LookupKey) fails LookupMissing, LookupRefused, LookupTimedOut -> LookupResult {
    if (key == "slow") {
        reject LookupTimedOut
    }
    ...
}
```

```jadpo check project=e-outcome-unknown-fix-2 hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

type LookupTimedOut = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused, LookupTimedOut -> LookupResult {
    if (key == "slow") {
        reject LookupTimedOut
    }
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
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

A slow key now gives `"timed out"`.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
