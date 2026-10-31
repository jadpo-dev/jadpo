# `migration.record_invalid`

A file in a migration's folder is missing, can't be read, or was changed by hand, so Jadpo can't use the migration.

## Why Jadpo refuses it

`jadpo migrate new` writes each migration's folder: `snapshot.json`, the
whole schema at that point, and `postgres.sql`, the SQL that gets there.
`jadpo check` compares the entities with the latest snapshot, and the
next migration is worked out from it. A record that can't be read breaks
both, and a guess at what it held could change a real database in a way
nobody wrote.

Jadpo won't rebuild the file for you. The right content is the one that
was committed, and that may already have been applied to a database.

## What it looks like

A project whose first migration's snapshot was edited by hand:

```jadpo check project=e-record-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-record-invalid file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

database/migrations/20261020T1400_initial/snapshot.json:

```jadpo check project=e-record-invalid file=database/migrations/20261020T1400_initial/snapshot.json expect=migration.record_invalid
{"format": 1}
```

```text output
error: `database/migrations/20261020T1400_initial/snapshot.json` can't be read
 --> database/migrations/20261020T1400_initial/snapshot.json
  reason: A migration's records are written by `jadpo migrate new` and never edited by hand; this one can't be used: it has no `stores` object.
  next: Restore it from version control (human)
  rule: migration.record_invalid
  help: https://jadpo.dev/docs/errors/migration.record_invalid

check failed: 1 error, 2 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which committed version of the migration your databases are
compared with.

## How to fix it

**If the file was committed**, bring back the committed version:

```sh
git restore database/migrations/20261020T1400_initial/snapshot.json
```

The snapshot again matches what `jadpo migrate new` wrote, so
`jadpo check` can compare the entities with it, and the next migration
starts from it.

A migration's files are never fixed by hand, since `jadpo migrate new`
is the only thing that writes them.

## Related

- [Migrations](../language/05-data.md#migrations)
- [Migrations, in Records and manifest](../language/18-records-and-manifest.md#migrations)
