# `failure.reject_placement`

A `reject` is inside an expression.

## Why Jadpo refuses it

`reject` stops the callable, and every place a callable can stop is at the
start of a line, where a reader can see it. So `reject` is a statement of its
own or the result of a `match` arm, and never part of a value.

## What it looks like

main.jadpo:

```jadpo check project=e-reject-placement expect=failure.reject_placement
type Count = Int {}

type CountRefused = Failure {
    kind: FailureKind.rejected
}

function capped(count: Count) fails CountRefused -> Count {
    var checked: Count = reject CountRefused
    return checked
}
```

```text output
error: `reject` can only start a statement
 --> main.jadpo:8:26
  |
8 |     var checked: Count = reject CountRefused
  |                          ^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: `reject` is a statement of its own, or the result of a `match` arm, never part of a value.
  next: Move `reject` to a line of its own, under an `if` or as a `match` arm (agent)
  rule: failure.reject_placement
  help: https://jadpo.dev/docs/errors/failure.reject_placement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it should stop under a condition**, give it a line of its own under an
`if`:

```jadpo fragment
    if (count < 0) {
        reject CountRefused
    }
    return count
```

```jadpo check project=e-reject-placement-fix-1 hidden
type Count = Int {}

type CountRefused = Failure {
    kind: FailureKind.rejected
}

function capped(count: Count) fails CountRefused -> Count {
    if (count < 0) {
        reject CountRefused
    }
    return count
}
```

A negative count now ends `capped` with `CountRefused`.

## Related

- [Raising a failure: `reject`](../language/04-failures.md#raising-a-failure-reject)
- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
