# `seed.failed`

A seed `jadpo seed` ran stopped at a call that failed or faulted, and the calls before it stay in the database.

## Why Jadpo refuses it

`jadpo seed` runs a seed's calls against the development database, each
committing on its own, as calls from the app would. When one fails or
faults, carrying on could write records that depend on the one that
didn't happen, so the seed stops there. It shows where, as a test's
failure does, and says what the earlier calls left in the database,
since they were committed and aren't undone.

## What it looks like

A project whose notes are each written down once, with a seed `demo`:

```jadpo check project=e-seed-failed file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-seed-failed file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-seed-failed file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-seed-failed file=domains/notes/failures/note_exists.jadpo hidden
type NoteExists = Failure {
    kind: FailureKind.conflict
    message: "That note is already written down."
}
```

```jadpo check project=e-seed-failed file=domains/notes/entities/note.jadpo hidden
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

```jadpo check project=e-seed-failed file=domains/notes/actions/add_note.jadpo hidden
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

```jadpo check project=e-seed-failed file=database/seeds/demo.jadpo migrate=initial
use notes

/// A few notes, for trying the app.
seed {
    attempt notes.add_note("Buy milk")
    attempt notes.add_note("Book the dentist")
}
```

Run twice, the seed's first call fails the second time, since that note
is already written down:

```sh run project=e-seed-failed database expect=seed.failed
jadpo seed demo > /dev/null
jadpo seed demo
```

```text output
error: Seed `demo` stopped with `NoteExists` from action add_note
 --> domains/notes/actions/add_note.jadpo:7:20
  |
7 |     return attempt Note.create {
  |                    ^^^^^^^^^^^^^ rejected here
  reason: Its code is `note_exists`, conflict (409): "That note is already written down." The calls were seed demo -> action add_note. What the seed's earlier calls committed stays in the database.
  next: Fix what it reports, then run `jadpo seed demo` again (agent)
  rule: seed.failed
  help: https://jadpo.dev/docs/errors/seed.failed

seed failed: `demo`
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the seed should run more than once**, have it skip a note that is
already there:

```jadpo fragment
seed {
    match notes.add_note("Buy milk") {
        success -> {}
        NoteExists -> {}
    }
    ...
}
```

```jadpo check project=e-seed-failed-again file=database/seeds/demo.jadpo hidden
use notes/failures { NoteExists }
use notes

/// A few notes, for trying the app.
seed {
    match notes.add_note("Buy milk") {
        success -> {}
        NoteExists -> {}
    }
    match notes.add_note("Book the dentist") {
        success -> {}
        NoteExists -> {}
    }
}
```

```jadpo check project=e-seed-failed-again file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-seed-failed-again file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-seed-failed-again file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-seed-failed-again file=domains/notes/failures/note_exists.jadpo hidden
type NoteExists = Failure {
    kind: FailureKind.conflict
    message: "That note is already written down."
}
```

```jadpo check project=e-seed-failed-again file=domains/notes/entities/note.jadpo hidden
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

```jadpo check project=e-seed-failed-again file=domains/notes/actions/add_note.jadpo hidden migrate=initial
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

A second run of `jadpo seed demo` then leaves the notes as they are and
succeeds.

## Related

- [`jadpo seed`](../language/14-tooling.md#jadpo-seed)
- [Seeds](../language/05-data.md#seeds)
