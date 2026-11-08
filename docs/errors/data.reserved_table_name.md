# `data.reserved_table_name`

An entity's table would have a name starting with `jadpo_`.

## Why Jadpo refuses it

Tables whose names start with `jadpo_` are the runtime's own: the outbox,
event deliveries and idempotency keys. An entity's table takes its name
from the entity, so one called `JadpoAudit` would land among them, and
could clash with one a later release adds.

## What it looks like

main.jadpo:

```jadpo check project=e-reserved-table expect=data.reserved_table_name
entity JadpoAudit {
    id: Uuid
}
```

```text output
error: `JadpoAudit`'s table would be called `jadpo_audit`
 --> main.jadpo:1:8
  |
1 | entity JadpoAudit {
  |        ^^^^^^^^^^ a system table's prefix
  reason: Tables whose names start with `jadpo_` are the runtime's own (the outbox, event deliveries, idempotency keys), so no entity's table may take that prefix.
  next: Rename `JadpoAudit` (agent)
  rule: data.reserved_table_name
  help: https://jadpo.dev/docs/errors/data.reserved_table_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the entity can have another name**, rename it so its table doesn't
start with `jadpo_`:

```jadpo check project=e-reserved-table-fix
entity ActivityEntry {
    id: Uuid
}
```

Its table is now called `activity_entry`.

## Related

- [System tables](../language/05-data.md#system-tables)
- [Storage types](../language/05-data.md#storage-types)
