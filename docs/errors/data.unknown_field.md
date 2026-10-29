# `data.unknown_field`

```jadpo check project=e-unknown-field expect=data.unknown_field
entity Project {
    id: Uuid
    name: Text

    unique: {
        name_per_owner: [owner_id, name]
    }
}
```

```text output
error: `Project` has no field `owner_id`
 --> main.jadpo:6:26
  |
6 |         name_per_owner: [owner_id, name]
  |                          ^^^^^^^^ no such field
  reason: The unique rule `name_per_owner` names fields of `Project`, its own.
  next: Remove `owner_id` (agent)
  rule: data.unknown_field
  help: docs/errors/data.unknown_field.md

check failed: 1 error, 1 file
```
