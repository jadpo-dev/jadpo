# `event.settings_invalid`

`events: { store: }` names a database:

```jadpo check project=e-events-settings file=app.jadpo expect=event.settings_invalid
application {
    events: {
        store: archive
    }
}
```

```text output
error: `store: archive` isn't a database
 --> app.jadpo:3:16
  |
3 |         store: archive
  |                ^^^^^^^ not a database
  reason: An event is kept in a database's outbox until its subscribers have it; this application's databases are `main`.
  next: Name a database connection (agent)
  rule: event.settings_invalid
  help: docs/language/12-jobs-and-events.html#event-settings-invalid

check failed: 1 error, 1 file
```
