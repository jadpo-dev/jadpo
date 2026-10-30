# `migration.decision_required`

`stars` is new and required; `jadpo migrate new add_stars` wrote the migration and left the decision to fill in:

```jadpo check project=e-decision-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-required file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-required file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-required file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-required file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_required
migration {
    Note.stars: {}
}
```

```text output
error: Migration `20261020T1401_add_stars` needs a decision about `Note.stars`
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:2:5
  |
2 |     Note.stars: {}
  |     ^^^^^^^^^^ decide here
  reason: `Note.stars` is required, and the `Note` records already stored have no value for it.
  next: Set `backfill:` for `Note.stars`, such as `backfill: 1`, then run `jadpo migrate new` to finish the migration (human)
  rule: migration.decision_required
  help: https://jadpo.dev/docs/errors/migration.decision_required

check failed: 1 error, 4 files
```
