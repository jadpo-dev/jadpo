# `data.lifecycle_purge_since`

`since:` names the time field purge counts from:

```jadpo check project=e-lifecycle-purge-since expect=data.lifecycle_purge_since
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30d
            since: title
        }
    }
}
```

```text output
error: `title` isn't a time
 --> main.jadpo:14:20
   |
14 |             since: title
   |                    ^^^^^ a `TodoTitle`
  reason: `since:` names the `Instant` field purge counts from: a record goes once it has been hidden for `after:` since that time.
  next: Name an `Instant` field of the record (agent)
  rule: data.lifecycle_purge_since
  help: https://jadpo.dev/docs/errors/data.lifecycle_purge_since

check failed: 1 error, 1 file
```
