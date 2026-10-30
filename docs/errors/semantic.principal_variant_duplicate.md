# `semantic.principal_variant_duplicate`

```jadpo check project=e-principal-variant-duplicate expect=semantic.principal_variant_duplicate
principal {
    user: {
        subject: Member.handle
    }
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `user:` is declared twice
 --> main.jadpo:5:5
  |
2 |     user: {
  |     ---- first declared here
5 |     user: {
  |     ^^^^ declared again
  reason: A principal says once which entity a user is, so a caller is looked up one way.
  next: Remove one `user:` (agent)
  rule: semantic.principal_variant_duplicate
  help: https://jadpo.dev/docs/errors/semantic.principal_variant_duplicate

check failed: 1 error, 1 file
```
