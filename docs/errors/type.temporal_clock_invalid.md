# `type.temporal_clock_invalid`

A clock time written as text isn't in the form `HH:MM` or `HH:MM:SS`, such as one with a single-digit hour.

## Why Jadpo refuses it

`at:` is a clock time written out, and the compiler reads it, so a time it
can't read is caught here and not when the app runs. It is written
`HH:MM`, or `HH:MM:SS` with seconds, each part two digits, from `00:00` to
`23:59`.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.temporal_clock_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the time is right but written another way**, give each part two
digits:

```jadpo fragment
    return Opening(temporal.resolve(day, at: "09:30", zone: Zone.utc, dst: Dst.later))
```

```jadpo check project=e-clock-invalid-fix hidden
type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

function opening(day: ShopDay) -> Opening {
    return Opening(temporal.resolve(day, at: "09:30", zone: Zone.utc, dst: Dst.later))
}
```

`opening` now gives half past nine in the morning on that day.

## Related

- [Choosing the instant: `dst:`](../language/10-configuration-and-time.md#choosing-the-instant-dst)
- [Zones and local times](../language/10-configuration-and-time.md#zones-and-local-times)
