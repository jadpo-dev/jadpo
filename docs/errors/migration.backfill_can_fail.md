# `migration.backfill_can_fail`

A title can be empty or longer than five characters, so its length can
break `Rank`'s `min` or `max`, and nothing says what such a record's rank
would be:

```jadpo check project=e-backfill-can-fail file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-can-fail file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    title: Text
}
```

```jadpo check project=e-backfill-can-fail file=domains/planning/types/rank.jadpo hidden
type Rank = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-backfill-can-fail file=domains/planning/entities/todo.jadpo hidden migrate=add_rank
use planning/types { Rank }

entity Todo {
    id: Uuid
    title: Text
    rank: Rank
}
```

```jadpo check project=e-backfill-can-fail file=database/migrations/20261020T1401_add_rank/changes.jadpo expect=migration.backfill_can_fail
migration {
    Todo.rank: {
        backfill: Rank(text.length(Todo.title))
    }
}
```

```text output
error: The backfill for `Todo.rank` can fail
 --> database/migrations/20261020T1401_add_rank/changes.jadpo:3:19
  |
3 |         backfill: Rank(text.length(Todo.title))
  |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can break `Rank`'s `min` and `max`
  reason: A backfill gives every `Todo` record already stored a value, and a record whose value failed would have none, so nothing in a backfill may fail.
  next: Work the value out with operations that can't fail, such as `integer.min` and `integer.max` to keep a number in its range (agent)
  rule: migration.backfill_can_fail
  help: https://jadpo.dev/docs/errors/migration.backfill_can_fail

check failed: 1 error, 4 files
```

Kept in its range with `integer.min` and `integer.max`, the construction
can't fail, and the backfill runs.
