# `migration.decision_invalid`

A migration's decision doesn't fit the change it is about.

## Why Jadpo refuses it

`changes.jadpo` holds the decisions a migration needs. Each is read against
what the migration changes: a value for a new required field has to be a
value of that field's type, and retiring a field is written
`FieldChange.retired`. A decision that doesn't fit would put a value in the
database the entity doesn't allow, or do something other than the change
needs.

## What it looks like

`Note.stars` allows 1 to 5, and the decision gives 9:

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

`database/migrations/20261020T1401_add_stars/changes.jadpo`:

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
  help: https://jadpo.dev/docs/errors/migration.decision_invalid

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A decision of the wrong kind is this error too, such as a value for a field
that is gone, which takes `FieldChange.retired`. The error says which
decision the change takes.

## How to fix it

**If the value is wrong**, give one its type allows:

```jadpo check project=e-decision-invalid-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-invalid-fix file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-invalid-fix file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-invalid-fix file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-invalid-fix file=database/migrations/20261020T1401_add_stars/changes.jadpo migrate=finish
migration {
    Note.stars: {
        backfill: 5
    }
}
```

Every stored `Note` gets 5 stars. Run `jadpo migrate new` to finish the
migration.

## Related

- [Migrations](../language/05-data.md#migrations)
- [Computed backfills](../language/05-data.md#computed-backfills)
- [`migration.decision_required`](../errors/migration.decision_required.md)
