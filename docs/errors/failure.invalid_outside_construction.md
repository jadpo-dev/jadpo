# `failure.invalid_outside_construction`

`invalid:` covers only a construction's rules, never another call's
failures or a write's conditions:

```jadpo check project=e-invalid-outside-construction expect=failure.invalid_outside_construction
type Note = Text {}

type NoteRefused = Failure {
    kind: FailureKind.rejected
}

type NoteInvalid = Failure {
    kind: FailureKind.invalid_value
}

action check_note(note: Note) fails NoteRefused -> Note {
    if (note == "") {
        reject NoteRefused
    }
    return note
}

action save_note(note: Note) fails NoteRefused, NoteInvalid -> Note {
    return attempt check_note(note) {
        invalid: NoteInvalid
    }
}
```

```text output
error: `check_note(...)` isn't a construction, so `invalid:` covers nothing
 --> main.jadpo:20:9
   |
20 |         invalid: NoteInvalid
   |         ^^^^^^^ not a construction
  reason: `check_note(...)` fails with the failures its `fails` list names; `invalid:` is the catch-all only for the rules a construction's value can break, never for another call's failures.
  next: Remove `invalid: ...`; `attempt` passes `check_note(...)`'s failures on, and `match` turns one into another (agent)
  rule: failure.invalid_outside_construction
  help: docs/errors/failure.invalid_outside_construction.md

check failed: 1 error, 1 file
```
