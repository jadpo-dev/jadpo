# `data.on_delete_not_reference`

```jadpo check project=e-on-delete-not-reference expect=data.on_delete_not_reference
entity Note {
    id: Uuid
    title: Text {
        on_delete: OnDelete.cascade
    }
}
```

```text output
error: `Note.title` isn't a reference
 --> main.jadpo:4:20
  |
4 |         on_delete: OnDelete.cascade
  |                    ^^^^^^^^^^^^^^^^ nothing to delete
  reason: `on_delete:` says what deleting a referenced record does, so it belongs only on a field typed `X.id`.
  next: Remove `on_delete:` (agent)
  rule: data.on_delete_not_reference
  help: docs/language/05-data.html#data-on-delete-not-reference

check failed: 1 error, 1 file
```
