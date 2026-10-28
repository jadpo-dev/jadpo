# `type.temporal_clock_invalid`

```jadpo check project=e-clock-invalid expect=type.temporal_clock_invalid
type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

function opening(day: ShopDay) -> Opening {
    return Opening(temporal.resolve(day, at: "9:30", zone: Zone.utc, dst: Dst.later))
}
```

```text output
error: `"9:30"` isn't a clock time
 --> main.jadpo:6:46
  |
6 |     return Opening(temporal.resolve(day, at: "9:30", zone: Zone.utc, dst: Dst.later))
  |                                              ^^^^^^ not `HH:MM`
  reason: `temporal.resolve`'s `at:` is a clock time written `HH:MM`, or `HH:MM:SS` with seconds, each part two digits, from `00:00` to `23:59`.
  next: Write the time with two-digit hours and minutes, such as `"09:30"` (agent)
  rule: type.temporal_clock_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-clock-invalid

check failed: 1 error, 1 file
```
