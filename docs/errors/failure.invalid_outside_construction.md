# `failure.invalid_outside_construction`

An `attempt` block has `invalid:` on something other than building a value.

## Why Jadpo refuses it

`invalid:` catches the rules a value can break when it is built. A call's
failures, a read's and a write's conditions are named and listed, and each
needs saying what becomes of it, so a catch-all would hide a condition added
later.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.invalid_outside_construction

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An entity's operation maps its own conditions, such as `missing:`, each with
an arm:

main.jadpo:

```jadpo check project=e-invalid-outside-entity expect=failure.invalid_outside_construction
entity Note {
    id: Uuid

    policy: {
        read: [Access.public]
    }
}

action find_note(note_id: Note.id) fails NoteNotFound -> Note {
    return attempt Note.get(note_id) {
        invalid: NoteNotFound
    }
}
```

```text output
error: `Note.get(...)` isn't a construction, so `invalid:` covers nothing
 --> main.jadpo:11:9
   |
11 |         invalid: NoteNotFound
   |         ^^^^^^^ not a construction
  reason: `Note.get(...)` is an entity's operation, which maps each of its own conditions, such as `missing:` or `conflict:`, with an arm; `invalid:` is the catch-all only for the rules a construction's value can break.
  next: Remove `invalid: ...`, and map each condition the operation has with its own arm (agent)
  rule: failure.invalid_outside_construction
  help: https://jadpo.dev/docs/errors/failure.invalid_outside_construction

check failed: 1 error, 1 file
```

A write maps each rule it can break in `conflict:`:

main.jadpo:

```jadpo check project=e-invalid-outside-write expect=failure.invalid_outside_construction
type Handle = Text {
    max_length: 40
}

type HandleBad = Failure {
    kind: FailureKind.conflict
}

entity Account {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
    }
}

action claim(handle: Handle) fails HandleBad -> Account {
    var account: Account = attempt Account.create {
        set: {
            handle: handle
        }
        conflict: {
            Account.handle: HandleBad
        }
        invalid: HandleBad
    }
    return account
}
```

```text output
error: `Account.create` isn't a construction, so `invalid:` covers nothing
 --> main.jadpo:28:9
   |
28 |         invalid: HandleBad
   |         ^^^^^^^ not a construction
  reason: `Account.create` is an entity's write: each rule it can break is mapped in `conflict:`, keyed by the rule; `invalid:` is the catch-all only for the rules a construction's value can break.
  next: Remove `invalid: ...`, and map each rule in `conflict:` (agent)
  rule: failure.invalid_outside_construction
  help: https://jadpo.dev/docs/errors/failure.invalid_outside_construction

check failed: 1 error, 1 file
```

## How to fix it

**If the call's failures should pass on as they are**, remove the block:

```jadpo fragment
action save_note(note: Note) fails NoteRefused -> Note {
    return attempt check_note(note)
}
```

```jadpo check project=e-invalid-outside-fix-1 hidden
type Note = Text {}

type NoteRefused = Failure {
    kind: FailureKind.rejected
}

action check_note(note: Note) fails NoteRefused -> Note {
    if (note == "") {
        reject NoteRefused
    }
    return note
}

action save_note(note: Note) fails NoteRefused -> Note {
    return attempt check_note(note)
}
```

`save_note` now ends with whatever failure `check_note` gives.

**If one should become another**, `match` on the call:

```jadpo fragment
action save_note(note: Note) fails NoteInvalid -> Note {
    return match check_note(note) {
        success(kept) -> kept
        NoteRefused -> reject NoteInvalid
    }
}
```

```jadpo check project=e-invalid-outside-fix-2 hidden
type Note = Text {}

type NoteRefused = Failure {
    kind: FailureKind.rejected
}

action check_note(note: Note) fails NoteRefused -> Note {
    if (note == "") {
        reject NoteRefused
    }
    return note
}

type NoteInvalid = Failure {
    kind: FailureKind.invalid_value
}

action save_note(note: Note) fails NoteInvalid -> Note {
    return match check_note(note) {
        success(kept) -> kept
        NoteRefused -> reject NoteInvalid
    }
}
```

A note `check_note` refuses now ends `save_note` with `NoteInvalid`.

## Related

- [A value built at run time: rule keys and `invalid:`](../language/04-failures.md#a-value-built-at-run-time-rule-keys-and-invalid)
- [A failure from a call: `match`](../language/04-failures.md#a-failure-from-a-call-match)
