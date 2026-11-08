# `data.lifecycle_purge_visible`

A lifecycle has a `purge:` but no `visible:` saying which records are hidden.

## Why Jadpo refuses it

Purge permanently deletes records once they have been hidden for a while.
Without `visible:` no record is ever hidden, so purge would have nothing
it was allowed to delete, and the lifecycle wouldn't say what it was for.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.lifecycle_purge_visible

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If deleted records should drop out of view**, add `visible:`:

```jadpo fragment
    lifecycle: {
        visible: deleted_at == none
        ...
    }
```

```jadpo check project=e-purge-visible-fix-1 hidden
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
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

Reads now leave out a deleted todo, and purge removes it 30 days later.

**If no record should ever be removed**, remove `purge:`:

```jadpo fragment
    lifecycle: {
        transitions: {
            ...
        }
    }
```

```jadpo check project=e-purge-visible-fix-2 hidden
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
    }
}
```

`Todo.delete` now only sets `deleted_at`, and every record stays.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
