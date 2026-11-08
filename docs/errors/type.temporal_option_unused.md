# `type.temporal_option_unused`

An operation is given an option that means nothing for its value, such as `dst:` on a date.

## Why Jadpo refuses it

An option is there because it changes what the operation gives, so one
that changes nothing is a mistake about the value. `dst:` chooses an
instant when a local time falls in a daylight-saving gap or overlap, and a
date has no clock time to fall in one.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`zone:` on a date or a local time is reported the same way: a `ZonedTime`
carries its zone, and a `CalendarDate` has none.

## How to fix it

**If the option does nothing here**, remove it:

```jadpo fragment
    return temporal.add_days(day, 1)
```

```jadpo check project=e-option-unused-fix hidden
type ShopDay = CalendarDate {}

function next_day(day: ShopDay) -> ShopDay {
    return temporal.add_days(day, 1)
}
```

The call gives the same date, and nobody reading it looks for a time.

## Related

- [Choosing the instant: `dst:`](../language/10-configuration-and-time.md#choosing-the-instant-dst)
- [Arithmetic and comparison](../language/10-configuration-and-time.md#arithmetic-and-comparison)
