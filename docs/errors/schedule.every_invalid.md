# `schedule.every_invalid`

`every:` is a whole number of seconds, minutes, hours or days:

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
  help: docs/language/12-jobs-and-events.html#schedule-every-invalid

check failed: 1 error, 1 file
```
