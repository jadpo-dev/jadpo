# `type.not_a_type`

A field's type is a type, never a value: a call, a literal or an enum's
variant written where the type goes is refused, naming the field. Every
block of fields is held to it, an entity's and `config { }`'s settings too:

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
  help: docs/errors/type.not_a_type.md

check failed: 1 error, 1 file
```
