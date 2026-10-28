# `data.store_unknown`

```jadpo check project=e-store-unknown file=app.jadpo hidden
application {}
```

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
  help: docs/errors/data.store_unknown.md

check failed: 1 error, 2 files
```
