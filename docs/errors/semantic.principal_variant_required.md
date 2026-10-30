# `semantic.principal_variant_required`

```jadpo check project=e-principal-variant-required expect=semantic.principal_variant_required
principal {}
```

```text output
error: The principal declares no kind of caller
 --> main.jadpo:1:1
  |
1 | principal {}
  | ^^^^^^^^^ no `user:` or `service:`
  reason: A principal says which entity a signed-in caller is: `user:` for a person, `service:` for another system, or both.
  next: Add `user: { subject: <Entity>.<field> }` (agent)
  rule: semantic.principal_variant_required
  help: https://jadpo.dev/docs/errors/semantic.principal_variant_required

check failed: 1 error, 1 file
```
