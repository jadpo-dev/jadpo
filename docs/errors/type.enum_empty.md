# `type.enum_empty`

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
  help: docs/language/02-values-and-types.html#type-enum-empty

check failed: 1 error, 1 file
```
