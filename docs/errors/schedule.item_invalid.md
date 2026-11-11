# `schedule.item_invalid`

A schedule holds an item it doesn't take, gives one twice or leaves out `every:` or `run:`.

## Why Jadpo refuses it

A schedule is two things: how often it runs, `every:`, and the action it
runs, `run:`. Everything else about how it runs is the same for every
schedule: one run at a time, ticks missed while a run is going merged
into one, and a failed run tried again at its next due time. So there is
nothing else to set, and Jadpo refuses an item it would ignore, such as
`retry:` or `timeout:`, rather than let it look like it changes
something. A schedule without `every:` or `run:` can't run at all.

## What it looks like

main.jadpo:

```jadpo check project=e-schedule-item expect=schedule.item_invalid
action tidy() -> Unit {}

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
    retry: next_schedule
}
```

```text output
error: `retry` isn't something a schedule holds
 --> main.jadpo:7:5
  |
7 |     retry: next_schedule
  |     ^^^^^ not a schedule item
  reason: A schedule says how often it runs with `every:` and the action it runs with `run:`; nothing else, since one run at a time, merged missed ticks and the next tick as the retry are how every schedule runs.
  next: Write `every: <duration>` and `run: <action>()`, once each (agent)
  rule: schedule.item_invalid
  help: https://jadpo.dev/docs/errors/schedule.item_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A schedule that leaves out `run:` is the same error:

```jadpo check project=e-schedule-item-run expect=schedule.item_invalid
/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
}
```

```text output
error: `tidy_up` has no `run:`
 --> main.jadpo:2:10
  |
2 | schedule tidy_up {
  |          ^^^^^^^ no `run:`
  reason: A schedule says how often it runs with `every:` and the action it runs with `run:`; nothing else, since one run at a time, merged missed ticks and the next tick as the retry are how every schedule runs.
  next: Write `every: <duration>` and `run: <action>()`, once each (agent)
  rule: schedule.item_invalid
  help: https://jadpo.dev/docs/errors/schedule.item_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the item was meant to change how the schedule is retried**, remove
it. In `main.jadpo`:

```jadpo fragment
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

```jadpo check project=e-schedule-item-fix-1 hidden
action tidy() -> Unit {}

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

`tidy` runs every 15 minutes, and a run that fails is tried again at the
next quarter of an hour, as every schedule's is.

**If the schedule has no `run:`**, add one that calls the action it
runs, with no arguments. In `main.jadpo`:

```jadpo fragment
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

```jadpo check project=e-schedule-item-fix-2 hidden
action tidy() -> Unit {}

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

The schedule now runs `tidy` every 15 minutes.

## Related

- [`every:` and `run:`](../language/12-jobs-and-events.md#every-and-run)
- [When a run fails](../language/12-jobs-and-events.md#when-a-run-fails)
- [Schedules](../language/12-jobs-and-events.md#schedules)
