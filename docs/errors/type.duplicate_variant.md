# `type.duplicate_variant`

An enum declares the same variant twice.

## Why Jadpo refuses it

Each variant of an enum has its own name, so `TicketStatus.open` and a
`match` arm for it name exactly one alternative. Two `open` lines would
leave a value, and every `match` on it, naming two at once. Jadpo won't
merge them or rename one, because it can't know whether the second was
a mistake or a different state you meant to name.

## What it looks like

main.jadpo:

```jadpo check project=e-duplicate-variant expect=type.duplicate_variant
type TicketStatus = Enum {
    open
    closed
    open
}
```

```text output
error: `TicketStatus.open` is declared twice
 --> main.jadpo:4:5
  |
2 |     open
  |     ---- first declared here
4 |     open
  |     ^^^^ declared again
  reason: Each variant of an enum has its own name, so a value and a `match` arm name exactly one.
  next: Remove one `open`, or rename it (agent)
  rule: type.duplicate_variant
  help: https://jadpo.dev/docs/errors/type.duplicate_variant

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the second `open` is another state.

**If it is the same state**, remove the second line:

```jadpo check project=e-duplicate-variant-fix-1
type TicketStatus = Enum {
    open
    closed
}
```

A `TicketStatus` is `open` or `closed`.

**If it is another state**, give it its own name:

```jadpo check project=e-duplicate-variant-fix-2
type TicketStatus = Enum {
    open
    closed
    reopened
}
```

A `TicketStatus` now has three variants, and every `match` on one
handles `reopened` as well.

## Related

- [Enums](../language/02-values-and-types.md#enums)
- [Match](../language/03-logic.md#match)
