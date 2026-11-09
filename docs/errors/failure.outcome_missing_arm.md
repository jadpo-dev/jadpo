# `failure.outcome_missing_arm`

A `match` on a call's outcome doesn't handle every failure the call can give.

## Why Jadpo refuses it

A `match` on a call's outcome is exhaustive, so a failure added to the call
later stops the build at every place that has to consider it. A failure with
no arm would have nowhere to go at run time.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

```jadpo check project=e-outcome-missing file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-missing expect=failure.outcome_missing_arm
function missing_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
    }
}
```

```text output
error: This `match` doesn't handle `LookupRefused`
 --> main.jadpo:2:12
  |
2 |     return match lookup(key) {
  |            ^^^^^^^^^^^^^^^^^ failures not handled
  reason: `lookup(...)` can fail with `LookupMissing` and `LookupRefused`, and a `match` on its outcome has an arm for every one: a value, `reject` or `propagate`.
  next: Add an arm for `LookupRefused` (agent)
  rule: failure.outcome_missing_arm
  help: https://jadpo.dev/docs/errors/failure.outcome_missing_arm

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If this code should handle it**, add an arm:

```jadpo fragment
function missing_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-missing-fix-1 hidden
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

function missing_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

A refused lookup now gives `"refused"`.

**If the caller should fail the same way**, pass it on with `propagate` and
list it after `fails`:

```jadpo fragment
function missing_failure(key: LookupKey) fails LookupRefused -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> propagate
    }
}
```

```jadpo check project=e-outcome-missing-fix-2 hidden
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

function missing_failure(key: LookupKey) fails LookupRefused -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> propagate
    }
}
```

A refused lookup now ends `missing_failure` with `LookupRefused`.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
