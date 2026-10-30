# `data.cleared_required_reference`

```jadpo check project=e-cleared-required expect=data.cleared_required_reference
entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    project_id: Project.id {
        on_delete: OnDelete.cleared
    }
}
```

```text output
error: `Todo.project_id` can't be cleared
 --> main.jadpo:8:20
  |
8 |         on_delete: OnDelete.cleared
  |                    ^^^^^^^^^^^^^^^^ the field can't be `none`
  reason: `OnDelete.cleared` sets the reference to `none` when the `Project` is deleted, which only an optional reference (`Project.id?`) can hold.
  next: Make it `Project.id?`, or use `OnDelete.restricted` or `OnDelete.cascade` (agent)
  rule: data.cleared_required_reference
  help: https://jadpo.dev/docs/errors/data.cleared_required_reference

check failed: 1 error, 1 file
```
