# `data.store_unknown`

An entity's `store:` names something that isn't one of the app's database connections.

## Why Jadpo refuses it

`store:` says which database keeps an entity's records. It names a
connection that `app.jadpo` declares with `driver: Driver.postgres`, or
`main` when the app declares no database. A connection that keeps files or
a name nothing declares has nowhere to put a table.

## What it looks like

`app.jadpo` declares no connections:

```jadpo check project=e-store-unknown file=app.jadpo hidden
application {}
```

domains/notes/entities/note.jadpo:

```jadpo check project=e-store-unknown file=domains/notes/entities/note.jadpo expect=data.store_unknown
entity Note {
    id: Uuid

    store: archive
}
```

```text output
error: `archive` isn't a database `Note` can live in
 --> domains/notes/entities/note.jadpo:4:12
  |
4 |     store: archive
  |            ^^^^^^^ not a database
  reason: `app.jadpo` declares no connection called `archive`; an entity's `store:` names a connection with `driver: Driver.postgres`, or `main` when `app.jadpo` declares no database.
  next: Name a database connection, or declare one in `app.jadpo` (agent)
  rule: data.store_unknown
  help: https://jadpo.dev/docs/errors/data.store_unknown

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A connection that isn't a database is refused the same way. Here `sheets`
keeps files:

```jadpo check project=e-store-not-database file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/notes/sheets"
        }
    }
}
```

```jadpo check project=e-store-not-database file=domains/notes/entities/note.jadpo expect=data.store_unknown
entity Note {
    id: Uuid

    store: sheets
}
```

```text output
error: `sheets` isn't a database `Note` can live in
 --> domains/notes/entities/note.jadpo:4:12
  |
4 |     store: sheets
  |            ^^^^^^ not a database
  reason: `sheets` has no database driver; an entity's `store:` names a connection with `driver: Driver.postgres`, or `main` when `app.jadpo` declares no database.
  next: Name a database connection, or declare one in `app.jadpo` (agent)
  rule: data.store_unknown
  help: https://jadpo.dev/docs/errors/data.store_unknown

check failed: 1 error, 2 files
```

## How to fix it

**If the entity belongs in the app's main database**, remove `store:`:

```jadpo check project=e-store-unknown-fix-1 file=domains/notes/entities/note.jadpo
entity Note {
    id: Uuid
}
```

```jadpo check project=e-store-unknown-fix-1 file=app.jadpo hidden migrate=initial
application {}
```

Notes are now kept in `main`.

**If `archive` is a database the app needs**, declare it in `app.jadpo`:

```jadpo fragment
        archive: {
            driver: Driver.postgres
        }
```

```jadpo check project=e-store-unknown-fix-2 file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-store-unknown-fix-2 file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid

    store: archive
}
```

Notes are now kept in the `archive` database.

## Related

- [Stores](../language/05-data.md#stores)
