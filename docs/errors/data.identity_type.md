# `data.identity_type`

```jadpo check project=e-identity-type expect=data.identity_type
entity Note {
    id: Text
}
```

```text output
error: `Note.id` must be a required `Uuid`
 --> main.jadpo:2:5
  |
2 |     id: Text
  |     ^^ not a `Uuid`
  reason: An entity's `id` is a `Uuid`, or a named type built on one; links between entities are references to it.
  next: Write `id: Uuid` (agent)
  rule: data.identity_type
  help: docs/errors/data.identity_type.md

check failed: 1 error, 1 file
```
