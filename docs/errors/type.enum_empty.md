# `type.enum_empty`

An enum declares no variants.

## Why Jadpo refuses it

An enum is a closed list of named alternatives, and a value of it is
exactly one of them. With none listed there is nothing a value could
be, so the type could never be built and a `match` on it would have no
arms. Jadpo won't fill in the variants, because they are what the type
means.

## What it looks like

main.jadpo:

```jadpo check project=e-enum-empty-type expect=type.enum_empty
type TicketStatus = Enum {}
```

```text output
error: `TicketStatus` has no variants
 --> main.jadpo:1:6
  |
1 | type TicketStatus = Enum {}
  |      ^^^^^^^^^^^^ an empty enum
  reason: An enum is a closed list of named alternatives, so it needs at least one for a value to be.
  next: Add the enum's variants, one name per line (agent)
  rule: type.enum_empty
  help: https://jadpo.dev/docs/errors/type.enum_empty

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the type has states**, list them, one name per line:

```jadpo check project=e-enum-empty-type-fix
type TicketStatus = Enum {
    open
    waiting
    closed
}
```

A `TicketStatus` is `open`, `waiting` or `closed`, written
`TicketStatus.open` and so on.

## Related

- [Enums](../language/02-values-and-types.md#enums)
