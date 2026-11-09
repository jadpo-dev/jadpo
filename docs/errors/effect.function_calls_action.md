# `effect.function_calls_action`

A function, or other code that only works out a value, calls an action or a query.

## Why Jadpo refuses it

A function only works out a value from its arguments, so the same arguments
always give the same result and a test needs no database. An action can read
and change stored data and a query reads it, so a call to either would hide
that inside something that promises not to. The same goes for an inline
function, a constant, a type's rule, a backfill and a template.

## What it looks like

main.jadpo:

```jadpo check project=e-fn-action expect=effect.function_calls_action
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

function pretend_pure(note: Note) -> Note {
    return save(note)
}
```

```text output
error: A function can't call action `save`
 --> main.jadpo:8:12
  |
8 |     return save(note)
  |            ^^^^ an action
  reason: A function is pure: it computes its result from its arguments alone, so it calls only functions, library functions and the standard library; an action may read and write data.
  next: Make the caller an action, or call `save` from the action that uses this function (agent)
  rule: effect.function_calls_action
  help: https://jadpo.dev/docs/errors/effect.function_calls_action

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A query reads stored data, so it is refused the same way:

main.jadpo:

```jadpo check project=e-fn-query expect=effect.function_calls_action
type NoteText = Text {}

entity Note {
    id: Uuid
    text: NoteText

    policy: {
        read: [Access.public]
    }
}

type NoteView = Output {
    text: NoteText
}

type NoteMissing = Failure {
    kind: FailureKind.not_found
}

query note_view(note_id: Note.id) fails NoteMissing -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteMissing
}

function read_note(note_id: Note.id) fails NoteMissing -> NoteView {
    return attempt note_view(note_id)
}
```

```text output
error: A function can't call query `note_view`
 --> main.jadpo:27:20
   |
27 |     return attempt note_view(note_id)
   |                    ^^^^^^^^^ a query
  reason: A function is pure: it computes its result from its arguments alone, so it calls only functions, library functions and the standard library; a query may read data.
  next: Make the caller an action, or call `note_view` from the action that uses this function (agent)
  rule: effect.function_calls_action
  help: https://jadpo.dev/docs/errors/effect.function_calls_action

check failed: 1 error, 1 file
```

An inline function is held to the same rule, even inside an action:

main.jadpo:

```jadpo check project=e-fn-inline expect=effect.function_calls_action
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

action save_all(notes: List<Note>) -> List<Note> {
    return list.map(notes, note -> save(note))
}
```

```text output
error: An inline function can't call action `save`
 --> main.jadpo:8:36
  |
8 |     return list.map(notes, note -> save(note))
  |                                    ^^^^ an action
  reason: An inline function is pure: it computes its result from its arguments alone, so it calls only functions, library functions and the standard library; an action may read and write data.
  next: Call `save` outside the inline function (agent)
  rule: effect.function_calls_action
  help: https://jadpo.dev/docs/errors/effect.function_calls_action

check failed: 1 error, 1 file
```

## How to fix it

**If the caller has to save the note**, make it an action:

```jadpo fragment
action pretend_pure(note: Note) -> Note {
    return save(note)
}
```

```jadpo check project=e-fn-action-fix-1 hidden
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

action pretend_pure(note: Note) -> Note {
    return save(note)
}
```

It can now call `save`, and a function can no longer call it.

**If the function only works something out**, have the action make the call
and pass the function what it needs:

```jadpo fragment
function tidied(note: Note) -> Note {
    return note
}

action save_tidied(note: Note) -> Note {
    return tidied(save(note))
}
```

```jadpo check project=e-fn-action-fix-2 hidden
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

function tidied(note: Note) -> Note {
    return note
}

action save_tidied(note: Note) -> Note {
    return tidied(save(note))
}
```

The function is pure again, and the saving is where a reader expects it.

## Related

- [Functions and actions](../language/03-logic.md#functions-and-actions)
- [`effect.function_persistence`](effect.function_persistence.md)
