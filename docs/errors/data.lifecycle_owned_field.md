# `data.lifecycle_owned_field`

A create, update or patch sets a field that only the entity's lifecycle may change.

## Why Jadpo refuses it

A lifecycle owns every field its transitions set and every field its
`initial:` gives. Those fields change only through the transitions, so a
record always moves between its stages as the lifecycle says: a `when:`
is checked, and every field the transition sets changes together.

Here `hide_todo` sets `deleted_at` with a plain update. That skips the
`delete` transition, so anything the transition adds later, such as a
`when:` or a second field, would be skipped too.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-owned-field expect=data.lifecycle_owned_field
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
    }
}

action hide_todo(todo: Todo) -> Todo {
    return Todo.update(todo) {
        set: {
            deleted_at: clock.now
        }
    }
}
```

```text output
error: `Todo.update` sets `deleted_at`, which only `Todo`'s lifecycle changes
 --> main.jadpo:20:13
   |
20 |             deleted_at: clock.now
   |             ^^^^^^^^^^ owned by the lifecycle
  reason: `Todo`'s lifecycle owns `deleted_at`: only its transition `delete` changes it, so the record's stages always move as the lifecycle says.
  next: Remove `deleted_at`, and call `Todo.delete(todo)` to change it (agent)
  rule: data.lifecycle_owned_field
  help: https://jadpo.dev/docs/errors/data.lifecycle_owned_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A create that gives a field `initial:` gives is refused too. Here every
new story starts as a draft, and `start_story` tries to start one in
review:

main.jadpo:

```jadpo check project=e-lifecycle-owned-initial expect=data.lifecycle_owned_field
type HeadlineText = Text {
    max_length: 120
}

type ReviewStage = Enum {
    draft
    in_review
}

entity Story {
    id: Uuid
    headline: HeadlineText
    stage: ReviewStage

    lifecycle: {
        initial: {
            stage: ReviewStage.draft
        }
        transitions: {
            submit: {
                when: stage == ReviewStage.draft
                set: {
                    stage: ReviewStage.in_review
                }
            }
        }
    }
}

action start_story(headline: Story.headline) -> Story {
    return Story.create {
        set: {
            headline: headline
            stage: ReviewStage.in_review
        }
    }
}
```

```text output
error: `Story.create` sets `stage`, which only `Story`'s lifecycle changes
 --> main.jadpo:34:13
   |
34 |             stage: ReviewStage.in_review
   |             ^^^^^ owned by the lifecycle
  reason: `Story`'s lifecycle owns `stage`: only its transition `submit` changes it, so the record's stages always move as the lifecycle says. A new record gets it from `initial:`, or `none` when the lifecycle gives none.
  next: Remove `stage` here (agent)
  rule: data.lifecycle_owned_field
  help: https://jadpo.dev/docs/errors/data.lifecycle_owned_field

check failed: 1 error, 1 file
```

## How to fix it

**If the update is the stage change a transition makes**, call the
transition instead. `Todo.delete` runs the `delete` transition and gives
nothing back:

```jadpo fragment
action hide_todo(todo: Todo) -> Unit {
    Todo.delete(todo)
}
```

```jadpo check project=e-lifecycle-owned-field-fix-1 hidden
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
    }
}

action hide_todo(todo: Todo) -> Unit {
    Todo.delete(todo)
}
```

Hiding a todo now goes through `delete`, so it sets `deleted_at` exactly
as the lifecycle says, and the todo drops out of every read.

**If a create gives a field `initial:` gives**, remove that line:

```jadpo fragment
    return Story.create {
        set: {
            headline: headline
        }
    }
```

```jadpo check project=e-lifecycle-owned-initial-fix-2 hidden
type HeadlineText = Text {
    max_length: 120
}

type ReviewStage = Enum {
    draft
    in_review
}

entity Story {
    id: Uuid
    headline: HeadlineText
    stage: ReviewStage

    lifecycle: {
        initial: {
            stage: ReviewStage.draft
        }
        transitions: {
            submit: {
                when: stage == ReviewStage.draft
                set: {
                    stage: ReviewStage.in_review
                }
            }
        }
    }
}

action start_story(headline: Story.headline) -> Story {
    return Story.create {
        set: {
            headline: headline
        }
    }
}
```

Every new story starts as a draft. It moves to review only when
`Story.submit` runs its `submit` transition.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
- [Transitions](../language/06-reads-and-writes.md#transitions)
- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
