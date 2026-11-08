# `data.lifecycle_unknown_field`

A lifecycle names a field the entity doesn't have.

## Why Jadpo refuses it

Every field a lifecycle names is one of the record it is written in. Its
conditions become part of every statement that reads the record, so a name
that matches no column has no meaning there.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-unknown-field expect=data.lifecycle_unknown_field
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deletd_at == none
    }
}
```

```text output
error: `Todo` has no field `deletd_at`
 --> main.jadpo:6:18
  |
6 |         visible: deletd_at == none
  |                  ^^^^^^^^^ not declared
  reason: A lifecycle names the fields of the record it is written in, and `Todo`'s fields are `id` and `deleted_at`. Did you mean `deleted_at`?
  next: Use `deleted_at` instead of `deletd_at` (agent)
  rule: data.lifecycle_unknown_field
  help: https://jadpo.dev/docs/errors/data.lifecycle_unknown_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you misspelt the field**, use its name:

```jadpo fragment
        visible: deleted_at == none
```

```jadpo check project=e-lifecycle-unknown-field-fix-1 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
    }
}
```

Reads now see only the todos that have no `deleted_at`.

**If the entity doesn't have the field yet**, add it:

```jadpo fragment
entity Todo {
    id: Uuid
    archived_at: Instant?
    ...
}
```

```jadpo check project=e-lifecycle-unknown-field-fix-2 hidden
entity Todo {
    id: Uuid
    archived_at: Instant?
    deleted_at: Instant?

    lifecycle: {
        visible: archived_at == none
    }
}
```

`archived_at` is now a field of `Todo`, and the condition reads it.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
