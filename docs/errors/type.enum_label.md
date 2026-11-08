# `type.enum_label`

An enum variant is given a label or a value, as in `open: "Open"`.

## Why Jadpo refuses it

An enum has no display labels: a variant is a name, or a name with a
block of its fields. Wording for people depends on who reads it, so it
lives in the domain's language files, which give each variant its
wording in every supported locale. A label written in the enum would be
one language's wording, fixed in the type.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.enum_label

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the label is how people should see the variant**, write the
variant alone:

```jadpo check project=e-enum-label-fix
type TicketStatus = Enum {
    open
    closed
}
```

Then give `open` its wording in the domain's language files, as
`ticket_status: { open: "Open" }`. A template that writes out a
`TicketStatus` shows that wording, in the reader's language.

## Related

- [Enums](../language/02-values-and-types.md#enums)
- [Enums with wording](../language/10-configuration-and-time.md#enums-with-wording)
