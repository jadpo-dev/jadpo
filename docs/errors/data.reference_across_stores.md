# `data.reference_across_stores`

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
  help: docs/language/05-data.html#data-reference-across-stores

check failed: 1 error, 3 files
```
