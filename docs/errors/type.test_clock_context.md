# `type.test_clock_context`

Application code cannot move the clock:

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
  help: docs/language/13-testing.html#type-test-clock-context

check failed: 1 error, 1 file
```
