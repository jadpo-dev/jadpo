# `failure.outcome_duplicate_arm`

A `match` on a call's outcome handles the same failure twice.

## Why Jadpo refuses it

Each failure a call can give has one arm, and the first takes every instance
of it, so a second never runs. Code that can't run reads as behaviour the app
has, which is worse than no code at all.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

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

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the second arm is a leftover**, remove it:

```jadpo fragment
function duplicate_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-dup-fix-1 hidden
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

function duplicate_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

Each failure now has one arm, and a reader knows which runs.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
