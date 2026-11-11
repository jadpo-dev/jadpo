# `event.emit_invalid`

Something other than an action emits, or what is emitted isn't an event or an email.

## Why Jadpo refuses it

An emitted event is saved in the same database transaction as an
action's writes, and handed to its subscribers only once that
transaction commits; if the action fails, the event is thrown away. A
function, a query, a test, a fixture or a seed has no transaction to
save it in, so only an action emits. A function is pure, so it returns
the event for the action that calls it to emit; a test, a fixture or a
seed emits by calling an action. What `emit` is given must be an event, a type with the
`Event` base, or an email to send, since only those have a contract its
subscribers or the mail service can read.

## What it looks like

main.jadpo:

```jadpo check project=e-emit-invalid expect=event.emit_invalid
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

entity Sighting {
    id: Uuid
    note: Note

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
    }
}

function seen(note: Note) -> NoteSeen {
    emit NoteSeen {
        note: note
    }
    return NoteSeen {
        note: note
    }
}

action glance(note: Note) -> Sighting {
    var sighting: Sighting = Sighting.create {
        set: {
            note: note
        }
    }
    var announced: NoteSeen = seen(note)
    assert announced.note == note
    return sighting
}
```

```text output
error: A function can't emit `NoteSeen`
 --> main.jadpo:20:5
   |
20 |     emit NoteSeen {
   |     ^^^^^^^^^^^^^^^ only actions emit
  reason: An event is kept in the same transaction as an action's writes and handed to its subscribers once it commits, so only actions emit; a function is pure, so it returns the event for the action that calls it to emit.
  next: Move the `emit` into the action that calls this function, or make this an action (agent)
  rule: event.emit_invalid
  help: https://jadpo.dev/docs/errors/event.emit_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value that isn't an event, emitted from an action, is the same error:

```jadpo check project=e-emit-invalid-value expect=event.emit_invalid
type Note = Text {
    max_length: 200
}

entity Sighting {
    id: Uuid
    note: Note

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
    }
}

action glance(note: Note) -> Sighting {
    var sighting: Sighting = Sighting.create {
        set: {
            note: note
        }
    }
    emit note
    return sighting
}
```

```text output
error: `emit` is given a `Note`
 --> main.jadpo:21:10
   |
21 |     emit note
   |          ^^^^ not an event
  reason: `emit` announces that something happened: its value is an event, a type with the `Event` base declared in a domain's `events/` folder, or an email to send, a type with the `EmailMessage` base in its `emails/` folder.
  next: Emit an event, `emit <Name> { ... }`, declaring it as `type <Name> = Event { ... }` (agent)
  rule: event.emit_invalid
  help: https://jadpo.dev/docs/errors/event.emit_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the function only builds the event**, take the `emit` out of it and
emit what it returns from the action that calls it, `glance`. In
`main.jadpo`:

```jadpo fragment
function seen(note: Note) -> NoteSeen {
    return NoteSeen {
        note: note
    }
}

action glance(note: Note) -> Sighting {
    ...
    emit seen(note)
    return sighting
}
```

```jadpo check project=e-emit-invalid-fix-1 hidden
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

entity Sighting {
    id: Uuid
    note: Note

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
    }
}

function seen(note: Note) -> NoteSeen {
    return NoteSeen {
        note: note
    }
}

action glance(note: Note) -> Sighting {
    var sighting: Sighting = Sighting.create {
        set: {
            note: note
        }
    }
    emit seen(note)
    return sighting
}
```

`seen` stays a function that anything may call. A `NoteSeen` is now
saved with the `Sighting` `glance` writes, and reaches its subscribers
only once that write commits.

**If the action means to announce something**, declare an event for it
and emit that, with the value as one of its fields. In `main.jadpo`:

```jadpo fragment
type NoteSeen = Event {
    note: Note
}

action glance(note: Note) -> Sighting {
    ...
    emit NoteSeen {
        note: note
    }
    return sighting
}
```

```jadpo check project=e-emit-invalid-fix-2 hidden
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

entity Sighting {
    id: Uuid
    note: Note

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
    }
}

action glance(note: Note) -> Sighting {
    var sighting: Sighting = Sighting.create {
        set: {
            note: note
        }
    }
    emit NoteSeen {
        note: note
    }
    return sighting
}
```

A subscriber can now listen for `NoteSeen` and be handed the note once
`glance`'s write has committed.

## Related

- [Who can emit](../language/12-jobs-and-events.md#who-can-emit)
- [Saved with the action, delivered after it](../language/12-jobs-and-events.md#saved-with-the-action-delivered-after-it)
- [Declaring an event](../language/12-jobs-and-events.md#declaring-an-event)
