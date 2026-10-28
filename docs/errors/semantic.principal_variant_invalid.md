# `semantic.principal_variant_invalid`

```jadpo check project=e-principal-variant-invalid expect=semantic.principal_variant_invalid
principal {
    user: {
        active: Member.suspended == false
    }
}

entity Member {
    id: Uuid
    suspended: Bool
}
```

```text output
error: `user:` has no `subject:`
 --> main.jadpo:2:5
  |
2 |     user: {
  |     ^^^^ no `subject:`
  reason: A principal's variant names the unique field that identifies a caller, `subject: <Entity>.<field>`, and may say when a record counts as active, `active: <condition>`.
  next: Add `subject: <Entity>.<field>`, naming a unique field (agent)
  rule: semantic.principal_variant_invalid
  help: docs/language/08-identity.html#semantic-principal-variant-invalid

check failed: 1 error, 1 file
```
