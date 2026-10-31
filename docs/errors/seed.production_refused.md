# `seed.production_refused`

A seed was run against a database that isn't on this machine, which may be production.

## Why Jadpo refuses it

A seed is data for development: a few records to try the app with,
made by calling your app's own actions. Data a production database
needs comes from a migration's backfill, which every database gets the
same way, in order, once. So `jadpo seed` runs only against a database
on this machine, reached through a Unix socket, `localhost`,
`127.0.0.1` or `::1`, and refuses any other before it runs anything,
since one elsewhere may hold real people's data.

Jadpo won't decide that a database elsewhere is safe to seed. Only you
know whether it is a server you can throw away.

## What it looks like

A project whose seed `demo` writes a few notes:

```jadpo check project=e-seed-production file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-seed-production file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-seed-production file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-seed-production file=domains/notes/failures/note_exists.jadpo hidden
type NoteExists = Failure {
    kind: FailureKind.conflict
    message: "That note is already written down."
}
```

```jadpo check project=e-seed-production file=domains/notes/entities/note.jadpo hidden
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

```jadpo check project=e-seed-production file=domains/notes/actions/add_note.jadpo hidden
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

```jadpo check project=e-seed-production file=database/seeds/demo.jadpo migrate=initial
use notes

/// A few notes, for trying the app.
seed {
    attempt notes.add_note("Buy milk")
    attempt notes.add_note("Book the dentist")
}
```

`DATABASE_URL`, the variable the connection's setting reads, names a
database on another server, as it might after copying a production
address into the environment:

```sh run project=e-seed-production expect=seed.production_refused
DATABASE_URL=postgresql://app@db.example.com/notes jadpo seed demo
```

```text output
error: The database `main` at db.example.com isn't on this machine
  reason: A seed is data for development, so `jadpo seed` runs only against a database on this machine: a Unix socket, `localhost`, `127.0.0.1` or `::1`. One anywhere else may be production, whose data comes from a migration backfill. Nothing was run.
  next: Give the connection's setting a database on this machine with `jadpo config set`, or start `jadpo dev`, which starts one (human)
  rule: seed.production_refused
  help: https://jadpo.dev/docs/errors/seed.production_refused
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the seed's records go into.

The address is shown by its host, never with a password, and an address
given with `--url` is refused the same way.

## How to fix it

The question is which database on this machine the records are for.

**If they are for the database `jadpo dev` starts**, leave
`DATABASE_URL` unset and run the seed while `jadpo dev` is running in
another terminal:

```sh
unset DATABASE_URL
jadpo seed demo
```

The seed finds the database `jadpo dev` started, applies its pending
migrations and writes the notes there.

**If you keep a development database of your own**, store its address
for this machine:

```sh
jadpo config set database_url
```

Type an address on this machine, such as
`postgresql://localhost/notes_dev`, at its prompt. It is saved in
`.env.local`, and `jadpo seed demo` reads it from there.

## Related

- [Seeds](../language/05-data.md#seeds)
- [`jadpo seed`](../language/14-tooling.md#jadpo-seed)
- [PostgreSQL over TLS](../language/05-data.md#postgresql-over-tls)
- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
