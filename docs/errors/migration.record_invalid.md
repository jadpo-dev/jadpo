# `migration.record_invalid`

A snapshot edited by hand:

```jadpo check project=e-record-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-record-invalid file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-record-invalid file=database/migrations/20261020T1400_initial/snapshot.json expect=migration.record_invalid
{"format": 1}
```

```text output
error: `database/migrations/20261020T1400_initial/snapshot.json` can't be read
 --> database/migrations/20261020T1400_initial/snapshot.json
  reason: A migration's records are written by `jadpo migrate new` and never edited by hand; this one can't be used: it has no `stores` object.
  next: Restore it from version control (human)
  rule: migration.record_invalid
  help: docs/language/05-data.html#migration-record-invalid

check failed: 1 error, 2 files
```
