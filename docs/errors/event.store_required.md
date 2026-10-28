# `event.store_required`

An action that emits and writes no store has nowhere to keep its events until `events: { store: }` names one:

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
  help: docs/language/12-jobs-and-events.html#event-store-required

check failed: 1 error, 1 file
```
