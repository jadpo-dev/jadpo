# `type.unknown_enum_variant`

The code names a variant that the enum doesn't have, often a misspelling of one it does.

## Why Jadpo refuses it

An enum is a closed list of named alternatives, so every value it can
hold is one of the variants it declares. A name that isn't among them
can never be a value of the enum. Jadpo reports it when you check, and
names the variant it is closest to when there is one.

## What it looks like

main.jadpo:

```jadpo check project=e-variant expect=type.unknown_enum_variant
type TicketStatus = Enum {
    open
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.opne
}
```

```text output
error: `TicketStatus` has no variant `opne`
 --> main.jadpo:7:25
  |
7 |     return TicketStatus.opne
  |                         ^^^^ not a variant
  reason: `TicketStatus`'s variants are `open` and `closed`. Did you mean `open`?
  next: Use `TicketStatus.open` (agent)
  rule: type.unknown_enum_variant
  help: https://jadpo.dev/docs/errors/type.unknown_enum_variant

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a variant the enum has**, spell it as the enum declares
it:

```jadpo fragment
    return TicketStatus.open
```

```jadpo check project=e-variant-fix-1 hidden
type TicketStatus = Enum {
    open
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.open
}
```

`reopen` now returns the `open` variant.

**If the enum needs another alternative**, add it to the enum's list:

```jadpo check project=e-variant-fix-2
type TicketStatus = Enum {
    open
    reopened
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.reopened
}
```

A `match` on `TicketStatus` must now cover `reopened` as well.

## Related

- [Enums](../language/02-values-and-types.md#enums)
