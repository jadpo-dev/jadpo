# `type.unknown_enum_variant`

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
  help: docs/language/02-values-and-types.html#type-unknown-enum-variant

check failed: 1 error, 1 file
```
