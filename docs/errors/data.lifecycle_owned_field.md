# `data.lifecycle_owned_field`

Only a lifecycle's transitions change the fields it owns: every field a transition sets or `initial:` gives:

```jadpo check project=e-lifecycle-owned-field expect=data.lifecycle_owned_field
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                set: {
                    deleted_at: clock.now
                }
            }
        }
    }
}

action hide_todo(todo: Todo) -> Todo {
    return Todo.update(todo) {
        set: {
            deleted_at: clock.now
        }
    }
}
```

```text output
error: `Todo.update` sets `deleted_at`, which only `Todo`'s lifecycle changes
 --> main.jadpo:20:13
   |
20 |             deleted_at: clock.now
   |             ^^^^^^^^^^ owned by the lifecycle
  reason: `Todo`'s lifecycle owns `deleted_at`: only its transition `delete` changes it, so the record's stages always move as the lifecycle says.
  next: Remove `deleted_at`, and call `Todo.delete(todo)` to change it (agent)
  rule: data.lifecycle_owned_field
  help: https://jadpo.dev/docs/errors/data.lifecycle_owned_field

check failed: 1 error, 1 file
```
