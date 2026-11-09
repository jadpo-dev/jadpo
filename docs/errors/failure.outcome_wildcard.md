# `failure.outcome_wildcard`

A `match` on a call's outcome uses `_` for its failures.

## Why Jadpo refuses it

Every failure a call can give has an arm of its own, so a failure added to the
call later stops the build at every `match` that has to consider it. A `_`
would quietly take the new one too, and a case nobody thought about would
get an answer written for something else.

## What it looks like

`lookup` is declared in another file of the project, and can fail with
`LookupMissing` or `LookupRefused`.

```jadpo check project=e-outcome-wildcard file=lookup.jadpo hidden
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

```jadpo check project=e-outcome-wildcard expect=failure.outcome_wildcard
function wildcard_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        _ -> "fallback"
    }
}
```

```text output
error: `_` can't stand for a call's failures
 --> main.jadpo:4:9
  |
4 |         _ -> "fallback"
  |         ^ wildcard
  reason: Every failure a call can give has an arm of its own, so a failure added to the call later is caught at every `match` that has to consider it.
  next: Replace `_` with arms for `LookupMissing` and `LookupRefused` (agent)
  rule: failure.outcome_wildcard
  help: https://jadpo.dev/docs/errors/failure.outcome_wildcard

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If some failures get the same answer**, still give each one its own arm:

```jadpo fragment
function wildcard_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-wildcard-fix-1 hidden
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

function wildcard_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

A failure added to `lookup` later now stops the build here, until someone
says what it means.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
