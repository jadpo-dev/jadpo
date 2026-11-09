# `failure.outcome_pattern_invalid`

A `match` on a call's outcome has an arm with a pattern for values.

## Why Jadpo refuses it

A `match` on a call has a `success(name)` arm for its value and an arm for each
failure it can give. A pattern such as `none` or a literal is about the value,
which the outcome match hasn't looked inside. Mixing the two would leave
unclear which case an arm is about.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

```jadpo check project=e-outcome-pattern file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-pattern expect=failure.outcome_pattern_invalid
function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
        none -> "nothing"
    }
}
```

```text output
error: `none` can't match a call's outcome
 --> main.jadpo:6:9
  |
6 |         none -> "nothing"
  |         ^^^^ a pattern for values
  reason: A `match` on `lookup(...)` has a `success(name)` arm for its value and an arm for each failure it can give; to look at the value's own cases, match on the success value.
  next: Remove this arm, or match on the success value inside `success(...)` (agent)
  rule: failure.outcome_pattern_invalid
  help: https://jadpo.dev/docs/errors/failure.outcome_pattern_invalid

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the arm was a leftover**, remove it:

```jadpo fragment
function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-pattern-fix-1 hidden
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

function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

The `match` now handles each outcome once.

**If you do want to look at the value**, match on it inside `success(...)`:

```jadpo fragment
function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> match value {
            "found" -> "seen"
            _ -> value
        }
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-pattern-fix-2 hidden
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

function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> match value {
            "found" -> "seen"
            _ -> value
        }
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

The value's own cases are now handled by a `match` on the value.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
