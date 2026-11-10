# `migration.backfill_outside_row`

A migration's backfill reads something other than the record it fills.

## Why Jadpo refuses it

A backfill runs once, on each record already stored, and has to give the
same value whenever and wherever the migration runs. So it works from that
record's own fields, constants and pure functions. Another entity's field,
a setting each deployment sets for itself, and a field this same migration
adds would each make the value depend on something else: which order the
records were reached in, which deployment ran the migration, or a value no
stored record has yet.

## What it looks like

A backfill reads another entity's field:

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

`database/migrations/20261020T1401_add_starred/changes.jadpo`:

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
  help: https://jadpo.dev/docs/errors/migration.backfill_outside_row

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

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

`domains/notes/libraries/draft_labels.jadpo`:

```jadpo check project=e-backfill-setting file=domains/notes/libraries/draft_labels.jadpo
use notes/types { Label }

library draft_labels {
    function default_label() -> Label {
        return config.draft_label
    }
}
```

`database/migrations/20261020T1401_add_label/changes.jadpo`:

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
  help: https://jadpo.dev/docs/errors/migration.backfill_outside_row

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

`database/migrations/20261020T1401_add_titles/changes.jadpo`:

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
  help: https://jadpo.dev/docs/errors/migration.backfill_outside_row

check failed: 1 error, 4 files
```

## How to fix it

**If every stored record can start with the same value**, write the value:

```jadpo check project=e-backfill-outside-row-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-outside-row-fix file=domains/notes/entities/note.jadpo hidden
entity Note {
    id: Uuid
    starred: Bool
}
```

```jadpo check project=e-backfill-outside-row-fix file=domains/notes/entities/draft.jadpo hidden migrate=initial
entity Draft {
    id: Uuid
}
```

```jadpo check project=e-backfill-outside-row-fix file=domains/notes/entities/draft.jadpo hidden migrate=add_starred
entity Draft {
    id: Uuid
    starred: Bool
}
```

```jadpo check project=e-backfill-outside-row-fix file=database/migrations/20261020T1401_add_starred/changes.jadpo migrate=finish
migration {
    Draft.starred: {
        backfill: false
    }
}
```

Every stored `Draft` starts unstarred. Run `jadpo migrate new` to finish
the migration.

**If the value came from a setting**, write what it should be in the
migration itself, as a value, so every deployment gets the same one:

```jadpo check project=e-backfill-setting-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-setting-fix file=domains/notes/types/label.jadpo hidden
type Label = Text {}
```

```jadpo check project=e-backfill-setting-fix file=config.jadpo hidden
use notes/types { Label }

config {
    draft_label: Label {
        default: "draft"
    }
}
```

```jadpo check project=e-backfill-setting-fix file=domains/notes/entities/draft.jadpo hidden migrate=initial
entity Draft {
    id: Uuid
}
```

```jadpo check project=e-backfill-setting-fix file=domains/notes/entities/draft.jadpo hidden migrate=add_label
use notes/types { Label }

entity Draft {
    id: Uuid
    label: Label
}
```

```jadpo check project=e-backfill-setting-fix file=database/migrations/20261020T1401_add_label/changes.jadpo migrate=finish
migration {
    Draft.label: {
        backfill: "draft"
    }
}
```

Every stored `Draft` gets the label `draft`, whichever deployment runs the
migration.

**If the value depends on a field this migration adds**, work it out from
the fields the record already had, or write a constant:

```jadpo check project=e-backfill-added-field-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-backfill-added-field-fix file=domains/notes/entities/draft.jadpo hidden migrate=initial
entity Draft {
    id: Uuid
}
```

```jadpo check project=e-backfill-added-field-fix file=domains/notes/types/title.jadpo hidden
type Title = Text {}
```

```jadpo check project=e-backfill-added-field-fix file=domains/notes/entities/draft.jadpo hidden migrate=add_titles
use notes/types { Title }

entity Draft {
    id: Uuid
    title: Title
    subheading: Title
}
```

```jadpo check project=e-backfill-added-field-fix file=database/migrations/20261020T1401_add_titles/changes.jadpo migrate=finish
migration {
    Draft.title: {
        backfill: "Untitled"
    }
    Draft.subheading: {
        backfill: "Untitled"
    }
}
```

Both new fields get a value that doesn't depend on the other.

## Related

- [What a backfill may read](../language/05-data.md#what-a-backfill-may-read)
- [Computed backfills](../language/05-data.md#computed-backfills)
- [Migrations](../language/05-data.md#migrations)
