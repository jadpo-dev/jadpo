# `data.duplicate_rule`

The same two fields in another order are the same rule:

```jadpo check project=e-duplicate-rule expect=data.duplicate_rule
entity Account {
    id: Uuid
    tenant: Text
    handle: Text

    unique: {
        tenant_handle: [tenant, handle]
        handle_tenant: [handle, tenant]
    }
}
```

```text output
error: The unique rule `handle_tenant` repeats `tenant_handle`
 --> main.jadpo:8:9
  |
7 |         tenant_handle: [tenant, handle]
  |         ------------- first here
8 |         handle_tenant: [handle, tenant]
  |         ^^^^^^^^^^^^^ same fields
  reason: Both cover the same fields, in whatever order, so they are one rule; a write's conflict names one of them (area 6).
  next: Remove `handle_tenant` (agent)
  rule: data.duplicate_rule
  help: https://jadpo.dev/docs/errors/data.duplicate_rule

check failed: 1 error, 1 file
```
