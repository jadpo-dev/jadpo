# `type.temporal_option_unused`

```jadpo check project=e-option-unused expect=type.temporal_option_unused
type ShopDay = CalendarDate {}

function next_day(day: ShopDay) -> ShopDay {
    return temporal.add_days(day, 1, dst: Dst.compatible)
}
```

```text output
error: `dst:` means nothing for a `ShopDay`
 --> main.jadpo:4:38
  |
4 |     return temporal.add_days(day, 1, dst: Dst.compatible)
  |                                      ^^^ `ShopDay` has no clock time
  reason: `dst:` chooses an instant when a local time falls in a daylight-saving gap or overlap; `temporal.add_days` on a `ShopDay` moves a date, which has no clock time, so no such time arises.
  next: Remove `dst: ...` (agent)
  rule: type.temporal_option_unused
  help: https://jadpo.dev/docs/errors/type.temporal_option_unused

check failed: 1 error, 1 file
```

`zone:` on a date or a local time is reported the same way: a
`ZonedTime` carries its zone, and a `CalendarDate` has none.
