# `migration.backfill_can_fail`

A migration fills a field with a backfill that can fail.

## Why Jadpo refuses it

A backfill gives every record already stored a value, in the middle of a
migration. A record whose value failed would have none, and nothing could
say what it should be, so nothing in a backfill may fail. A construction
that could break the field's rules, or a call that could fail with nothing
handling it, is refused before the migration runs.

## What it looks like

A title can be empty or longer than five characters, so its length can
break `Rank`'s `min` or `max`:

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

`database/migrations/20261020T1401_add_rank/changes.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value can be kept in range**, keep it there with operations that
can't fail, so the construction is proven safe:

```jadpo check project=e-backfill-can-fail-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-can-fail-fix file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    title: Text
}
```

```jadpo check project=e-backfill-can-fail-fix file=domains/planning/types/rank.jadpo hidden
type Rank = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-backfill-can-fail-fix file=domains/planning/entities/todo.jadpo hidden migrate=add_rank
use planning/types { Rank }

entity Todo {
    id: Uuid
    title: Text
    rank: Rank
}
```

```jadpo check project=e-backfill-can-fail-fix file=database/migrations/20261020T1401_add_rank/changes.jadpo migrate=finish
migration {
    Todo.rank: {
        backfill: Rank(integer.max(1, integer.min(5, text.length(Todo.title))))
    }
}
```

Every stored `Todo` gets a rank between 1 and 5, and the backfill can't
fail. Then run `jadpo migrate new` to finish the migration.

**If every stored record can start with the same value**, write it:

```jadpo check project=e-backfill-can-fail-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-can-fail-fix-2 file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    title: Text
}
```

```jadpo check project=e-backfill-can-fail-fix-2 file=domains/planning/types/rank.jadpo hidden
type Rank = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-backfill-can-fail-fix-2 file=domains/planning/entities/todo.jadpo hidden migrate=add_rank
use planning/types { Rank }

entity Todo {
    id: Uuid
    title: Text
    rank: Rank
}
```

```jadpo check project=e-backfill-can-fail-fix-2 file=database/migrations/20261020T1401_add_rank/changes.jadpo migrate=finish
migration {
    Todo.rank: {
        backfill: 1
    }
}
```

Every stored `Todo` gets rank 1, a value `Rank` allows. Run
`jadpo migrate new` to finish the migration.

## Related

- [Computed backfills](../language/05-data.md#computed-backfills)
- [What a backfill may read](../language/05-data.md#what-a-backfill-may-read)
- [Migrations](../language/05-data.md#migrations)
