# `data.identity_missing`

```jadpo check project=e-identity-missing expect=data.identity_missing
entity Note {
    body: Text
}
```

```text output
error: `Note` has no `id` field
 --> main.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ no `id`
  reason: Every entity's identity is one field, `id`, based on `Uuid`: generated when a record is created and never supplied by callers, so other code can refer to it as `Entity.id`.
  next: Add `id: Uuid` as the entity's first field (agent)
  rule: data.identity_missing
  help: docs/language/05-data.html#data-identity-missing

check failed: 1 error, 1 file
```
