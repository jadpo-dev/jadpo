# `data.on_delete_required`

A field refers to a record of another entity but doesn't say what happens to it when that record is deleted.

## Why Jadpo refuses it

A field typed `User.id` is a reference: the database keeps a foreign key
from it to the `User` it points at. When that user is deleted, something
has to happen to the records that point at them. The delete can be
refused, the records can be deleted too, or an optional reference can be
emptied.

Jadpo won't pick one for you, because there is no safe default. Refusing
can block a user from leaving, and deleting can remove data someone
wanted to keep.

## What it looks like

main.jadpo:

```jadpo check project=e-on-delete-required expect=data.on_delete_required
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id
}
```

```text output
error: `Todo.owner_id` doesn't say what deleting its `User` does
 --> main.jadpo:7:5
  |
7 |     owner_id: User.id
  |     ^^^^^^^^ refers to a `User`
  reason: Any field typed `User.id` is a reference with a foreign key, and its block must say what happens when the `User` is deleted: `OnDelete.restricted` refuses the delete, `OnDelete.cascade` deletes this record too, and `OnDelete.cleared` sets an optional reference to `none`. There is no default.
  next: Add `on_delete:` to `owner_id`'s block (human)
  rule: data.on_delete_required
  help: https://jadpo.dev/docs/errors/data.on_delete_required

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what happens to a user's todos when the user is deleted.

## How to fix it

The question is what should happen to a todo when its owner is deleted.

**If a user with todos shouldn't be deletable**, use
`OnDelete.restricted`:

```jadpo fragment
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
```

```jadpo check project=e-on-delete-required-fix-1 hidden
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
}
```

Deleting a user who still has todos fails with a conflict. The todos have
to be deleted or moved first.

**If the todos should go with their owner**, use `OnDelete.cascade`:

```jadpo fragment
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
```

```jadpo check project=e-on-delete-required-fix-2 hidden
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
}
```

Deleting a user deletes their todos in the same transaction.

**If the todos should stay, with no owner**, make the reference optional
and use `OnDelete.cleared`:

```jadpo fragment
    owner_id: User.id? {
        on_delete: OnDelete.cleared
    }
```

```jadpo check project=e-on-delete-required-fix-3 hidden
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id? {
        on_delete: OnDelete.cleared
    }
}
```

Deleting a user keeps their todos and sets each one's `owner_id` to
`none`. Every todo's owner can now be missing, so code that reads it has
to handle `none`.

## Related

- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
