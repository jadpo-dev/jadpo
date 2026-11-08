# `data.lifecycle_value`

A lifecycle's `set:` or `initial:` gives a field something other than a fixed value.

## Why Jadpo refuses it

A transition takes no parameters and knows nothing of who is asking, so it
changes a record the same way wherever it runs. Each field is set to a
fixed value: a literal, an enum value, `none` or `clock.now`. A field kept
whole as JSON is only ever set to `none`, which is the column's `NULL`,
because any other value is written through its type's JSON form by an
action.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-value expect=data.lifecycle_value
entity Todo {
    id: Uuid
    deleted_at: Instant?
    edited_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                set: {
                    deleted_at: edited_at
                }
            }
        }
    }
}
```

```text output
error: `edited_at` is another field of the record
 --> main.jadpo:11:33
   |
11 |                     deleted_at: edited_at
   |                                 ^^^^^^^^^ a field, not a value
  reason: A transition sets each field to a fixed value (a literal, an enum value, `none` or `clock.now`), the same every time it runs; copying one field into another isn't one.
  next: Use a literal, an enum value, `none` or `clock.now` (agent)
  rule: data.lifecycle_value
  help: https://jadpo.dev/docs/errors/data.lifecycle_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value worked out from something else is refused the same way:

```jadpo check project=e-lifecycle-value-worked-out expect=data.lifecycle_value
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                set: {
                    deleted_at: clock.now + 1d
                }
            }
        }
    }
}
```

```text output
error: `clock.now + 1d` isn't a fixed value
 --> main.jadpo:10:33
   |
10 |                     deleted_at: clock.now + 1d
   |                                 ^^^^^^^^^^^^^^ worked out, or from outside the record
  reason: A transition sets fixed values: a literal, an enum value, `none` or `clock.now`. It takes no parameters and knows nothing of who is asking, so the same change happens wherever it runs.
  next: Use a literal, an enum value, `none` or `clock.now` (agent)
  rule: data.lifecycle_value
  help: https://jadpo.dev/docs/errors/data.lifecycle_value

check failed: 1 error, 1 file
```

So is any value but `none` for a field kept as JSON:

```jadpo check project=e-lifecycle-value-json expect=data.lifecycle_value
type Tag = Text {
    max_length: 20
}

type Label = Object {
    line: Tag
}

entity Todo {
    id: Uuid
    label: Label?

    lifecycle: {
        transitions: {
            label_it: {
                set: {
                    label: Label(line: "urgent")
                }
            }
        }
    }
}
```

```text output
error: `label` is stored as JSON
 --> main.jadpo:17:28
   |
17 |                     label: Label(line: "urgent")
   |                            ^^^^^^^^^^^^^^^^^^^^^ set as JSON
  reason: `Todo.label` is stored whole as its JSON in one `jsonb` column (area 5, "Storage types"), so a lifecycle sets it only to `none`, which is the column's `NULL`; any other value is written by an action's write, through the route codec.
  next: Set `label` to `none`, or in an action's write (agent)
  rule: data.lifecycle_value
  help: https://jadpo.dev/docs/errors/data.lifecycle_value

check failed: 1 error, 1 file
```

## How to fix it

**If the transition records when it ran**, set the field to `clock.now`:

```jadpo fragment
            delete: {
                set: {
                    deleted_at: clock.now
                }
            }
```

```jadpo check project=e-lifecycle-value-fix-1 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?
    edited_at: Instant?

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
```

`Todo.delete` now hides the todo and records when.

**If the value depends on the record or the request**, set it in an action
instead, with an `update`, and take the field out of the lifecycle:

```jadpo fragment
action hide(todo: Todo) -> Todo {
    return Todo.update(todo) {
        set: {
            deleted_at: todo.edited_at
        }
    }
}
```

```jadpo check project=e-lifecycle-value-fix-2 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?
    edited_at: Instant?
}

action hide(todo: Todo) -> Todo {
    return Todo.update(todo) {
        set: {
            deleted_at: todo.edited_at
        }
    }
}
```

The action can now use any value it has worked out.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
- [Structured values](../language/05-data.md#structured-values)
