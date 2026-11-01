# `migration.drop_not_accepted`

A migration removes data, and no acceptance in the project says a person agreed to it.

## Why Jadpo refuses it

A drop can't be undone: the column or table goes, with every value in it,
when the migration runs, and no build brings it back. Retiring an entity
isn't gone yet, but it takes its records out of the reach of the app, of
erasure and of retention. So each needs a person, and a person's decision
is an acceptance with a reason.

On a branch, `jadpo check` reports a removal the change adds as
[`acceptance.missing`](../errors/acceptance.missing.md). This error is what
stands behind that, for wherever there is no branch to compare: `jadpo check`
with no before side, and every command about to run the migrations, whatever
git says.

## What it looks like

`Todo.legacy_flag` was retired by an earlier migration, and a later one
drops it:

```jadpo check project=e-drop-not-accepted file=app.jadpo hidden
application {}
```

```jadpo check project=e-drop-not-accepted file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    legacy_flag: Bool
}
```

```jadpo check project=e-drop-not-accepted file=domains/planning/entities/todo.jadpo hidden migrate=retire_legacy_flag
entity Todo {
    id: Uuid
}
```

```jadpo check project=e-drop-not-accepted file=database/migrations/20261020T1401_retire_legacy_flag/changes.jadpo hidden migrate=finish
migration {
    Todo.legacy_flag: FieldChange.retired
}
```

```sh run project=e-drop-not-accepted expect=migration.drop_not_accepted
jadpo migrate new drop_legacy_flag --drop Todo.legacy_flag > /dev/null
jadpo migrate apply
```

```text output
error: Migration `20261020T1402_drop_legacy_flag` removes data nobody accepted: Todo.legacy_flag
 --> database/migrations/20261020T1402_drop_legacy_flag/changes.jadpo:4:5
  |
4 |     Todo.legacy_flag: FieldChange.dropped
  |     ^^^^^^^^^^^^^^^^ DataChange.column_dropped
  reason: `Todo.legacy_flag` is dropped for good from store `main`: column `todo.legacy_flag` (`boolean`) and every value in it. So a person accepts it first, and no acceptance with a reason lists `Todo.legacy_flag: DataChange.column_dropped`.
  next: A person writes an acceptance in `acceptances/` listing `Todo.legacy_flag: DataChange.column_dropped`, with why on a `///` line above `acceptance` (human)
  rule: migration.drop_not_accepted
  help: https://jadpo.dev/docs/errors/migration.drop_not_accepted

migrate apply stopped: 1 error
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether every `legacy_flag` already stored is lost for good.

## How to fix it

The question is whether the data should go.

**If it should go**, write down why. `jadpo acceptance new <purpose>`
writes the draft on a branch, or write the file yourself, listing what the
message names, with your reason on the `///` line above `acceptance`:

```jadpo fragment
// acceptances/20261020T1500_drop_legacy_flag.jadpo
/// Nothing has read the legacy flag since 3.2, and support never asks for it.
acceptance {
    changes: {
        Todo.legacy_flag: DataChange.column_dropped
    }
}
```

```jadpo check project=e-drop-not-accepted-fix file=acceptances/20261020T1500_drop_legacy_flag.jadpo hidden
// acceptances/20261020T1500_drop_legacy_flag.jadpo
/// Nothing has read the legacy flag since 3.2, and support never asks for it.
acceptance {
    changes: {
        Todo.legacy_flag: DataChange.column_dropped
    }
}
```

```jadpo check project=e-drop-not-accepted-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-drop-not-accepted-fix file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
}
```

Then `jadpo migrate apply` runs the migration. The acceptance is committed
with the change, and the pull request's review is where an owner of
`acceptances/` approves it.

**If it shouldn't go**, delete the migration's folder, as long as it hasn't
run anywhere:

```sh
rm -r database/migrations/20261020T1402_drop_legacy_flag
```

The field stays retired, its column and data kept, and nothing more needs
deciding.

## Related

- [Removing data](../language/05-data.md#removing-data)
- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [`acceptance.missing`](../errors/acceptance.missing.md)
