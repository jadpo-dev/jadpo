# `data.lifecycle_section`

A lifecycle, a transition or a `purge:` has a part it doesn't take, lacks one it needs, or gives one twice.

## Why Jadpo refuses it

A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`, and
has at least `visible:` or `transitions:`, since without either it would
only stop its records being deleted. A transition holds `when:` and
`set:`, and sets at least one field. `purge:` holds both `after:` and
`since:`. Each part is given once, and so is each transition and each
field in `set:` or `initial:`, so there is never a question of which one
counts.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-section expect=data.lifecycle_section
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        hidden: deleted_at != none
    }
}
```

```text output
error: `hidden` isn't part of a lifecycle
 --> main.jadpo:7:9
  |
7 |         hidden: deleted_at != none
  |         ^^^^^^ not a section here
  reason: A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`.
  next: Remove `hidden` from the lifecycle (agent)
  rule: data.lifecycle_section
  help: https://jadpo.dev/docs/errors/data.lifecycle_section

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A part given twice is refused the same way:

```jadpo check project=e-lifecycle-twice expect=data.lifecycle_section
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        visible: deleted_at != none
    }
}
```

```text output
error: `visible:` is given twice
 --> main.jadpo:7:9
  |
6 |         visible: deleted_at == none
  |         ------- first given here
7 |         visible: deleted_at != none
  |         ^^^^^^^ given again
  reason: Each part of a lifecycle is given once, so there is never a question of which counts.
  next: Remove one `visible:` (agent)
  rule: data.lifecycle_section
  help: https://jadpo.dev/docs/errors/data.lifecycle_section

check failed: 1 error, 1 file
```

So is a part that is needed and missing, here `purge:`'s `since:`:

```jadpo check project=e-lifecycle-missing expect=data.lifecycle_section
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30d
        }
    }
}
```

```text output
error: The purge has no `since:`
 --> main.jadpo:7:9
  |
7 |         purge: {
  |         ^^^^^ no `since:`
  reason: `purge:` says how long a hidden record is kept, `after:`, counted from one of its time fields, `since:`.
  next: Add `since: ...` (agent)
  rule: data.lifecycle_section
  help: https://jadpo.dev/docs/errors/data.lifecycle_section

check failed: 1 error, 1 file
```

So is a lifecycle with neither `visible:` nor `transitions:`:

```jadpo check project=e-lifecycle-neither expect=data.lifecycle_section
type TodoStage = Enum {
    open
    done
}

entity Todo {
    id: Uuid
    stage: TodoStage

    lifecycle: {
        initial: {
            stage: TodoStage.open
        }
    }
}
```

```text output
error: `Todo`'s lifecycle hides nothing and changes nothing
 --> main.jadpo:10:5
   |
10 |     lifecycle: {
   |     ^^^^^^^^^ no `visible:` or `transitions:`
  reason: A lifecycle says which records reads see (`visible:`) or the named changes its fields go through (`transitions:`); one with neither would only stop the records being deleted.
  next: Add `visible:` or `transitions:`, or remove `lifecycle:` (agent)
  rule: data.lifecycle_section
  help: https://jadpo.dev/docs/errors/data.lifecycle_section

check failed: 1 error, 1 file
```

So is a transition whose `set:` gives no fields:

```jadpo check project=e-lifecycle-empty-set expect=data.lifecycle_section
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        transitions: {
            delete: {
                set: {}
            }
        }
    }
}
```

```text output
error: The transition `delete`'s `set:` gives no fields
 --> main.jadpo:8:17
  |
8 |                 set: {}
  |                 ^^^ nothing to change
  reason: A transition is a named change, so it sets at least one field.
  next: Add the fields it changes to `set:` (agent)
  rule: data.lifecycle_section
  help: https://jadpo.dev/docs/errors/data.lifecycle_section

check failed: 1 error, 1 file
```

## How to fix it

**If the part isn't one the lifecycle takes, or is given twice**, remove
it:

```jadpo fragment
    lifecycle: {
        visible: deleted_at == none
    }
```

```jadpo check project=e-lifecycle-section-fix-1 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
    }
}
```

Reads now see only the todos that have no `deleted_at`.

**If a part is missing**, add it:

```jadpo fragment
        purge: {
            after: 30d
            since: deleted_at
        }
```

```jadpo check project=e-lifecycle-section-fix-2 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

Purge now knows how long to keep a hidden record, and from when.

**If a transition is meant to change something**, name the fields in its
`set:`:

```jadpo fragment
            delete: {
                set: {
                    deleted_at: clock.now
                }
            }
```

```jadpo check project=e-lifecycle-section-fix-3 hidden
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

`Todo.delete` now records when the todo was deleted.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
