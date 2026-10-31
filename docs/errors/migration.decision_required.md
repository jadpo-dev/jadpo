# `migration.decision_required`

A migration needs your decision, such as a value for records already stored, and its `changes.jadpo` doesn't give one yet.

## Why Jadpo refuses it

Here `Note` gains a required field, `stars`, and the database already
holds notes with no stars. A required column can't be added while rows
in it have no value, so the migration can't run until something says
what those notes get.

Jadpo won't make up the value, because it becomes real data on every
existing note. `jadpo migrate new` writes the migration with a line to
fill in, and `jadpo check` fails until it is filled.

## What it looks like

`Note` had only an `id` when the first migration was written. Then
`stars` was added, and `jadpo migrate new add_stars` wrote a migration
whose decision is still empty:

```jadpo check project=e-decision-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-required file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-required file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-required file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

database/migrations/20261020T1401_add_stars/changes.jadpo:

```jadpo check project=e-decision-required file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_required
migration {
    Note.stars: {}
}
```

```text output
error: Migration `20261020T1401_add_stars` needs a decision about `Note.stars`
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:2:5
  |
2 |     Note.stars: {}
  |     ^^^^^^^^^^ decide here
  reason: `Note.stars` is required, and the `Note` records already stored have no value for it.
  next: Set `backfill:` for `Note.stars`, such as `backfill: 1`, then run `jadpo migrate new` to finish the migration (human)
  rule: migration.decision_required
  help: https://jadpo.dev/docs/errors/migration.decision_required

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how many stars every note already stored gets.

## How to fix it

The question is what value the notes already stored should have.

**If every existing note should get the same value**, write it as the
field's `backfill:`. `changes.jadpo` becomes:

```jadpo check project=e-decision-required-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-required-fix-1 file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-required-fix-1 file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-required-fix-1 file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-required-fix-1 file=database/migrations/20261020T1401_add_stars/changes.jadpo migrate=finish
migration {
    Note.stars: {
        backfill: 1
    }
}
```

Then run `jadpo migrate new` with no purpose, which finishes the
migration that is waiting for its decision:

```sh
jadpo migrate new
```

```text
  finished 20261020T1401_add_stars, which migrates from 20261020T1400_initial

  Note
    + stars  smallint  new field

Wrote database/migrations/20261020T1401_add_stars/
    snapshot.json  the whole schema at this point
    postgres.sql   3 statements: add column, fill existing rows, require a value
    changes.jadpo  every decision made

next: jadpo check  confirms the migration matches the entities
```

When the migration is applied, it adds the column, sets every existing
note's `stars` to 1, and then makes the column required.

A value can also be worked out from each note's own fields, when the new
field reshapes data the note already holds
([Computed backfills](../language/05-data.md#computed-backfills)).

## Related

- [Migrations](../language/05-data.md#migrations)
- [Computed backfills](../language/05-data.md#computed-backfills)
