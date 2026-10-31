# `data.store_required`

An entity names no database to live in, and the app declares databases but none called `main`.

## Why Jadpo refuses it

An entity lives in the `main` database unless its file says
`store: <connection>`. With no database declared, `main` is a PostgreSQL
database you don't have to declare. Once `app.jadpo` declares databases of
its own, `main` is no longer implied, so an entity that names no store has
nowhere to go.

Jadpo won't pick one of your databases for the entity, because that
decides where its records are kept.

## What it looks like

`app.jadpo` declares one database, `archive`:

```jadpo check project=e-store-required file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
        }
    }
}
```

domains/notes/entities/note.jadpo:

```jadpo check project=e-store-required file=domains/notes/entities/note.jadpo expect=data.store_required
entity Note {
    id: Uuid
}
```

```text output
error: `Note` names no store, and there is no `main`
 --> domains/notes/entities/note.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ which store?
  reason: An entity lives in `main` unless its file says `store: <connection>`, and `app.jadpo` declares no database called `main`; its databases are `archive`.
  next: Add `store: archive` to `Note` (human)
  rule: data.store_required
  help: https://jadpo.dev/docs/errors/data.store_required

check failed: 1 error, 2 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database keeps notes.

## How to fix it

**If notes belong in `archive`**, say so in the entity.
`domains/notes/entities/note.jadpo` becomes:

```jadpo check project=e-store-required-fix-1 file=domains/notes/entities/note.jadpo
entity Note {
    id: Uuid

    store: archive
}
```

```jadpo check project=e-store-required-fix-1 file=app.jadpo hidden migrate=initial
application {
    connections: {
        archive: {
            driver: Driver.postgres
        }
    }
}
```

Notes are kept in the `archive` database.

**If notes belong in a database of their own**, declare `main` beside
`archive` in `app.jadpo`:

```jadpo fragment
        main: {
            driver: Driver.postgres
        }
```

```jadpo check project=e-store-required-fix-2 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-store-required-fix-2 file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

Notes, and every other entity that names no store, are kept in `main`.

## Related

- [Stores](../language/05-data.md#stores)
