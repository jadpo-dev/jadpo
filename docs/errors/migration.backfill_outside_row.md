# `migration.backfill_outside_row`

A backfill reads only the record it fills:

```jadpo check project=e-backfill-outside-row file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-outside-row file=domains/notes/entities/note.jadpo hidden
entity Note {
    id: Uuid
    starred: Bool
}
```

```jadpo check project=e-backfill-outside-row file=domains/notes/entities/draft.jadpo hidden migrate=initial
entity Draft {
    id: Uuid
}
```

```jadpo check project=e-backfill-outside-row file=domains/notes/entities/draft.jadpo hidden migrate=add_starred
entity Draft {
    id: Uuid
    starred: Bool
}
```

```jadpo check project=e-backfill-outside-row file=database/migrations/20261020T1401_add_starred/changes.jadpo expect=migration.backfill_outside_row
migration {
    Draft.starred: {
        backfill: Note.starred
    }
}
```

```text output
error: The backfill for `Draft.starred` can't read `Note.starred`
 --> database/migrations/20261020T1401_add_starred/changes.jadpo:3:19
  |
3 |         backfill: Note.starred
  |                   ^^^^^^^^^^^^ another entity's field
  reason: A backfill works each `Draft` record's value out from that record's own fields, constants and pure functions, so it gives the same value whenever and wherever the migration runs.
  next: Work the value out from `Draft`'s own fields, or a constant (agent)
  rule: migration.backfill_outside_row
  help: docs/errors/migration.backfill_outside_row.md

check failed: 1 error, 4 files
```

Nor does it read a setting through a function it calls, a `crypto`
operation's `key:` included, since each deployment sets its own and
`jadpo migrate apply` is given none:

```jadpo check project=e-backfill-setting file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-setting file=domains/notes/types/label.jadpo hidden
type Label = Text {}
```

```jadpo check project=e-backfill-setting file=config.jadpo hidden
use notes/types { Label }

config {
    draft_label: Label {
        default: "draft"
    }
}
```

```jadpo check project=e-backfill-setting file=domains/notes/entities/draft.jadpo hidden migrate=initial
entity Draft {
    id: Uuid
}
```

```jadpo check project=e-backfill-setting file=domains/notes/entities/draft.jadpo hidden migrate=add_label
use notes/types { Label }

entity Draft {
    id: Uuid
    label: Label
}
```

```jadpo check project=e-backfill-setting file=domains/notes/libraries/draft_labels.jadpo
use notes/types { Label }

library draft_labels {
    function default_label() -> Label {
        return config.draft_label
    }
}
```

```jadpo check project=e-backfill-setting file=database/migrations/20261020T1401_add_label/changes.jadpo expect=migration.backfill_outside_row
migration {
    Draft.label: {
        backfill: draft_labels.default_label()
    }
}
```

```text output
error: The backfill for `Draft.label` can't read `config.draft_label`
 --> database/migrations/20261020T1401_add_label/changes.jadpo:3:19
  |
3 |         backfill: draft_labels.default_label()
  |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ reads a setting, through `draft_labels.default_label`
  reason: A backfill gives each `Draft` record the value its own fields give, whenever and wherever the migration runs, and `draft_labels.default_label` reads `config.draft_label`, which each deployment sets for itself.
  next: Work the value out without `config.draft_label`: from `Draft`'s own fields, or a constant (agent)
  rule: migration.backfill_outside_row
  help: docs/errors/migration.backfill_outside_row.md

check failed: 1 error, 6 files
```

Nor a field the same migration adds, which no record already stored has
a value for while the backfill runs:

```jadpo check project=e-backfill-added-field file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-added-field file=domains/notes/entities/draft.jadpo hidden migrate=initial
entity Draft {
    id: Uuid
}
```

```jadpo check project=e-backfill-added-field file=domains/notes/types/title.jadpo hidden
type Title = Text {}
```

```jadpo check project=e-backfill-added-field file=domains/notes/entities/draft.jadpo hidden migrate=add_titles
use notes/types { Title }

entity Draft {
    id: Uuid
    title: Title
    subheading: Title
}
```

```jadpo check project=e-backfill-added-field file=database/migrations/20261020T1401_add_titles/changes.jadpo expect=migration.backfill_outside_row
migration {
    Draft.title: {
        backfill: "Untitled"
    }
    Draft.subheading: {
        backfill: Draft.title
    }
}
```

```text output
error: The backfill for `Draft.subheading` can't read `Draft.title`
 --> database/migrations/20261020T1401_add_titles/changes.jadpo:6:19
  |
6 |         backfill: Draft.title
  |                   ^^^^^^^^^^^ a field this migration adds
  reason: `Draft.title` is added by this same migration, so no `Draft` record already stored has a value for it while the backfill runs.
  next: Work the value out from the fields `Draft` had before this migration, or a constant (agent)
  rule: migration.backfill_outside_row
  help: docs/errors/migration.backfill_outside_row.md

check failed: 1 error, 4 files
```
