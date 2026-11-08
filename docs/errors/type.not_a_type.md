# `type.not_a_type`

A field's type is written as a value, a call or a block of fields, such as an enum's variant where the enum goes.

## Why Jadpo refuses it

A field's type says which values the field may hold, such as
`TicketStatus`; one of those values, such as `TicketStatus.open`, is
given where a value is built. Writing the value where the type goes
usually means a default was meant, which an `Input` field gives with
`default:`. Every block of fields is held to it: an object's, an
entity's and the settings in `config { }`.

## What it looks like

main.jadpo:

```jadpo check project=e-not-a-type expect=type.not_a_type
type TicketStatus = Enum {
    open
    closed
}

type NewTicket = Input {
    status: TicketStatus.open
}
```

```text output
error: `status`'s type is written as a value, `TicketStatus.open`
 --> main.jadpo:7:13
  |
7 |     status: TicketStatus.open
  |             ^^^^^^^^^^^^^^^^^ a value of `TicketStatus`, not a type
  reason: A field's type says which values it may hold, such as `TicketStatus`; `TicketStatus.open` is one of those values, given where a value is built.
  next: Write `TicketStatus` (agent)
  rule: type.not_a_type
  help: https://jadpo.dev/docs/errors/type.not_a_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A block of fields where the type goes is the same error, because there is
no unnamed object:

main.jadpo:

```jadpo check project=e-not-a-type-block expect=type.not_a_type
type Street = Text {
    max_length: 200
}

type NewTicket = Input {
    address: {
        street: Street
    }
}
```

```text output
error: `address`'s type is written as a block of fields
 --> main.jadpo:6:14
  |
6 |     address: {
  |              ^ an unnamed object, not a type
  reason: There is no unnamed object: a nested shape is declared once, with a name, and each field that holds it uses that name.
  next: Declare `type Address = Object { ... }` with these fields, and write `address: Address` (agent)
  rule: type.not_a_type
  help: https://jadpo.dev/docs/errors/type.not_a_type

check failed: 1 error, 1 file
```

Its fix is the one its `next:` line gives: declare the shape once, as a
named type with those fields, and write `address: Address`.

## How to fix it

The question is whether the value was meant as the field's default.

**If it was**, write the type, and give the value as `default:`:

```jadpo fragment
type NewTicket = Input {
    status: TicketStatus {
        default: TicketStatus.open
    }
}
```

```jadpo check project=e-not-a-type-fix-1 hidden
type TicketStatus = Enum {
    open
    closed
}

type NewTicket = Input {
    status: TicketStatus {
        default: TicketStatus.open
    }
}
```

A request that leaves `status` out now gets `TicketStatus.open`.

**If it wasn't**, write the type alone:

```jadpo fragment
type NewTicket = Input {
    status: TicketStatus
}
```

```jadpo check project=e-not-a-type-fix-2 hidden
type TicketStatus = Enum {
    open
    closed
}

type NewTicket = Input {
    status: TicketStatus
}
```

A request now sends `status` as either of the two variants.

## Related

- [Field types](../language/02-values-and-types.md#field-types)
- [Enums](../language/02-values-and-types.md#enums)
- [Names that aren't values](../language/03-logic.md#names-that-arent-values)
