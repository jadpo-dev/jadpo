# `migration.store_diverged`

The migrations a database has applied don't match the project's: one has changed since it ran, the project doesn't have one, or one comes out of order.

## Why Jadpo refuses it

A database's `jadpo_migrations` table records each migration it has
applied, with a digest of it. `jadpo migrate apply` compares that
record with the project's migrations before it runs anything. When they
disagree, the database holds what other SQL did, and applying this
project's migrations on top could undo a change, repeat one, or build on
a schema they weren't written for. So nothing is applied.

Jadpo won't rewrite the record or the database to make them agree.
Which is right, the database or the project, only you know.

## What it looks like

A project with one migration, applied to a new database, whose SQL is
then changed by hand:

```jadpo check project=e-store-diverged file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-store-diverged file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-store-diverged file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-store-diverged file=domains/notes/entities/note.jadpo hidden migrate=initial
use notes/types { NoteBody }

/// Something to remember.
entity Note {
    id: Uuid
    body: NoteBody
}
```
```sh run project=e-store-diverged database expect=migration.store_diverged
jadpo migrate apply
echo "-- tidied by hand" >> database/migrations/20261020T1400_initial/postgres.sql
jadpo migrate apply
```

```text output
Applied 1 migration to `main`
    20261020T1400_initial  applied
error: `20261020T1400_initial` changed after `main` applied it
  reason: A migration's SQL is never changed once it has run anywhere: the database holds what the old SQL did.
  next: Restore `20261020T1400_initial` as it was when it ran, and make the change in a new migration (human)
  rule: migration.store_diverged
  help: https://jadpo.dev/docs/errors/migration.store_diverged
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what the database's schema is from here on.

A database that has applied a migration the project doesn't have, as
one migrated by a newer version of the code has, gives the same error
naming that migration. Here the project's second migration is applied,
then its folder is removed, as checking out an older commit would:

```jadpo check project=e-store-diverged-unknown file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-store-diverged-unknown file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-store-diverged-unknown file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-store-diverged-unknown file=domains/notes/entities/note.jadpo hidden migrate=initial
use notes/types { NoteBody }

/// Something to remember.
entity Note {
    id: Uuid
    body: NoteBody
}
```
```jadpo check project=e-store-diverged-unknown file=domains/notes/entities/note.jadpo hidden migrate=add_pinned
use notes/types { NoteBody }

/// Something to remember.
entity Note {
    id: Uuid
    body: NoteBody
    pinned: Bool?
}
```

```sh run project=e-store-diverged-unknown database expect=migration.store_diverged
jadpo migrate apply
rm -r database/migrations/20261020T1401_add_pinned
jadpo migrate apply
```

```text output
Applied 2 migrations to `main`
    20261020T1400_initial     applied
    20261020T1401_add_pinned  applied
error: `main` has applied `20261020T1401_add_pinned`, which this project doesn't have
  reason: The database was migrated by another version of the project, so applying this one's migrations could undo or repeat its changes.
  next: Bring the project up to date with the code that migrated it, then apply again (human)
  rule: migration.store_diverged
  help: https://jadpo.dev/docs/errors/migration.store_diverged
```

A migration added before one the database has already applied, as
merging two branches can, is refused the same way: it would run after
the later one, on a schema it wasn't written for.

## How to fix it

The question is which is right: the database, or the project.

**If the migration was changed by mistake**, restore it as it was when
it ran:

```sh
git restore database/migrations/20261020T1400_initial/postgres.sql
jadpo migrate apply
```

The database is up to date again. A change you meant to make goes in a
new migration, which `jadpo migrate new` writes.

**If the database was migrated by a newer version of the code**, bring
the project up to date with that code, then apply again:

```sh
git pull
jadpo migrate apply
```

The project has every migration the database applied, and only the
ones after them run.

**If two branches each added a migration**, keep the one the database
has applied, remove the other, and write its change again after it:

```sh
jadpo migrate new add_pinned
```

The change is a new migration, named after the latest, so it runs on
the schema the database has.

## Related

- [Migrations](../language/05-data.md#migrations)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)
- [`migration.record_invalid`](migration.record_invalid.md)
