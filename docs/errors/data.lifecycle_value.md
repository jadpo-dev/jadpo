# `data.lifecycle_value`

A transition sets each field to a fixed value, the same every time it runs:

```jadpo check project=e-lifecycle-value expect=data.lifecycle_value
entity Todo {
    id: Uuid
    deleted_at: Instant?
    edited_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                set: {
                    deleted_at: edited_at
                }
            }
        }
    }
}
```

```text output
error: `edited_at` is another field of the record
 --> main.jadpo:11:33
   |
11 |                     deleted_at: edited_at
   |                                 ^^^^^^^^^ a field, not a value
  reason: A transition sets each field to a fixed value (a literal, an enum value, `none` or `clock.now`), the same every time it runs; copying one field into another isn't one.
  next: Use a literal, an enum value, `none` or `clock.now` (agent)
  rule: data.lifecycle_value
  help: docs/language/05-data.html#data-lifecycle-value

check failed: 1 error, 1 file
```
