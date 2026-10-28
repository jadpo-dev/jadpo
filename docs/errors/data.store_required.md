# `data.store_required`

`app.jadpo` declares a database, so `main` is no longer implied:

```jadpo check project=e-store-required file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
        }
    }
}
```

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
  help: docs/language/05-data.html#data-store-required

check failed: 1 error, 2 files
```
