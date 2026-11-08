# `data.lifecycle_transition_name`

A lifecycle's transition is named after an operation the entity already has or one of its fields, or isn't in lower_snake_case.

## Why Jadpo refuses it

A transition is called like an operation of its entity,
`Todo.archive(todo)`, so its name has to be free. `get`, `create` and
`update` are operations every entity has, and `Todo.archived_at(value)`
builds a value of that field's type. A person's `erase` and `export`, and
an audited entity's `history`, are taken the same way. `delete` is the one
operation a transition may take over.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-transition-name expect=data.lifecycle_transition_name
entity Todo {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            update: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: A transition can't be called `update`
 --> main.jadpo:7:13
  |
7 |             update: {
  |             ^^^^^^ name in use
  reason: `Todo.update` is already an operation every entity has, so a transition can't take its name; `delete` is the one a transition may take over.
  next: Name the transition after the change it makes, such as `archive` (agent)
  rule: data.lifecycle_transition_name
  help: https://jadpo.dev/docs/errors/data.lifecycle_transition_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A transition named after a field is refused the same way:

```jadpo check project=e-transition-field expect=data.lifecycle_transition_name
entity Todo {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            archived_at: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: A transition can't be called `archived_at`
 --> main.jadpo:7:13
  |
7 |             archived_at: {
  |             ^^^^^^^^^^^ name in use
  reason: `Todo.archived_at(value)` builds a value of the field's type, so a transition can't share the field's name.
  next: Name the transition after the change it makes, such as `archive` (agent)
  rule: data.lifecycle_transition_name
  help: https://jadpo.dev/docs/errors/data.lifecycle_transition_name

check failed: 1 error, 1 file
```

So is a name that isn't in lower_snake_case:

```jadpo check project=e-transition-case expect=data.lifecycle_transition_name
entity Todo {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            Archive: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: A transition can't be called `Archive`
 --> main.jadpo:7:13
  |
7 |             Archive: {
  |             ^^^^^^^ name in use
  reason: A transition is called like an operation, `Todo.archive(record)`, so its name is lower_snake_case.
  next: Name the transition after the change it makes, such as `archive` (agent)
  rule: data.lifecycle_transition_name
  help: https://jadpo.dev/docs/errors/data.lifecycle_transition_name

check failed: 1 error, 1 file
```

## How to fix it

**If the transition has a name that is taken**, name it after the change
it makes:

```jadpo fragment
            archive: {
                set: {
                    archived_at: clock.now
                }
            }
```

```jadpo check project=e-transition-name-fix hidden
entity Todo {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            archive: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
```

Code now runs it as `Todo.archive(todo)`.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
- [Transitions](../language/06-reads-and-writes.md#transitions)
