# `deliveries.wrong_state`

A delivery or schedule run was asked to do something its state rules out, such as cancelling one that has already failed.

## Why Jadpo refuses it

Each action of `jadpo deliveries` applies to items in some states only:
`retry` and `dismiss` to one that failed, `cancel` to one still waiting
to run, and `resolve` to one whose outside call may or may not have
happened. An action that doesn't apply to an item as it is would do
nothing, or the wrong thing: cancelling a delivery that already failed
stops nothing. So nothing is done, and the error says what state the
item is in and what applies to it.

Jadpo won't do the action that applies instead: whether a failed
delivery is sent again or set aside decides what the app does for the
person it was for.

## What it looks like

A project where adding a note emits `NoteAdded`, which the subscriber
`share_added` delivers to `share_note`. Until there is a team to share
with, `share_note` fails, and `share_added` tries once:

```jadpo check project=e-deliveries-wrong-state file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-deliveries-wrong-state file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-deliveries-wrong-state file=domains/notes/types/note_body.jadpo hidden
/// What a note says.
type NoteBody = Text {
    max_length: 500
}
```

```jadpo check project=e-deliveries-wrong-state file=domains/notes/failures/no_team.jadpo hidden
type NoTeam = Failure {
    kind: FailureKind.unavailable
    message: "There's no team to share notes with yet."
}
```

```jadpo check project=e-deliveries-wrong-state file=domains/notes/entities/note.jadpo hidden
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

```jadpo check project=e-deliveries-wrong-state file=domains/notes/events/note_added.jadpo hidden
use notes/entities { Note }

/// A note was written down.
type NoteAdded = Event {
    note_id: Note.id
}
```

```jadpo check project=e-deliveries-wrong-state file=domains/notes/actions/add_note.jadpo hidden
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

```jadpo check project=e-deliveries-wrong-state file=domains/notes/actions/share_note.jadpo hidden
use notes/entities { Note }
use notes/failures { NoTeam }

/// Shares a note with the team, once there is one.
action share_note(note_id: Note.id) fails NoTeam -> Unit {
    reject NoTeam
}
```

domains/notes/subscribers/share_added.jadpo:

```jadpo check project=e-deliveries-wrong-state file=domains/notes/subscribers/share_added.jadpo
use notes/actions { share_note }
use notes/events { NoteAdded }

/// Shares each note once it is written down.
subscriber share_added {
    on: NoteAdded
    run: share_note(event.note_id)
    retry: {
        attempts: 1
    }
}
```

```jadpo check project=e-deliveries-wrong-state file=database/seeds/demo.jadpo hidden migrate=initial
use notes

/// A note, for trying the app.
seed {
    notes.add_note("Buy milk")
}
```

Each delivery's id is random, so a run prints a different id every time
and can't be checked; this output is copied from a real run. It is
`jadpo deliveries` on a Mac, with `DATABASE_URL` naming a new PostgreSQL
database after `jadpo migrate apply` and the seed `demo` wrote a note,
whose delivery failed, then `cancel` on that delivery:

```sh
jadpo deliveries
jadpo deliveries cancel dc0f8b4c
```

```text
  id        item                     state     last tried (UTC)
  dc0f8b4c  NoteAdded → share_added  x failed  21:06:21 · no_team

deliveries need attention: 1 failed

next: jadpo deliveries retry dc0f8b4c  sends it again, once the cause is fixed
error: `dc0f8b4c` isn't pending, so there is nothing to cancel
  reason: Its delivery of `NoteAdded` to `share_added` failed. Cancel stops a delivery, or a run asked for by hand, that is waiting to run.
  next: Run `jadpo deliveries retry dc0f8b4c`, or `jadpo deliveries dismiss dc0f8b4c` (human)
  rule: deliveries.wrong_state
  help: https://jadpo.dev/docs/errors/deliveries.wrong_state
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the note is shared after all.

## How to fix it

The question is whether the delivery should go again.

**If it should, once its cause is fixed**, retry it:

```sh
jadpo deliveries retry dc0f8b4c
```

The delivery is pending again, with a fresh retry budget, and the app's
next tick sends it.

**If it shouldn't go at all**, dismiss it:

```sh
jadpo deliveries dismiss dc0f8b4c
```

`share_added` never receives that `NoteAdded`, and the delivery is
listed under `jadpo deliveries --state dismissed`, with who dismissed it.

## Related

- [`jadpo deliveries`](../language/14-tooling.md#jadpo-deliveries)
- [What an operator sees](../language/12-jobs-and-events.md#what-an-operator-sees)
- [Retries and giving up](../language/12-jobs-and-events.md#retries-and-giving-up)
- [`deliveries.not_found`](deliveries.not_found.md)
