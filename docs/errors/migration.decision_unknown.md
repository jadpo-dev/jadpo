# `migration.decision_unknown`

A migration decides about something that isn't changing.

## Why Jadpo refuses it

`jadpo migrate new` works out what a migration changes and lists the
decisions it needs. A decision about something the migration doesn't
change does nothing, and whoever reads it would think it had: a typo in a
field's name, or a decision left over from a change that has since been
undone.

## What it looks like

Nothing in this migration changes `Note.body`:

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

`database/migrations/20261020T1401_add_stars/changes.jadpo`:

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
  reason: A migration's `changes.jadpo` holds only the decisions its own changes need, and nothing `20261020T1401_add_stars` changes about `Note.body` does: a decision is a value for existing rows of a required field or for rows a field no longer allows, retiring a field or an entity that is gone, or dropping one that is retired.
  next: Remove `Note.body` (agent)
  rule: migration.decision_unknown
  help: https://jadpo.dev/docs/errors/migration.decision_unknown

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Dropping a field that is still there is this error with a message of its
own, since a drop takes a migration of its own, which writes the line for
you:

```jadpo check project=e-decision-unknown-drop file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-unknown-drop file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
    body: Text
}
```

```jadpo check project=e-decision-unknown-drop file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-unknown-drop file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    body: Text
    stars: Stars
}
```

`database/migrations/20261020T1401_add_stars/changes.jadpo`:

```jadpo check project=e-decision-unknown-drop file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_unknown
migration {
    Note.stars: {
        backfill: 3
    }
    Note.body: FieldChange.dropped
}
```

```text output
error: `Note.body` isn't dropped by this migration
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:5:5
  |
5 |     Note.body: FieldChange.dropped
  |     ^^^^^^^^^ no such change
  reason: `20261020T1401_add_stars` still has `Note.body` in its snapshot, so nothing here drops it: a drop comes from a migration of its own, which writes this line, its snapshot and its SQL.
  next: Remove the line, then run `jadpo migrate new <purpose> --drop Note.body` once the migration is finished (agent)
  rule: migration.decision_unknown
  help: https://jadpo.dev/docs/errors/migration.decision_unknown

check failed: 1 error, 4 files
```

## How to fix it

**If the decision isn't needed**, remove it:

```jadpo check project=e-decision-unknown-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-unknown-fix file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-unknown-fix file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-unknown-fix file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-unknown-fix file=database/migrations/20261020T1401_add_stars/changes.jadpo migrate=finish
migration {
    Note.stars: {
        backfill: 3
    }
}
```

`changes.jadpo` holds only what this migration decides. Run
`jadpo migrate new` to finish it.

**If you want to remove the field**, take the line out and finish this
migration first. Removing a field takes two later migrations, one retiring
it and one dropping it, and the second is written for you once the field
is retired:

```sh
jadpo migrate new remove_note_body --drop Note.body
```

The drop gets a migration of its own, which writes its decision, snapshot
and SQL together.

## Related

- [Migrations](../language/05-data.md#migrations)
- [Removing data](../language/05-data.md#removing-data)
