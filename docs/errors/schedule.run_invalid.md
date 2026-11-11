# `schedule.run_invalid`

A schedule's `run:` isn't a call, with no arguments, of an action in the schedule's own domain.

## Why Jadpo refuses it

A schedule has no request and no event behind it, so there is nothing
for it to pass. What a run needs, the action finds out itself: the time
from `clock.now`, which is fixed for the whole run at the tick's time,
and anything else from its queries. Keeping every argument out of `run:`
keeps all of the run's logic in the action, where it is tested and
reviewed like any other, and an action of another domain is reached by
calling it from one of the schedule's own.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/schedule.run_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the action needs the time of the run**, let it read `clock.now`
itself and take no parameters. In `main.jadpo`:

```jadpo fragment
action remind() -> Unit {
    var at: RemindAt = RemindAt(clock.now)
    ...
}

schedule reminders {
    every: 15m
    run: remind()
}
```

```jadpo check project=e-schedule-run-fix-1 hidden
type RemindAt = Instant {}

action remind() -> Unit {
    var at: RemindAt = RemindAt(clock.now)
    assert at == RemindAt(clock.now)
}

/// Sends the reminders due.
schedule reminders {
    every: 15m
    run: remind()
}
```

`remind` gets the same time the schedule would have passed: inside a
run, every read of `clock.now` gives the time the run started.

## Related

- [`every:` and `run:`](../language/12-jobs-and-events.md#every-and-run)
- [The time inside a run](../language/12-jobs-and-events.md#the-time-inside-a-run)
- [Schedules](../language/12-jobs-and-events.md#schedules)
