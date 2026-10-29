# `data.lifecycle_unknown_field`

Every field a lifecycle names is one of the record's own:

```jadpo check project=e-lifecycle-unknown-field expect=data.lifecycle_unknown_field
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deletd_at == none
    }
}
```

```text output
error: `Todo` has no field `deletd_at`
 --> main.jadpo:6:18
  |
6 |         visible: deletd_at == none
  |                  ^^^^^^^^^ not declared
  reason: A lifecycle names the fields of the record it is written in, and `Todo`'s fields are `id` and `deleted_at`. Did you mean `deleted_at`?
  next: Use `deleted_at` instead of `deletd_at` (agent)
  rule: data.lifecycle_unknown_field
  help: docs/errors/data.lifecycle_unknown_field.md

check failed: 1 error, 1 file
```
