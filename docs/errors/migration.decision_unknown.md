# `migration.decision_unknown`

```jadpo check project=e-decision-unknown file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-unknown file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-unknown file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-unknown file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-unknown file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_unknown
migration {
    Note.stars: {
        backfill: 3
    }
    Note.body: FieldChange.retired
}
```

```text output
error: Nothing about `Note.body` needs a decision
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:5:5
  |
5 |     Note.body: FieldChange.retired
  |     ^^^^^^^^^ no such change
  reason: A migration's `changes.jadpo` holds only the decisions its own changes need, and nothing `20261020T1401_add_stars` changes about `Note.body` does: a decision is a value for existing rows of a required field or for rows a field no longer allows, or retiring a field that is gone.
  next: Remove `Note.body` (agent)
  rule: migration.decision_unknown
  help: docs/language/05-data.html#migration-decision-unknown

check failed: 1 error, 4 files
```
