# `data.unique_single_field`

```jadpo check project=e-unique-single expect=data.unique_single_field
entity Account {
    id: Uuid
    handle: Text

    unique: {
        only_handle: [handle]
    }
}
```

```text output
error: The unique rule `only_handle` has one field
 --> main.jadpo:6:9
  |
6 |         only_handle: [handle]
  |         ^^^^^^^^^^^ one field
  reason: The `unique:` section holds rules over several fields; a single field is made unique in its own block, `handle: ... { unique: true }`.
  next: Move it to `handle`'s block as `unique: true` (agent)
  rule: data.unique_single_field
  help: docs/language/05-data.html#data-unique-single-field

check failed: 1 error, 1 file
```
