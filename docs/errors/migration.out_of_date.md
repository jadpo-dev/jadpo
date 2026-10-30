# `migration.out_of_date`

A project with an entity and no migration yet:

```jadpo check project=e-out-of-date file=app.jadpo hidden
application {}
```

```jadpo check project=e-out-of-date file=domains/notes/entities/note.jadpo expect=migration.out_of_date
entity Note {
    id: Uuid
    body: Text
}
```

```text output
error: No migration creates these entities' tables yet
 --> domains/notes/entities/note.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ no table yet
  reason: `jadpo check` compares the database schema the entities ask for with the migrations in `database/migrations/`, and there are none: new entity `Note`, new system table for circuit breakers, new system table for deliveries, new system table for handled deliveries, new system table for housekeeping, and 3 more.
  next: Run `jadpo migrate new initial` (agent)
  rule: migration.out_of_date
  help: https://jadpo.dev/docs/errors/migration.out_of_date

check failed: 1 error, 2 files
```
