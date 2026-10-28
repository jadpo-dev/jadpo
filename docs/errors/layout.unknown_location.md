# `layout.unknown_location`

```jadpo check project=e-location file=app.jadpo hidden
application {}
```

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
  help: docs/errors/layout.unknown_location.md

check failed: 1 error, 2 files
```
