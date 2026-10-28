# `data.lifecycle_purge_after`

A hidden record is kept for a positive duration before purge deletes it:

```jadpo check project=e-lifecycle-purge-after expect=data.lifecycle_purge_after
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 0d
            since: deleted_at
        }
    }
}
```

```text output
error: `after: 0d` isn't a positive duration
 --> main.jadpo:8:20
  |
8 |             after: 0d
  |                    ^^ not a positive duration
  reason: `after:` is how long a hidden record is kept before purge deletes it, a duration such as `30d` or `12h`, longer than nothing; a hidden record would be deleted the moment it is hidden.
  next: Write a duration, such as `after: 30d` (agent)
  rule: data.lifecycle_purge_after
  help: docs/errors/data.lifecycle_purge_after.md

check failed: 1 error, 1 file
```
