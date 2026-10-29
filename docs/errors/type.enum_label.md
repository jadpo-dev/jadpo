# `type.enum_label`

Enums have no display labels: a variant is a name, or a name with a block
of its fields. Its wording lives in the domain's translations (area 10,
"Enums with wording"):

```jadpo check project=e-enum-label expect=type.enum_label
type TicketStatus = Enum {
    open: "Open"
    closed
}
```

```text output
error: `TicketStatus.open` is given a label
 --> main.jadpo:2:11
  |
2 |     open: "Open"
  |           ^^^^^^ a label
  reason: Enums have no display labels: a variant is a name, or a name with a block of its fields. Wording for readers lives in the domain's translations, which give each value of `TicketStatus` its wording in every locale.
  next: Write `open` alone, and give its wording in the domain's translations: `ticket_status: { open: "Open" }` (agent)
  rule: type.enum_label
  help: docs/errors/type.enum_label.md

check failed: 1 error, 1 file
```
