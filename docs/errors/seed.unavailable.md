# `seed.unavailable`

The database a seed runs against couldn't be reached, so nothing was run.

## Why Jadpo refuses it

`jadpo seed` applies the database's pending migrations, then runs the
seed's calls against it, each committing on its own. With no database
to reach, it stops before the first call, so a seed never runs half
against one database and half against another.

Jadpo won't start a database for you here or pick another address.
Which database the records belong in, and whether its server should be
running, is yours to say.

## What it looks like

A project whose seed `demo` writes a few notes:

```jadpo check project=e-seed-unavailable file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-seed-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-seed-unavailable file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-seed-unavailable file=domains/notes/failures/note_exists.jadpo hidden
type NoteExists = Failure {
    kind: FailureKind.conflict
    message: "That note is already written down."
}
```

```jadpo check project=e-seed-unavailable file=domains/notes/entities/note.jadpo hidden
use notes/types { NoteBody }

/// Something to remember.
entity Note {
    id: Uuid
    body: NoteBody {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-seed-unavailable file=domains/notes/actions/add_note.jadpo hidden
use notes/entities { Note }
use notes/types { NoteBody }
use notes/failures { NoteExists }

/// Writes a note down.
action add_note(body: NoteBody) fails NoteExists -> Note {
    return attempt Note.create {
        set: {
            body: body
        }
        conflict: {
            Note.body: NoteExists
        }
    }
}
```

database/seeds/demo.jadpo:

```jadpo check project=e-seed-unavailable file=database/seeds/demo.jadpo migrate=initial
use notes

/// A few notes, for trying the app.
seed {
    attempt notes.add_note("Buy milk")
    attempt notes.add_note("Book the dentist")
}
```

`DATABASE_URL`, the variable the connection's setting reads, names a
database on this machine whose server isn't running:

```sh run project=e-seed-unavailable expect=seed.unavailable
DATABASE_URL=postgresql://localhost:5499/notes jadpo seed demo
```

```text output
error: The database `main` can't be reached
  reason: Connecting to it failed, so nothing was run.
  next: Check the database is running, then run `jadpo seed` again (human)
  rule: seed.unavailable
  help: https://jadpo.dev/docs/errors/seed.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database server the seed runs against.

## How to fix it

The question is whether the address is right.

**If the address is right**, start the database's server, then run the
seed again:

```sh
jadpo seed demo
```

The seed applies the pending migrations, then writes the notes.

**If the database should be the one `jadpo dev` starts**, leave
`DATABASE_URL` unset and run the seed while `jadpo dev` is running in
another terminal:

```sh
unset DATABASE_URL
jadpo seed demo
```

The seed finds the database `jadpo dev` started and writes the notes
there.

## Related

- [Seeds](../language/05-data.md#seeds)
- [`jadpo seed`](../language/14-tooling.md#jadpo-seed)
- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
- [Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls)
