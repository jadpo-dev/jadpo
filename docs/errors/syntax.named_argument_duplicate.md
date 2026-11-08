# `syntax.named_argument_duplicate`

A call gives the same named argument twice.

## Why Jadpo refuses it

Each named argument is given once. With two of them Jadpo would have to
ignore one, and it can't tell which you meant, so it stops even when the two
hold the same value.

## What it looks like

main.jadpo:

```jadpo check project=e-named-duplicate expect=syntax.named_argument_duplicate
type Day = CalendarDate {}

type Opening = ZonedTime {}

function london_morning(date: Day) -> Opening {
    return Opening(temporal.resolve(
        date: date,
        at: "09:00",
        zone: Zone.europe_london,
        zone: Zone.europe_london,
        dst: Dst.earlier
    ))
}
```

```text output
error: `zone:` is given twice
 --> main.jadpo:10:9
   |
 9 |         zone: Zone.europe_london,
   |         ---- first given here
10 |         zone: Zone.europe_london,
   |         ^^^^ given again here
  reason: Each named argument is given once.
  next: Remove one of the `zone:` arguments (agent)
  rule: syntax.named_argument_duplicate
  help: https://jadpo.dev/docs/errors/syntax.named_argument_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you gave the argument twice by mistake**, keep the value you mean and
remove the other:

```jadpo fragment
    return Opening(
        temporal.resolve(date: date, at: "09:00", zone: Zone.europe_london, dst: Dst.earlier)
    )
```

```jadpo check project=e-named-duplicate-fix hidden
type Day = CalendarDate {}

type Opening = ZonedTime {}

function london_morning(date: Day) -> Opening {
    return Opening(
        temporal.resolve(date: date, at: "09:00", zone: Zone.europe_london, dst: Dst.earlier)
    )
}
```

The call now gives each of its arguments once, and `zone:` is London.

## Related

- [Calling](../language/03-logic.md#calling)
- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
