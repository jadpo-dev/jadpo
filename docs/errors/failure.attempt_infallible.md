# `failure.attempt_infallible`

An `attempt` is on something that can't fail.

## Why Jadpo refuses it

`attempt` marks a point where a callable can stop early, and a reader relies
on every one meaning that. On something that always succeeds it marks a stop
that never happens. A call can fail only when its `fails` list names a
failure, and building a value can fail only when nothing proves the value
follows its type's rules.

## What it looks like

main.jadpo:

```jadpo check project=e-attempt-infallible expect=failure.attempt_infallible
type Count = Int {}

function plain(n: Count) -> Count {
    return n
}

function wrapper(n: Count) -> Count {
    return attempt plain(n)
}
```

```text output
error: `plain(...)` can't fail, so it takes no `attempt`
 --> main.jadpo:8:12
  |
8 |     return attempt plain(n)
  |            ^^^^^^^ nothing to pass on
  reason: `plain(...)` lists no failures after `fails`, so it always gives its result; `attempt` marks only calls that can fail.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: https://jadpo.dev/docs/errors/failure.attempt_infallible

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Building a value the declared ranges prove follows its type's rules can't
fail either:

main.jadpo:

```jadpo check project=e-attempt-infallible-proved expect=failure.attempt_infallible
type Quantity = Int {
    min: 1
    max: 1000
}

function single() -> Quantity {
    return attempt Quantity(1)
}
```

```text output
error: `Quantity(...)` can't fail, so it takes no `attempt`
 --> main.jadpo:7:12
  |
7 |     return attempt Quantity(1)
  |            ^^^^^^^ nothing to pass on
  reason: The declared ranges prove `Quantity(...)` always follows its type's rules, so building it never fails; `attempt` marks only what can.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: https://jadpo.dev/docs/errors/failure.attempt_infallible

check failed: 1 error, 1 file
```

Nor can a value that is already there, which isn't a call at all:

main.jadpo:

```jadpo check project=e-attempt-infallible-value expect=failure.attempt_infallible
type Count = Int {}

function wrapper(n: Count) -> Count {
    return attempt n
}
```

```text output
error: `n` can't fail, so it takes no `attempt`
 --> main.jadpo:4:12
  |
4 |     return attempt n
  |            ^^^^^^^ nothing to pass on
  reason: `attempt` marks a call that can fail, and this isn't a call.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: https://jadpo.dev/docs/errors/failure.attempt_infallible

check failed: 1 error, 1 file
```

## How to fix it

**If the call can't fail**, remove `attempt`:

```jadpo fragment
    return plain(n)
```

```jadpo check project=e-attempt-infallible-fix-1 hidden
type Count = Int {}

function plain(n: Count) -> Count {
    return n
}

function wrapper(n: Count) -> Count {
    return plain(n)
}
```

`wrapper` reads the same at run time, and no longer claims a way to stop.

**If it should be able to fail**, say how in the callable it calls, then keep
the `attempt` and list the failure in the caller:

```jadpo fragment
function plain(n: Count) fails CountRefused -> Count {
    if (n < 0) {
        reject CountRefused
    }
    return n
}
```

```jadpo check project=e-attempt-infallible-fix-2 hidden
type Count = Int {}

type CountRefused = Failure {
    kind: FailureKind.rejected
}

function plain(n: Count) fails CountRefused -> Count {
    if (n < 0) {
        reject CountRefused
    }
    return n
}

function wrapper(n: Count) fails CountRefused -> Count {
    return attempt plain(n)
}
```

A negative count now ends `plain` and `wrapper` with `CountRefused`.

## Related

- [Attempt](../language/03-logic.md#attempt)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
- [`failure.attempt_required`](failure.attempt_required.md)
