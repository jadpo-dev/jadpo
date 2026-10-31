# `migration.apply_failed`

Applying the project's migrations to a database stopped at a step that failed.

## Why Jadpo refuses it

`jadpo migrate apply` runs each pending migration in a transaction of
its own, as a deploy step. When a step fails, because the database
can't be reached, PostgreSQL refuses a statement, or a backfill stops at
a record, that step is rolled back and nothing after it runs. The
migrations before it stay applied, as does every batch of a backfill
that committed, and the next run carries on from there, so a failed run
never leaves a migration half done.

Jadpo won't skip the step or carry on without it: the migrations after
it, and the code you deploy, are written for the schema it makes. What
stopped it is a database, an address or data only you can put right.

## What it looks like

A project with one migration, whose `DATABASE_URL`, the variable the
connection's setting reads, names a database whose server isn't
running:

```jadpo check project=e-apply-failed file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-apply-failed file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-apply-failed file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-apply-failed file=domains/notes/entities/note.jadpo hidden migrate=initial
use notes/types { NoteBody }

/// Something to remember.
entity Note {
    id: Uuid
    body: NoteBody
}
```
```sh run project=e-apply-failed expect=migration.apply_failed
DATABASE_URL=postgresql://localhost:5499/notes jadpo migrate apply
```

```text output
error: Migrating `main` failed
  reason: The database can't be reached. The step that failed was rolled back; the migrations before it stay applied, as does every batch of a backfill that committed, and the next run carries on from there.
  next: Fix what stopped it, then run `jadpo migrate apply` again (human)
  rule: migration.apply_failed
  help: https://jadpo.dev/docs/errors/migration.apply_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the migrations go into, and whether it is ready
for them.

The same error names the migration and the field when the step that
failed is a backfill: one that stopped at a record, or stored values a
changed type no longer reads, each with what to change before applying
again. `jadpo seed` applies pending migrations before it runs, and
stops with this error when one fails.

## How to fix it

The question is what stopped the step, which the `reason:` line says.

**If the database's server isn't running**, start it, then apply
again:

```sh
jadpo migrate apply
```

The step that failed runs again, then every migration after it.

**If the address is wrong**, give the connection's setting the right
one, on this machine with `jadpo config set`, or in the environment of
the deploy:

```sh
jadpo config set database_url
```

`jadpo migrate apply` reads the address from the environment, then
`.env.local`, and applies the pending migrations there.

## Related

- [Migrations](../language/05-data.md#migrations)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)
- [Computed backfills](../language/05-data.md#computed-backfills)
- [Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls)
