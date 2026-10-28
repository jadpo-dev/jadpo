# `schedule.item_invalid`

A schedule holds `every:` and `run:`, once each; how it is retried is the same for every schedule:

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
  help: docs/language/12-jobs-and-events.html#schedule-item-invalid

check failed: 1 error, 1 file
```
