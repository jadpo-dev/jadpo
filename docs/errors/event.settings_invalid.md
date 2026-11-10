# `event.settings_invalid`

The application's `events:` holds something other than a `store:` that names one of its databases.

## Why Jadpo refuses it

An event waits in an outbox, a table in a database, until each of its
subscribers has it. An action that writes a database keeps its events
there; `application { events: { store: } }` names the database that
keeps the events of actions that write none. A store that isn't one of
the application's databases has no outbox to keep them in, and Jadpo
won't pick another database for them.

## What it looks like

app.jadpo:

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
  help: https://jadpo.dev/docs/errors/event.settings_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If `store:` names the wrong thing**, name one of the application's
databases. In `app.jadpo`:

```jadpo fragment
    events: {
        store: main
    }
```

```jadpo check project=e-events-settings-fix-1 file=app.jadpo hidden
application {
    events: {
        store: main
    }
}
```

An action that emits and writes no database now keeps its events in
`main`'s outbox until its subscribers have them.

## Related

- [Where the event is kept](../language/12-jobs-and-events.md#where-the-event-is-kept)
- [Saved with the action, delivered after it](../language/12-jobs-and-events.md#saved-with-the-action-delivered-after-it)
- [Stores](../language/05-data.md#stores)
