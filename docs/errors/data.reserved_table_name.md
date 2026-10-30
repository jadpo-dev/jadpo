# `data.reserved_table_name`

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
