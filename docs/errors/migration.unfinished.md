# `migration.unfinished`

A migration's decisions are written, but its SQL wasn't written from them.

## Why Jadpo refuses it

A migration's `postgres.sql` is written from its `changes.jadpo`. When the
decisions are written after the SQL, the SQL doesn't carry them out: applied
to a database, it wouldn't do what `changes.jadpo` says was decided. So
`jadpo check` fails until the SQL is written again from the decisions.

The SQL is never edited by hand, so `jadpo migrate new` is the only thing
that can finish it.

## What it looks like

The decision has been written in `changes.jadpo`, but the SQL was written
before it:

```jadpo check project=e-unfinished file=app.jadpo hidden
application {}
```

```jadpo check project=e-unfinished file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-unfinished file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-unfinished file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

`database/migrations/20261020T1401_add_stars/changes.jadpo`:

```jadpo check project=e-unfinished file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.unfinished
migration {
    Note.stars: {
        backfill: 3
    }
}
```

```text output
error: Migration `20261020T1401_add_stars` doesn't hold its decisions yet
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:1:1
  |
1 | migration {
  | ^^^^^^^^^ decided
  reason: Its decisions are all written in `changes.jadpo`, but `database/migrations/20261020T1401_add_stars/postgres.sql` was written before them, so it doesn't carry them out.
  next: Run `jadpo migrate new` to finish it (agent)
  rule: migration.unfinished
  help: https://jadpo.dev/docs/errors/migration.unfinished

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the decisions are what you want**, finish the migration:

```sh
jadpo migrate new
```

```jadpo check project=e-unfinished-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-unfinished-fix file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-unfinished-fix file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-unfinished-fix file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-unfinished-fix file=database/migrations/20261020T1401_add_stars/changes.jadpo hidden migrate=finish
migration {
    Note.stars: {
        backfill: 3
    }
}
```

`postgres.sql` is rewritten from the decisions, so it sets every stored
note's `stars` to 3, and `jadpo check` passes.

## Related

- [Migrations](../language/05-data.md#migrations)
- [`migration.decision_required`](../errors/migration.decision_required.md)
