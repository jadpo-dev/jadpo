# `failure.outcome_success_missing`

A `match` on a call's outcome has no `success(...)` arm.

## Why Jadpo refuses it

A `match` on a call has to say what happens in every outcome, and succeeding
is one of them. Without a `success(...)` arm the call's value would have
nowhere to go.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

```jadpo check project=e-outcome-no-success file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-no-success expect=failure.outcome_success_missing
function missing_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: This `match` doesn't handle `lookup(...)` succeeding
 --> main.jadpo:2:12
  |
2 |     return match lookup(key) {
  |            ^^^^^^^^^^^^^^^^^ no `success(...)` arm
  reason: A `match` on a call's outcome handles its value too, in a `success(name)` arm, as well as each failure.
  next: Add `success(value) -> ...` as the first arm (agent)
  rule: failure.outcome_success_missing
  help: https://jadpo.dev/docs/errors/failure.outcome_success_missing

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value is what you want**, add the arm first:

```jadpo fragment
function missing_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-no-success-fix-1 hidden
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

function missing_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

A successful lookup now gives its value.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
