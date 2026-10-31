# `data.retention_lifecycle_field`

A field's `retention:` would clear a field the entity's lifecycle owns or reads, changing which records are hidden or undoing what a transition set.

## Why Jadpo refuses it

A `retention:` on a field clears its value when its time is up. A
lifecycle decides which records reads see from its own fields, and only
its transitions change the fields they set. Here `deleted_at` is both:
`visible:` hides a todo once it is set, and the `delete` transition sets
it. Clearing it after 30 days wouldn't remove anything; it would bring
every deleted todo back.

Jadpo won't choose for you: whether a deleted todo is kept, and for how
long, is a decision about your users' data.

## What it looks like

main.jadpo:

```jadpo check project=e-retention-lifecycle-field expect=data.retention_lifecycle_field
config {
    deleted_todo_retention: Duration {
        default: 30d
    }
}

type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    deleted_at: Instant? {
        retention: {
            after: config.deleted_todo_retention
            since: deleted_at
        }
    }

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: Clearing `Todo.deleted_at` would change its lifecycle
 --> main.jadpo:15:9
   |
15 |         retention: {
   |         ^^^^^^^^^ clears a field of the lifecycle's
21 |     lifecycle: {
   |     --------- the lifecycle
  reason: A field's `retention:` clears its value when its time is up. `deleted_at` belongs to `Todo`'s lifecycle: its conditions read it, so clearing it would change which records are hidden or what a transition allows.
  next: Take `retention:` off `deleted_at`, and say when the record goes with `retention:` on `Todo` or the lifecycle's `purge:` (human)
  rule: data.retention_lifecycle_field
  help: https://jadpo.dev/docs/errors/data.retention_lifecycle_field

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how long a deleted todo is kept.

## How to fix it

The question is what should go when the time is up.

**If a deleted todo should go for good some time after it was deleted**,
that is the lifecycle's `purge:`, which deletes hidden records:

```jadpo fragment
entity Todo {
    ...
    deleted_at: Instant?

    lifecycle: {
        ...
        purge: {
            after: config.deleted_todo_retention
            since: deleted_at
        }
    }
}
```

```jadpo check project=e-retention-lifecycle-field-purge hidden
config {
    deleted_todo_retention: Duration {
        default: 30d
    }
}

type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: config.deleted_todo_retention
            since: deleted_at
        }
    }
}
```

A todo deleted more than 30 days ago is now removed at the next hourly
run, and one that isn't deleted stays.

**If every todo should go some time after it was made, deleted or not**,
put the retention on the entity and count from a time of its own:

```jadpo fragment
entity Todo {
    ...
    deleted_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }

    ...
    retention: {
        after: config.todo_retention
        since: created_at
    }
}
```

```jadpo check project=e-retention-lifecycle-field-entity hidden
config {
    todo_retention: Duration {
        default: 730d
    }
}

type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    deleted_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
    }

    retention: {
        after: config.todo_retention
        since: created_at
    }
}
```

Every todo made more than two years ago is now deleted at the next hourly
run, whether it was deleted or not.

## Related

- [Retention](../language/15-personal-data.md#retention)
- [Lifecycle](../language/05-data.md#lifecycle)
- [The purge schedule](../language/12-jobs-and-events.md#the-purge-schedule)
