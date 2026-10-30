# `data.field_list_required`

```jadpo check project=e-field-list expect=data.field_list_required
entity Project {
    id: Uuid
    keeper: Text
    name: Text

    unique: {
        name_per_keeper: keeper
    }
}
```

```text output
error: The unique rule `name_per_keeper` isn't a list of fields
 --> main.jadpo:7:26
  |
7 |         name_per_keeper: keeper
  |                          ^^^^^^ not a list
  reason: Each unique rule is a list of the entity's fields in brackets: `name_per_keeper: [field, other]`.
  next: Write `name_per_keeper: [field, ...]` (agent)
  rule: data.field_list_required
  help: https://jadpo.dev/docs/errors/data.field_list_required

check failed: 1 error, 1 file
```
