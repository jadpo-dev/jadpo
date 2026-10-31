# `schedule.every_invalid`

A schedule's `every:` says how often it runs in something other than a whole number of seconds, minutes, hours or days.

## Why Jadpo refuses it

A schedule's runs fall on a grid of times, one `every:` apart, kept in
the schedule's state so that a restart or a deploy doesn't move them.
The grid counts in whole seconds, minutes, hours or days above zero:
`30s`, `15m`, `1h` or `1d`. A gap in milliseconds, a fraction or zero has
no place on it, so Jadpo refuses it rather than round it to a different
interval.

## What it looks like

main.jadpo:

```jadpo check project=e-schedule-every expect=schedule.every_invalid
action tidy() -> Unit {}

/// Tidies up four times a second.
schedule tidy_up {
    every: 250ms
    run: tidy()
}
```

```text output
error: `every: 250ms` can't be how often a schedule runs
 --> main.jadpo:5:12
  |
5 |     every: 250ms
  |            ^^^^^ not seconds, minutes, hours or days
  reason: `every:` is a whole number of seconds, minutes, hours or days above zero (`30s`, `15m`, `1h`, `1d`), counted in UTC from when the application starts.
  next: Write a duration such as `every: 15m` (agent)
  rule: schedule.every_invalid
  help: https://jadpo.dev/docs/errors/schedule.every_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the work can wait a second or more between runs**, give `every:` a
whole number of seconds or more. In `main.jadpo`:

```jadpo fragment
/// Tidies up once a minute.
schedule tidy_up {
    every: 1m
```

```jadpo check project=e-schedule-every-fix-1 hidden
action tidy() -> Unit {}

/// Tidies up once a minute.
schedule tidy_up {
    every: 1m
    run: tidy()
}
```

`tidy` now runs once a minute. Only one run happens at a time, and ticks
missed while a run is still going become one run, not a backlog.

## Related

- [`every:` and `run:`](../language/12-jobs-and-events.md#every-and-run)
- [When runs happen](../language/12-jobs-and-events.md#when-runs-happen)
- [Schedules](../language/12-jobs-and-events.md#schedules)
