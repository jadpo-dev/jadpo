# `data.lifecycle_purge_visible`

Purge deletes records once they have been hidden for a while, so the lifecycle says which records are hidden:

```jadpo check project=e-lifecycle-purge-visible expect=data.lifecycle_purge_visible
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        transitions: {
            delete: {
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

```text output
error: `Todo`'s lifecycle purges records but hides none
 --> main.jadpo:13:9
   |
13 |         purge: {
   |         ^^^^^ no `visible:` above
  reason: Purge permanently deletes records once they have been hidden for a while, so the lifecycle must say which records are hidden, with `visible:`.
  next: Add `visible: <condition>` to the lifecycle (agent)
  rule: data.lifecycle_purge_visible
  help: docs/language/05-data.html#data-lifecycle-purge-visible

check failed: 1 error, 1 file
```
