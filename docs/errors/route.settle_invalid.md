# `route.settle_invalid`

A route's `settle:` isn't an answer query and a wait shorter than its deadline, or the route has nothing to wait for.

## Why Jadpo refuses it

`settle:` makes a route wait for the work its action set off, then answer with
a named query. It holds `answer:`, a named query given `result` or its fields,
and `within:`, shorter than the route's deadline, so the answer still has time
to run. It goes only on a route of a method other than `GET` that runs an
action and answers `.ok` or `.created`: anything else has no work to wait for,
or no answer to send.

## What it looks like

main.jadpo:

```jadpo check project=e-settle-invalid expect=route.settle_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
    text: Memo
}

action add_note(input: AddMemo) -> NoteView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

policy {
    actions: {
        add_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: NoteView
        status: SuccessStatus.created
        run: add_note(input)
        settle: {
            answer: add_note(input)
            within: 5s
        }
    }
}
```

```text output
error: `answer: add_note` is an action
 --> main.jadpo:63:21
   |
63 |             answer: add_note(input)
   |                     ^^^^^^^^ an action here
  reason: `settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.
  next: Answer with a named query, which reads how far the work has got (agent)
  rule: route.settle_invalid
  help: https://jadpo.dev/docs/errors/route.settle_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`within:` has to be shorter than the route's deadline, 10 seconds by
default:

```jadpo check project=e-settle-within expect=route.settle_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
    text: Memo
}

action add_note(input: AddMemo) -> NoteView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

policy {
    actions: {
        add_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: NoteView
        status: SuccessStatus.created
        run: add_note(input)
        settle: {
            answer: note_view(result.id)
            within: 15s
        }
    }
}
```

```text output
error: `within: 15s` doesn't fit inside the route's 10s deadline
 --> main.jadpo:64:21
   |
64 |             within: 15s
   |                     ^^^ not shorter than the deadline
  reason: `settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.
  next: Wait for less than the deadline, or give the route a longer `deadline:` (agent)
  rule: route.settle_invalid
  help: https://jadpo.dev/docs/errors/route.settle_invalid

check failed: 1 error, 1 file
```

`.accepted` would make both answers 202, and `.no_content` sends no answer:

```jadpo check project=e-settle-accepted expect=route.settle_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
    text: Memo
}

action add_note(input: AddMemo) -> NoteView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

policy {
    actions: {
        add_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: NoteView
        status: SuccessStatus.accepted
        run: add_note(input)
        settle: {
            answer: note_view(result.id)
            within: 5s
        }
    }
}
```

```text output
error: `POST /notes` answers `SuccessStatus.accepted` and waits with `settle:`
 --> main.jadpo:62:9
   |
62 |         settle: {
   |         ^^^^^^ waits for its work
  reason: `settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.
  next: Answer `SuccessStatus.ok` or `.created` once the work settles, or remove `settle:` and answer at once (agent)
  rule: route.settle_invalid
  help: https://jadpo.dev/docs/errors/route.settle_invalid

check failed: 1 error, 1 file
```

A query sets off no work to wait for:

```jadpo check project=e-settle-query expect=route.settle_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
    text: Memo
}

action add_note(input: AddMemo) -> NoteView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

policy {
    actions: {
        add_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    GET /{note_id: Note.id} {
        authentication: RouteAuthentication.none
        output: NoteView
        run: note_view(note_id)
        settle: {
            answer: note_view(result.id)
            within: 5s
        }
    }
}
```

```text output
error: `GET /notes/{note_id}` runs a query, which sets off no work to wait for
 --> main.jadpo:60:9
   |
60 |         settle: {
   |         ^^^^^^ nothing to wait for
  reason: `settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.
  next: Remove `settle:`; a query answers at once (agent)
  rule: route.settle_invalid
  help: https://jadpo.dev/docs/errors/route.settle_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the route should wait**, answer with a named query given `result` or
its fields, and wait for less than the deadline:

```jadpo fragment
POST / {
    settle: {
        answer: note_view(result.id)
        within: 5s
    }
    ...
}
```

```jadpo check project=e-settle-fixed hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
    text: Memo
}

action add_note(input: AddMemo) -> NoteView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

policy {
    actions: {
        add_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: NoteView
        status: SuccessStatus.created
        run: add_note(input)
        settle: {
            answer: note_view(result.id)
            within: 5s
        }
    }
}
```

The route now waits up to 5 seconds for the work it set off, then answers 201,
or 202 if the time ran out.

**If it shouldn't wait**, remove `settle:`:

```jadpo fragment
POST / {
    status: SuccessStatus.created
    ...
}
```

```jadpo check project=e-settle-removed hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
    text: Memo
}

action add_note(input: AddMemo) -> NoteView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

policy {
    actions: {
        add_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: NoteView
        status: SuccessStatus.created
        run: add_note(input)
    }
}
```

The route now answers once the action commits, and the work carries on
afterwards.

## Related

- [Waiting for follow-on work](../language/07-http.md#waiting-for-follow-on-work)
