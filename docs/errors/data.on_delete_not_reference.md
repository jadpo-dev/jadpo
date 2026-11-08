# `data.on_delete_not_reference`

A field that isn't a reference has `on_delete:`.

## Why Jadpo refuses it

`on_delete:` says what happens to a record when the record it refers to is
deleted, so it only means something on a field typed `X.id`. On any other
field there is nothing that can be deleted out from under it.

## What it looks like

main.jadpo:

```jadpo check project=e-on-delete-not-reference expect=data.on_delete_not_reference
entity Note {
    id: Uuid
    title: Text {
        on_delete: OnDelete.cascade
    }
}
```

```text output
error: `Note.title` isn't a reference
 --> main.jadpo:4:20
  |
4 |         on_delete: OnDelete.cascade
  |                    ^^^^^^^^^^^^^^^^ nothing to delete
  reason: `on_delete:` says what deleting a referenced record does, so it belongs only on a field typed `X.id`.
  next: Remove `on_delete:` (agent)
  rule: data.on_delete_not_reference
  help: https://jadpo.dev/docs/errors/data.on_delete_not_reference

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field isn't a link to another record**, remove `on_delete:`:

```jadpo check project=e-on-delete-not-reference-fix-1
entity Note {
    id: Uuid
    title: Text
}
```

`title` is now plain text.

**If the field should link to another record**, type it as a reference:

```jadpo fragment
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
```

```jadpo check project=e-on-delete-not-reference-fix-2 hidden
entity User {
    id: Uuid
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
}
```

Deleting a user now deletes their notes too.

## Related

- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
