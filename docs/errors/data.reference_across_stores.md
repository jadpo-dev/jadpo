# `data.reference_across_stores`

A field refers to a record of an entity that is kept in a different database.

## Why Jadpo refuses it

A reference such as `owner_id: User.id` becomes a foreign key, and a
database keeps foreign keys only between its own tables. Here `OldTodo`
is in the `archive` database and `User` is in `main`, so neither database
could check that the user exists or apply `on_delete:`.

Jadpo won't choose for you between moving one of the entities and giving
up the reference. Moving decides which database holds the records, and
giving up the reference means nothing keeps the two in step.

## What it looks like

app.jadpo:

```jadpo check project=e-across-stores file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-across-stores file=domains/planning/entities/user.jadpo hidden
entity User {
    id: Uuid
}
```

domains/planning/entities/old_todo.jadpo:

```jadpo check project=e-across-stores file=domains/planning/entities/old_todo.jadpo expect=data.reference_across_stores
use planning/entities { User }

entity OldTodo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    store: archive
}
```

```text output
error: `OldTodo.owner_id` refers to a `User` in another store
 --> domains/planning/entities/old_todo.jadpo:5:5
  |
5 |     owner_id: User.id {
  |     ^^^^^^^^ `OldTodo` is in `archive`
  reason: A reference is a foreign key, which a database keeps only between its own tables; an action writes one store, and work across stores goes through an event and a subscriber.
  next: Keep `OldTodo` and `User` in one store, or store the id as a plain `Uuid` (human)
  rule: data.reference_across_stores
  help: https://jadpo.dev/docs/errors/data.reference_across_stores

check failed: 1 error, 3 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database keeps old todos, and whether each one stays tied
to its user.

## How to fix it

The question is whether an old todo must stay tied to a real user.

**If it must**, keep `OldTodo` in the same store as `User`. Delete the
`store: archive` line, so `OldTodo` lives in `main`.
`domains/planning/entities/old_todo.jadpo` becomes:

```jadpo check project=e-across-stores-fix-1 file=domains/planning/entities/old_todo.jadpo
use planning/entities { User }

entity OldTodo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
}
```

```jadpo check project=e-across-stores-fix-1 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-across-stores-fix-1 file=domains/planning/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
}
```

The foreign key works again: an old todo must belong to a user that
exists, and deleting the user deletes their old todos.

**If the archive should keep its own records**, store the user's id as a
plain `Uuid`, with no `on_delete:`:

```jadpo fragment
    owner_id: Uuid
```

```jadpo check project=e-across-stores-fix-2 file=domains/planning/entities/old_todo.jadpo hidden
entity OldTodo {
    id: Uuid
    owner_id: Uuid

    store: archive
}
```

```jadpo check project=e-across-stores-fix-2 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-across-stores-fix-2 file=domains/planning/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
}
```

Old todos stay in `archive`. `owner_id` is now only a value, so nothing
checks that the user exists, and deleting a user leaves their old todos
as they are.

## Related

- [Stores](../language/05-data.md#stores)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
- [One store per action](../language/06-reads-and-writes.md#one-store-per-action)
