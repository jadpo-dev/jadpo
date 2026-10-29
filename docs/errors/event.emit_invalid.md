# `event.emit_invalid`

Only an action emits, and only an event; a function emits by calling an action:

```jadpo check project=e-emit-invalid expect=event.emit_invalid
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

function seen(note: Note) -> NoteSeen {
    emit NoteSeen {
        note: note
    }
    return NoteSeen {
        note: note
    }
}
```

```text output
error: A function can't emit `NoteSeen`
 --> main.jadpo:10:5
   |
10 |     emit NoteSeen {
   |     ^^^^^^^^^^^^^^^ only actions emit
  reason: An event is kept in the same transaction as an action's writes and handed to its subscribers once it commits, so only actions emit; a function emits by calling an action.
  next: Move the `emit` into the action this code calls, or make this an action (agent)
  rule: event.emit_invalid
  help: docs/errors/event.emit_invalid.md

check failed: 1 error, 1 file
```
