# `type.clock_context`

A function, a library function, an inline function or a backfill reads `clock.now`, or calls an operation that reads the clock such as `crypto.verify_jwt`, which only an action may do.

## Why Jadpo refuses it

A function gives the same result for the same arguments, so a test, a
query or a reviewer can rely on it without knowing when it ran. The time
changes with every operation, so reading it would make the function's
answer depend on when it was called. `clock.now` is the time the
operation started, fixed for the whole of it, and only an action reads
it; a function that needs the time takes it as a parameter.

## What it looks like

main.jadpo:

```jadpo check project=e-clock-context expect=type.clock_context
type ReminderCutoff = Instant {}

function cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now)
}
```

```text output
error: A function can't read `clock.now`
 --> main.jadpo:4:27
  |
4 |     return ReminderCutoff(clock.now)
  |                           ^^^^^^^^^ reads the clock
  reason: A function is pure: it gives the same result for the same arguments, so the time it needs arrives as a parameter; only actions read the clock.
  next: Pass the time in as a parameter, read in the action that calls this (agent)
  rule: type.clock_context
  help: https://jadpo.dev/docs/errors/type.clock_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Calling an operation that reads the clock is the same error.
`crypto.verify_jwt` checks a token's expiry against `clock.now`, so only
an action calls it:

main.jadpo:

```jadpo check project=e-clock-context-jwt expect=type.clock_context
type TokenRefused = Failure {
    kind: FailureKind.invalid_value
}

type TokenExpired = Failure {
    kind: FailureKind.invalid_value
}

function verified(token: Jwt, keys: List<Jwk>) fails TokenRefused, TokenExpired -> JwtClaims {
    return attempt crypto.verify_jwt(token, keys: keys) {
        invalid_signature: TokenRefused
        expired: TokenExpired
        not_yet_valid: TokenRefused
        malformed: TokenRefused
        wrong_audience: TokenRefused
    }
}
```

```text output
error: A function can't call `crypto.verify_jwt`
 --> main.jadpo:10:20
   |
10 |     return attempt crypto.verify_jwt(token, keys: keys) {
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ reads the clock
  reason: A function is pure: it gives the same result for the same arguments, so the time it needs arrives as a parameter; only actions read the clock.
  next: Pass the time in as a parameter, read in the action that calls this (agent)
  rule: type.clock_context
  help: https://jadpo.dev/docs/errors/type.clock_context

check failed: 1 error, 1 file
```

Its fix is the second one below: make `verified` an action.

## How to fix it

The question is whether the code needs to be a function.

**If it should stay a function**, take the time as a parameter, and read
`clock.now` in the action that calls it:

```jadpo fragment
function cutoff(now: ReminderCutoff) -> ReminderCutoff {
    return ReminderCutoff(now + 1h)
}

action reminder_cutoff() -> ReminderCutoff {
    return cutoff(ReminderCutoff(clock.now))
}
```

```jadpo check project=e-clock-context-fix-1 hidden
type ReminderCutoff = Instant {}

function cutoff(now: ReminderCutoff) -> ReminderCutoff {
    return ReminderCutoff(now + 1h)
}

action reminder_cutoff() -> ReminderCutoff {
    return cutoff(ReminderCutoff(clock.now))
}
```

The function now gives the same cutoff for the same time, so a test can
pass it any time it likes, and the action passes the time its operation
started.

**If it only reads the time**, make it an action:

```jadpo fragment
action cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now)
}
```

```jadpo check project=e-clock-context-fix-2 hidden
type ReminderCutoff = Instant {}

action cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now)
}
```

It now gives the time its operation started, the same for every action
that operation calls, and a test fixes it with `clock:`.

## Related

- [The current time: `clock.now`](../language/10-configuration-and-time.md#the-current-time-clocknow)
- [Where time can be read](../language/10-configuration-and-time.md#where-time-can-be-read)
- [Controlling time](../language/13-testing.md#controlling-time)
