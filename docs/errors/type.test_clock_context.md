# `type.test_clock_context`

Code outside a test calls `clock.advance`, which only a test may do.

## Why Jadpo refuses it

Outside a test, the time is the operation's `clock.now`, fixed while it
runs, and a later request or schedule run reads a later one, so code can
rely on it. Only a test moves its clock, so that it never has to wait.
Elsewhere, in a fixture or a seed too, the line is refused.

## What it looks like

main.jadpo:

```jadpo check project=e-clock-context expect=type.test_clock_context
type ShiftEnd = Instant {}

action end_shift() -> ShiftEnd {
    clock.advance(5m)
    return ShiftEnd(clock.now)
}
```

```text output
error: An action can't move the clock
 --> main.jadpo:4:5
  |
4 |     clock.advance(5m)
  |     ^^^^^^^^^^^^^ moves the clock
  reason: Only a test moves its clock: elsewhere the time is the operation's `clock.now`, fixed while it runs, and a later request or schedule run reads a later one.
  next: Move the clock in a test with `clock.advance(...)`, or remove this line (agent)
  rule: type.test_clock_context
  help: https://jadpo.dev/docs/errors/type.test_clock_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether you want a time later than now, or to move time in
a test.

**If you want a later time**, add the duration to `clock.now`:

```jadpo fragment
action end_shift() -> ShiftEnd {
    return ShiftEnd(clock.now + 5m)
}
```

```jadpo check project=e-clock-context-fix-1 hidden
type ShiftEnd = Instant {}

action end_shift() -> ShiftEnd {
    return ShiftEnd(clock.now + 5m)
}
```

`end_shift` now gives five minutes after the time the action started.

**If you wanted to move time in a test**, take the line out of the action
and write it in a test, in `main.test.jadpo`:

```jadpo check project=e-clock-context-fix-2 file=main.jadpo hidden
type ShiftEnd = Instant {}

action end_shift() -> ShiftEnd {
    return ShiftEnd(clock.now)
}
```

```jadpo check project=e-clock-context-fix-2 file=main.test.jadpo
test "a shift ends when it is ended" {
    clock: "2026-01-15T12:00:00Z"

    clock.advance(5m)
    assert end_shift() == "2026-01-15T12:05:00Z"
}
```

The test now runs with its clock five minutes on.

## Related

- [Controlling time](../language/13-testing.md#controlling-time)
- [The current time: `clock.now`](../language/10-configuration-and-time.md#the-current-time-clocknow)
