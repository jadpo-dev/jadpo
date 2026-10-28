# `syntax.positional_after_named`

```jadpo check project=e-positional-after expect=syntax.positional_after_named
type Day = CalendarDate {}

function london_morning(date: Day) -> Instant {
    return temporal.resolve(
        date: date,
        "09:00",
        zone: Zone.europe_london,
        dst: Dst.earlier
    )
}
```

```text output
error: A positional argument follows a named one
 --> main.jadpo:6:9
  |
5 |         date: date,
  |         ---- named from here
6 |         "09:00",
  |         ^^^^^^^ not named
  reason: Once an argument is named, every argument after it is named too.
  next: Name this argument, or move it before `date:` (agent)
  rule: syntax.positional_after_named
  help: docs/language/03-logic.html#syntax-positional-after-named

check failed: 1 error, 1 file
```
