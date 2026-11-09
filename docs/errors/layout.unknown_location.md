# `layout.unknown_location`

A file is somewhere the project layout has no place for.

## Why Jadpo refuses it

A project has one layout, so that a file's path follows from what it holds
and an agent knows where to look. A domain holds a fixed set of folders, and
each is named after what its files declare. A file anywhere else would be
code the project's other rules can't place.

## What it looks like

`helpers/` isn't one of a domain's folders.

```jadpo check project=e-location file=app.jadpo hidden
application {}
```

`domains/planning/helpers/due_dates.jadpo`:

```jadpo check project=e-location file=domains/planning/helpers/due_dates.jadpo expect=layout.unknown_location
type Days = Int {
    min: 0
}

library due_dates {
    function days_left(days: Days) -> Days {
        return days
    }
}
```

```text output
error: `helpers/` isn't a folder a domain has
 --> domains/planning/helpers/due_dates.jadpo
  reason: A domain holds only `entities/`, `types/`, `failures/`, `actions/`, `queries/`, `libraries/`, `routes/`, `schedules/`, `events/`, `subscribers/`, `emails/`, `translations/`, `prompts/` and `fixtures/`, each named after what its files declare.
  next: Move the file to `domains/planning/libraries/due_dates.jadpo` (agent)
  rule: layout.unknown_location
  help: https://jadpo.dev/docs/errors/layout.unknown_location

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`shared/` has a fixed set of folders too:

```jadpo check project=e-location-shared file=app.jadpo hidden
application {}
```

`shared/helpers/due_dates.jadpo`:

```jadpo check project=e-location-shared file=shared/helpers/due_dates.jadpo expect=layout.unknown_location
type Days = Int {
    min: 0
}

library due_dates {
    function days_left(days: Days) -> Days {
        return days
    }
}
```

```text output
error: `shared/helpers/` isn't part of `shared/`
 --> shared/helpers/due_dates.jadpo
  reason: `shared/` holds only `types/`, `libraries/` and `failures/`: code every domain may use.
  next: Move the file to `shared/libraries/due_dates.jadpo` (agent)
  rule: layout.unknown_location
  help: https://jadpo.dev/docs/errors/layout.unknown_location

check failed: 1 error, 2 files
```

A subfolder inside a fixed folder, a file directly in `shared/`, and a file
in the root `tests/` folder that isn't a `.test.jadpo` file are refused the
same way, as are a package's stray files.

## How to fix it

**If the file's contents belong in the folder the error names**, move it
there: `domains/planning/libraries/due_dates.jadpo`.

```jadpo check project=e-location-fix file=domains/planning/libraries/due_dates.jadpo
type Days = Int {
    min: 0
}

library due_dates {
    function days_left(days: Days) -> Days {
        return days
    }
}
```

```jadpo check project=e-location-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-location-fix file=domains/planning/actions/show_days.jadpo hidden
use planning/libraries { due_dates }

/// Stops early when no days are left.
action show_days() -> Unit {
    if (due_dates.days_left(3) > 0) {
        return
    }
}
```

The library is now where a domain's libraries live, and other files in
the domain can import it.

## Related

- [The layout in detail](../language/01-project-layout-and-modules.md#the-layout-in-detail)
- [Domains](../language/01-project-layout-and-modules.md#domains)
