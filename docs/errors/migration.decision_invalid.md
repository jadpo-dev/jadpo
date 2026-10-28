# `migration.decision_invalid`

```jadpo check project=e-decision-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-invalid file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-invalid file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-invalid file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-invalid file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_invalid
migration {
    Note.stars: {
        backfill: 9
    }
}
```

```text output
error: The decision about `Note.stars` doesn't fit its change
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:3:19
  |
3 |         backfill: 9
  |                   ^ doesn't fit
  reason: This value can't fill `Note.stars`: it is above the field's `max`.
  next: Write `Note.stars: { backfill: <value> }` with a value of its type (agent)
  rule: migration.decision_invalid
  help: docs/language/05-data.html#migration-decision-invalid

check failed: 1 error, 4 files
```
