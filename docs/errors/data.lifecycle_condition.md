# `data.lifecycle_condition`

A lifecycle condition tests the record's own fields against fixed values, never another record or who is asking:

```jadpo check project=e-lifecycle-condition expect=data.lifecycle_condition
entity Todo {
    id: Uuid
    owner_id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none and owner_id == current_user.id
    }
}
```

```text output
error: `current_user.id` isn't something a lifecycle condition can compare with
 --> main.jadpo:7:53
  |
7 |         visible: deleted_at == none and owner_id == current_user.id
  |                                                     ^^^^^^^^^^^^^^^ from outside the record
  reason: A lifecycle condition compares the record's own fields with fixed values: a literal, an enum value or `clock.now`. It never mentions other records or who is asking, which is policy (area 9), so it can join every statement's `WHERE` and an index.
  next: Compare with a literal, an enum value or `clock.now` (agent)
  rule: data.lifecycle_condition
  help: https://jadpo.dev/docs/errors/data.lifecycle_condition

check failed: 1 error, 1 file
```
