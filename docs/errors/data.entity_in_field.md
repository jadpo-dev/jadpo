# `data.entity_in_field`

A field holds a whole entity instead of a reference to one.

## Why Jadpo refuses it

An entity value is a snapshot of a record at one moment. Stored inside
another record, it would be a copy that goes out of date the moment the
original changes, and the database couldn't protect the link between them.
A field points at another record by its identity, `User.id`, which the
database keeps as a foreign key.

## What it looks like

main.jadpo:

```jadpo check project=e-entity-in-field expect=data.entity_in_field
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    keeper: User
}
```

```text output
error: `Todo.keeper` holds a whole `User`
 --> main.jadpo:7:13
  |
7 |     keeper: User
  |             ^^^^ a `User` record
  reason: An entity value is a snapshot, never stored inside another record; a field refers to a record by its identity, `User.id`, which the database keeps as a foreign key.
  next: Store `User.id` instead (agent)
  rule: data.entity_in_field
  help: https://jadpo.dev/docs/errors/data.entity_in_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A list, a set or a map holding an entity is refused the same way:

```jadpo check project=e-entity-in-list expect=data.entity_in_field
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    keepers: List<User>
}
```

```text output
error: `Todo.keepers` holds a whole `User`
 --> main.jadpo:7:14
  |
7 |     keepers: List<User>
  |              ^^^^^^^^^^ a `User` record
  reason: An entity value is a snapshot, never stored inside another record; a field refers to a record by its identity, `User.id`, which the database keeps as a foreign key.
  next: Store `User.id` instead (agent)
  rule: data.entity_in_field
  help: https://jadpo.dev/docs/errors/data.entity_in_field

check failed: 1 error, 1 file
```

## How to fix it

**If a todo points at one user**, store a reference, and say what happens
when that user is deleted:

```jadpo fragment
    keeper_id: User.id {
        on_delete: OnDelete.restricted
    }
```

```jadpo check project=e-entity-in-field-fix-1 hidden
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    keeper_id: User.id {
        on_delete: OnDelete.restricted
    }
}
```

The database now keeps a foreign key, and a user who still keeps a todo
can't be deleted.

**If a todo has several keepers**, make the link an entity of its own,
with a reference to each side:

```jadpo fragment
entity TodoKeeper {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    keeper_id: User.id {
        on_delete: OnDelete.cascade
    }

    unique: {
        one_keeper_each: [todo_id, keeper_id]
    }
}
```

```jadpo check project=e-entity-in-field-fix-2 hidden
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
}

entity TodoKeeper {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    keeper_id: User.id {
        on_delete: OnDelete.cascade
    }

    unique: {
        one_keeper_each: [todo_id, keeper_id]
    }
}
```

Each keeper of a todo is now one record, and a user can keep a todo only
once.

## Related

- [Entities and the `id`](../language/05-data.md#entities-and-the-id)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
