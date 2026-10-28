# `migration.unfinished`

The decision is written, but the SQL was written before it:

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
  help: docs/language/05-data.html#migration-unfinished

check failed: 1 error, 4 files
```
