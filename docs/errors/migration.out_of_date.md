# `migration.out_of_date`

The entities ask for a database schema that no migration has written yet, because they are new or have changed since the latest migration.

## Why Jadpo refuses it

Entities describe the tables you want, and migrations are the steps that
take a real database there. `jadpo check` compares the two. When they
disagree, a deploy would run code against tables that don't match it, so
the check fails at once, in the editor, the build and CI, rather than
when the code first reads a missing column.

## What it looks like

A project with an entity and no migration yet:

```jadpo check project=e-out-of-date file=app.jadpo hidden
application {}
```

domains/notes/entities/note.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Once a migration exists, changing an entity raises the same error, naming
what changed. Here `pinned` was added to `Note` after the first
migration was written:

```jadpo check project=e-out-of-date-changed file=app.jadpo hidden
application {}
```

```jadpo check project=e-out-of-date-changed file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
    body: Text
}
```

domains/notes/entities/note.jadpo:

```jadpo check project=e-out-of-date-changed file=domains/notes/entities/note.jadpo expect=migration.out_of_date
entity Note {
    id: Uuid
    body: Text
    pinned: Bool?
}
```

```text output
error: The entities have changed since the latest migration
 --> domains/notes/entities/note.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ changed
  reason: `jadpo check` compares the database schema the entities ask for with the latest migration's snapshot (`database/migrations/20261020T1400_initial/snapshot.json`): new field `Note.pinned`.
  next: Run `jadpo migrate new add_pinned` (agent)
  rule: migration.out_of_date
  help: https://jadpo.dev/docs/errors/migration.out_of_date

check failed: 1 error, 2 files
```

## How to fix it

**If there is no migration yet**, write the first one:

```sh
jadpo migrate new initial
```

```text
  compared the entities with an empty database

  Note
    + table  note                      new entity

  System tables
    + table  jadpo_circuit_breakers    system table for circuit breakers
    + table  jadpo_deliveries          system table for deliveries
    + table  jadpo_handled_deliveries  system table for handled deliveries
    + table  jadpo_housekeeping        system table for housekeeping
    + table  jadpo_idempotency_keys    system table for idempotency keys
    + table  jadpo_operator_actions    system table for operator actions
    + table  jadpo_outbox              system table for outbox

Wrote database/migrations/20261020T1400_initial/
    snapshot.json  the whole schema at this point
    postgres.sql   14 statements: create table, add unique rule, add reference, create index

next: jadpo check  confirms the migration matches the entities
```

The new folder holds the schema as the entities describe it and the SQL
that creates it. `jadpo migrate apply` runs that SQL against a database
as a deploy step.

**If the entities changed since the latest migration**, write the next
one, named after what it does:

```sh
jadpo migrate new add_pinned
```

```text
  compared the entities with 20261020T1400_initial

  Note
    + pinned  boolean  new field

Wrote database/migrations/20261020T1401_add_pinned/
    snapshot.json  the whole schema at this point
    postgres.sql   1 statement: add column

next: jadpo check  confirms the migration matches the entities
```

The migration adds the `pinned` column. A change that needs a decision,
such as a new required field, also writes a `changes.jadpo` to fill in
([`migration.decision_required`](../errors/migration.decision_required.md)).

## Related

- [Migrations](../language/05-data.md#migrations)
- [`migration.decision_required`](../errors/migration.decision_required.md)
