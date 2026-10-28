# `data.on_delete_required`

```jadpo check project=e-on-delete-required expect=data.on_delete_required
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id
}
```

```text output
error: `Todo.owner_id` doesn't say what deleting its `User` does
 --> main.jadpo:7:5
  |
7 |     owner_id: User.id
  |     ^^^^^^^^ refers to a `User`
  reason: Any field typed `User.id` is a reference with a foreign key, and its block must say what happens when the `User` is deleted: `OnDelete.restricted` refuses the delete, `OnDelete.cascade` deletes this record too, and `OnDelete.cleared` sets an optional reference to `none`. There is no default.
  next: Add `on_delete:` to `owner_id`'s block (human)
  rule: data.on_delete_required
  help: docs/errors/data.on_delete_required.md

check failed: 1 error, 1 file
```
