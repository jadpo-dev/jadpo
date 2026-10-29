# `syntax.named_argument_duplicate`

```jadpo check project=e-named-duplicate expect=syntax.named_argument_duplicate
type Day = CalendarDate {}

function london_morning(date: Day) -> Instant {
    return temporal.resolve(
        date: date,
        at: "09:00",
        zone: Zone.europe_london,
        zone: Zone.europe_london,
        dst: Dst.earlier
    )
}
```

```text output
error: `zone:` is given twice
 --> main.jadpo:8:9
  |
7 |         zone: Zone.europe_london,
  |         ---- first given here
8 |         zone: Zone.europe_london,
  |         ^^^^ given again here
  reason: Each named argument is given once.
  next: Remove one of the `zone:` arguments (agent)
  rule: syntax.named_argument_duplicate
  help: docs/errors/syntax.named_argument_duplicate.md

check failed: 1 error, 1 file
```
