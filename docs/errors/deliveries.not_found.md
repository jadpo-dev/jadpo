# `deliveries.not_found`

The id given to `jadpo deliveries` matches no delivery or schedule run in the database.

## Why Jadpo refuses it

`retry`, `dismiss`, `cancel`, `resolve`, `show` and `abandon` act on one
delivery or schedule run, named by the last 8 hex digits of its id, as
`jadpo deliveries` lists it. When nothing in the database has an id
ending that way, there is nothing to act on. The id may be mistyped, the
item may be gone (a delivery every subscriber has had is deleted after
24 hours, and a schedule's run after 30 days), or the command may be
reading another database than the one the id came from.

Jadpo won't act on the nearest id instead: retrying or dismissing the
wrong delivery changes what the app does for someone.

## What it looks like

A project where adding a note emits `NoteAdded`, which the subscriber
`share_added` delivers:

```jadpo check project=e-deliveries-not-found file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-deliveries-not-found file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-deliveries-not-found file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-deliveries-not-found file=domains/notes/entities/note.jadpo hidden
use notes/types { NoteBody }

/// Something to remember.
entity Note {
    id: Uuid
    body: NoteBody
    shared: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
```

```jadpo check project=e-deliveries-not-found file=domains/notes/events/note_added.jadpo hidden
use notes/entities { Note }

/// A note was written down.
type NoteAdded = Event {
    note_id: Note.id
}
```

```jadpo check project=e-deliveries-not-found file=domains/notes/actions/add_note.jadpo hidden
use notes/entities { Note }
use notes/types { NoteBody }
use notes/events { NoteAdded }

/// Writes a note down.
action add_note(body: NoteBody) -> Unit {
    var note: Note = Note.create {
        set: {
            body: body
            shared: false
        }
    }
    emit NoteAdded {
        note_id: note.id
    }
}
```

```jadpo check project=e-deliveries-not-found file=domains/notes/actions/share_note.jadpo hidden
use notes/entities { Note, NoteNotFound }

/// Shares a note with the team.
action share_note(note_id: Note.id) fails NoteNotFound -> Unit {
    attempt Note.update(note_id) {
        set: {
            shared: true
        }
    }
}
```

domains/notes/subscribers/share_added.jadpo:

```jadpo check project=e-deliveries-not-found file=domains/notes/subscribers/share_added.jadpo migrate=initial
use notes/actions { share_note }
use notes/events { NoteAdded }

/// Shares each note once it is written down.
subscriber share_added {
    on: NoteAdded
    run: share_note(event.note_id)
}
```

`jadpo deliveries retry` with an id that nothing in the database has:

```sh run project=e-deliveries-not-found database expect=deliveries.not_found
jadpo migrate apply
jadpo deliveries retry 0a1b2c3d
```

```text output
Applied 1 migration to `main`
    20261020T1400_initial  applied
error: No delivery or schedule run has the id `0a1b2c3d`
  reason: `jadpo deliveries` lists those that need an operator, and `--state` the others; a delivery whose event every subscriber had is deleted after 24 hours, and a schedule's run after 30 days.
  next: Run `jadpo deliveries` to see them, with their ids (human)
  rule: deliveries.not_found
  help: https://jadpo.dev/docs/errors/deliveries.not_found
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which delivery is sent again.

## How to fix it

The question is whether the command reads the database the id came from.

**If it does**, list what is there, with its ids, and use the id shown:

```sh
jadpo deliveries
jadpo deliveries --state pending
```

`jadpo deliveries` lists what failed or is in doubt; `--state` lists
the others, one state at a time.

**If the id came from another database**, name that one with `--url`:

```sh
jadpo deliveries retry 0a1b2c3d --url "$PRODUCTION_DATABASE_URL"
```

The command finds the item in that database and acts on it there.

## Related

- [`jadpo deliveries`](../language/14-tooling.md#jadpo-deliveries)
- [What an operator sees](../language/12-jobs-and-events.md#what-an-operator-sees)
- [`deliveries.wrong_state`](deliveries.wrong_state.md)
