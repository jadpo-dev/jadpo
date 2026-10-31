# `deliveries.unavailable`

The app's deliveries couldn't be read from their database, because it can't be reached or has no table for them yet.

## Why Jadpo refuses it

`jadpo deliveries` lists and acts on the app's event deliveries and
schedule runs, which Jadpo keeps in tables of their own in the app's
database. Those tables come with the project's migrations. When the
database can't be reached, or has no such table, there is nothing to
list or act on, so the command stops before it reads or changes
anything.

Jadpo won't apply the migrations for you here or try another address.
The database may be production's, and whether it is the right one only
you know.

## What it looks like

A project where adding a note emits `NoteAdded`, which the subscriber
`share_added` delivers:

```jadpo check project=e-deliveries-unavailable file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-deliveries-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-deliveries-unavailable file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-deliveries-unavailable file=domains/notes/entities/note.jadpo hidden
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

```jadpo check project=e-deliveries-unavailable file=domains/notes/events/note_added.jadpo hidden
use notes/entities { Note }

/// A note was written down.
type NoteAdded = Event {
    note_id: Note.id
}
```

```jadpo check project=e-deliveries-unavailable file=domains/notes/actions/add_note.jadpo hidden
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

```jadpo check project=e-deliveries-unavailable file=domains/notes/actions/share_note.jadpo hidden
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

```jadpo check project=e-deliveries-unavailable file=domains/notes/subscribers/share_added.jadpo migrate=initial
use notes/actions { share_note }
use notes/events { NoteAdded }

/// Shares each note once it is written down.
subscriber share_added {
    on: NoteAdded
    run: share_note(event.note_id)
}
```

`jadpo deliveries` against a new database, to which the project's migrations
haven't been applied:

```sh run project=e-deliveries-unavailable database expect=deliveries.unavailable
jadpo deliveries
```

```text output
error: The database `main` has no table for deliveries yet
  reason: `jadpo_deliveries` comes with the project's migrations, so either the database hasn't applied them, or it isn't this project's database.
  next: Run `jadpo migrate apply` if it is this project's database, then run the command again (human)
  rule: deliveries.unavailable
  help: https://jadpo.dev/docs/errors/deliveries.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the command reads, and whether its schema changes.

A database whose server isn't running gives the same error, saying it
can't be reached. Here `DATABASE_URL`, the variable the connection's
setting reads, names a database on this machine with no server running:

```sh run project=e-deliveries-unavailable expect=deliveries.unavailable
DATABASE_URL=postgresql://localhost:5499/app jadpo deliveries
```

```text output
error: The database `main` can't be reached
  reason: Connecting to it failed, so no deliveries could be read.
  next: Check the database is running and its address is right, then run the command again (human)
  rule: deliveries.unavailable
  help: https://jadpo.dev/docs/errors/deliveries.unavailable
```

## How to fix it

The question is whether the address names this project's database.

**If it is this project's database, not yet migrated**, apply the
migrations, then run the command again:

```sh
jadpo migrate apply
jadpo deliveries
```

The tables are made, and the command reads them.

**If its server isn't running**, start the server, then run the command
again:

```sh
jadpo deliveries
```

The command reaches the database and reads it.

**If the address names another database than the one you meant**, such
as production's, name that one with `--url`:

```sh
jadpo deliveries --url "$PRODUCTION_DATABASE_URL"
```

The command reads that database instead, and leaves the other alone.

## Related

- [`jadpo deliveries`](../language/14-tooling.md#jadpo-deliveries)
- [What an operator sees](../language/12-jobs-and-events.md#what-an-operator-sees)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)
- [Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls)
