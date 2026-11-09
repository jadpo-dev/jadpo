# `failure.outcome_success_value_required`

A `match` on a call's outcome has `propagate` in its `success(...)` arm.

## Why Jadpo refuses it

`propagate` passes a failure arm's failure on to the caller. The success arm
has no failure to pass on, only a value, so there is nothing for `propagate`
to mean there.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

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

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.outcome_success_value_required

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the arm should give the value**, give it:

```jadpo fragment
function invalid_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-success-value-fix-1 hidden
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

function invalid_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

A successful lookup now gives its value.

**If success should stop the callable**, `reject` a failure in the arm:

```jadpo fragment
function invalid_success(key: LookupKey) fails AlreadyThere -> LookupResult {
    return match lookup(key) {
        success -> reject AlreadyThere
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-success-value-fix-2 hidden
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

type AlreadyThere = Failure {
    kind: FailureKind.conflict
}

function invalid_success(key: LookupKey) fails AlreadyThere -> LookupResult {
    return match lookup(key) {
        success -> reject AlreadyThere
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

A lookup that finds something now ends with `AlreadyThere`.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
