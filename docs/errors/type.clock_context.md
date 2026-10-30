# `type.clock_context`

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
