# `syntax.positional_after_named`

An argument without a name follows a named one.

## Why Jadpo refuses it

Once an argument is named, every argument after it is named too. A value
with no name after a named one has no position a reader can count from, so
what it means would depend on which parameters the names before it had
already used.

## What it looks like

main.jadpo:

```jadpo check project=e-positional-after expect=syntax.positional_after_named
type Day = CalendarDate {}

type Opening = ZonedTime {}

function london_morning(date: Day) -> Opening {
    return Opening(temporal.resolve(
        date: date,
        "09:00",
        zone: Zone.europe_london,
        dst: Dst.earlier
    ))
}
```

```text output
error: A positional argument follows a named one
 --> main.jadpo:8:9
  |
7 |         date: date,
  |         ---- named from here
8 |         "09:00",
  |         ^^^^^^^ not named
  reason: Once an argument is named, every argument after it is named too.
  next: Name this argument, or move it before `date:` (agent)
  rule: syntax.positional_after_named
  help: https://jadpo.dev/docs/errors/syntax.positional_after_named

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value belongs in the middle of the call**, name it:

```jadpo fragment
    return Opening(
        temporal.resolve(date: date, at: "09:00", zone: Zone.europe_london, dst: Dst.earlier)
    )
```

```jadpo check project=e-positional-after-fix hidden
type Day = CalendarDate {}

type Opening = ZonedTime {}

function london_morning(date: Day) -> Opening {
    return Opening(
        temporal.resolve(date: date, at: "09:00", zone: Zone.europe_london, dst: Dst.earlier)
    )
}
```

Every argument now says which parameter it fills.

## Related

- [Calling](../language/03-logic.md#calling)
- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
