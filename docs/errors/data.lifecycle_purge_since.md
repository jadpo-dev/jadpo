# `data.lifecycle_purge_since`

A lifecycle's `purge:` has a `since:` that isn't one of the record's `Instant` fields.

## Why Jadpo refuses it

Purge deletes a record once it has been hidden for `after:`, counted from
a moment the record holds. `since:` names that moment, so it has to be one
of the record's own `Instant` fields, named bare as a lifecycle names
every field. Anything else gives purge no time to count from.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Anything but a field's bare name is refused the same way, including the
field written with its entity:

```jadpo check project=e-purge-since-qualified expect=data.lifecycle_purge_since
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30d
            since: Todo.deleted_at
        }
    }
}
```

```text output
error: `since: Todo.deleted_at` isn't a field
 --> main.jadpo:9:20
  |
9 |             since: Todo.deleted_at
  |                    ^^^^^^^^^^^^^^^ not a field
  reason: `since:` names the `Instant` field of the record that purge counts from, such as the one the hiding transition sets.
  next: Name an `Instant` field of the record (agent)
  rule: data.lifecycle_purge_since
  help: https://jadpo.dev/docs/errors/data.lifecycle_purge_since

check failed: 1 error, 1 file
```

## How to fix it

**If purge counts from when the record was hidden**, name the `Instant`
field that records it:

```jadpo fragment
        purge: {
            after: 30d
            since: deleted_at
        }
```

```jadpo check project=e-purge-since-fix hidden
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
            since: deleted_at
        }
    }
}
```

A hidden todo is now deleted for good 30 days after `deleted_at`.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
