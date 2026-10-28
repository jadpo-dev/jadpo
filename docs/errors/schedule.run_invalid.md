# `schedule.run_invalid`

`run:` calls an action of the schedule's own domain with nothing; the time is `clock.now`, fixed for the run:

```jadpo check project=e-schedule-run expect=schedule.run_invalid
type RemindAt = Instant {}

action remind(at: RemindAt) -> Unit {}

/// Sends the reminders due.
schedule reminders {
    every: 15m
    run: remind(RemindAt(clock.now))
}
```

```text output
error: A schedule passes nothing to `remind`
 --> main.jadpo:8:16
  |
8 |     run: remind(RemindAt(clock.now))
  |                ^^^^^^^^^^^^^^^^^^^^^ arguments given
  reason: `run:` calls one action of the schedule's own domain with no arguments; the action reads `clock.now`, fixed for the run, and queries for anything else, so all the logic lives in the action.
  next: Write `run: remind()`, and let `remind` read `clock.now` and its queries for what it needs (agent)
  rule: schedule.run_invalid
  help: docs/language/12-jobs-and-events.html#schedule-run-invalid

check failed: 1 error, 1 file
```
