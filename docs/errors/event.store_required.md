# `event.store_required`

An action emits an event but writes no database, and the app names no database to keep its events in.

## Why Jadpo refuses it

An event waits in an outbox, a table in a database, until each of its
subscribers has it. It is written in the same transaction as the
action's own writes, so it is delivered only if they commit. An action
that writes no database has no transaction to put it in, and Jadpo won't
choose a database for it: `application { events: { store: } }` names
the one that keeps such events, even when the action reads another.

## What it looks like

main.jadpo:

```jadpo check project=e-store-required expect=event.store_required
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

action glance(note: Note) -> Unit {
    emit NoteSeen {
        note: note
    }
}
```

```text output
error: `glance` emits `NoteSeen` but writes no store to keep it in
 --> main.jadpo:10:5
   |
10 |     emit NoteSeen {
   |     ^^^^^^^^^^^^^^^ no outbox
  reason: An event is kept in the outbox of the store its action writes, in the same transaction, until its subscribers have it; `glance` writes no store, itself or through what it calls, and `app.jadpo` names none with `events: { store: }`.
  next: Add `events: { store: main }` to `application { }` in `app.jadpo` (human)
  rule: event.store_required
  help: https://jadpo.dev/docs/errors/event.store_required

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database keeps the events of actions that write none.

## How to fix it

**If a database should keep these events**, name it in
`events: { store: }` in `app.jadpo`. A single file has no `app.jadpo`,
so here the example is a project, with `glance` in a `notes` domain and
one database, `main`. In `app.jadpo`:

```jadpo fragment
    events: {
        store: main
    }
```

```jadpo check project=e-event-store-required-fix-1 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
    events: {
        store: main
    }
}
```

```jadpo check project=e-event-store-required-fix-1 file=config.jadpo hidden
config {
    main_url: PostgresUrl
}
```

```jadpo check project=e-event-store-required-fix-1 file=domains/notes/types/note.jadpo hidden
/// A short note.
type Note = Text {
    max_length: 200
}
```

```jadpo check project=e-event-store-required-fix-1 file=domains/notes/events/note_seen.jadpo hidden
use notes/types { Note }

/// Someone looked at a note.
type NoteSeen = Event {
    note: Note
}
```

```jadpo check project=e-event-store-required-fix-1 file=domains/notes/types/sighting.jadpo hidden
use notes/types { Note }

/// The note someone looked at.
type Sighting = Input {
    note: Note
}
```

```jadpo check project=e-event-store-required-fix-1 file=domains/notes/actions/glance.jadpo hidden
use notes/types { Sighting }
use notes/events { NoteSeen }

/// Records that someone looked at a note.
action glance(sighting: Sighting) -> Unit {
    emit NoteSeen {
        note: sighting.note
    }
}
```

`glance` now begins a transaction in `main` for its events, and each
`NoteSeen` waits in `main`'s outbox until its subscribers have it. Every
other action that emits and writes no database keeps its events there
too.

An action that writes a database keeps its events in that one instead,
in the same transaction, so it never needs `events: { store: }`.

## Related

- [Where the event is kept](../language/12-jobs-and-events.md#where-the-event-is-kept)
- [Saved with the action, delivered after it](../language/12-jobs-and-events.md#saved-with-the-action-delivered-after-it)
- [Stores](../language/05-data.md#stores)
