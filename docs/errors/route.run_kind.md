# `route.run_kind`

A `GET` route runs an action that isn't read-only, or a route of another
method runs a query.

## Why Jadpo refuses it

HTTP promises that a `GET` only reads, so a browser, a cache or a client
library may send one again without asking. Jadpo keeps that promise by what
a route runs. A query can only read, so a `GET` can run one. An action can
write, so a `GET` runs one only when the compiler can prove it writes
nothing, emits nothing and makes no changing call, itself and through every
action it calls. `POST`, `PUT`, `PATCH` and `DELETE` run an action, which owns
the writes and the transaction, and never a query.

A `GET` running an action that changes something could change it again each
time it is repeated, and nothing can show whether you meant the route to read
or to write. The message names what the action does and where, following the
calls, so you can see which it is.

## What it looks like

Here the action adds a note, so it can't answer a `GET`:

main.jadpo:

```jadpo check project=e-run-kind expect=route.run_kind
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

action add_memo(input: AddMemo) -> MemoView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return MemoView {
        text: note.text
    }
}

routes Memos {
    prefix: /memos

    GET / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `GET /memos` runs action `add_memo`, which writes with `Note.create`
 --> main.jadpo:41:14
   |
24 |     var note: Note = Note.create {
   |                      ----------- here
41 |         run: add_memo(input)
   |              ^^^^^^^^ not read-only
  reason: A `GET` is safe to repeat, so it runs a named query, or an action the graph proves writes nothing, emits nothing and makes no changing call; `add_memo` writes with `Note.create`.
  next: Answer `/memos` with another method, or leave `Note.create` out of what `add_memo` does (agent)
  rule: route.run_kind
  help: https://jadpo.dev/docs/errors/route.run_kind

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the route changes anything.

**If the route adds or changes something**, answer it with another
method, here `POST`, which reads its input from the JSON body:

```jadpo fragment
    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
```

```jadpo check project=e-run-kind-post hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}

action add_memo(input: AddMemo) -> MemoView {
    var note: Note = Note.create {
        set: {
            text: input.text
        }
    }
    return MemoView {
        text: note.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

`POST /memos` runs `add_memo`, and a client sends the memo as JSON.

**If the route only reads**, run a query that reads what it shows, or
leave what changes something out of the action. Here the memos are stored as
`Note` records, and the route lists them:

```jadpo fragment
query recent_notes() -> List<MemoView> {
    from: Note
    order: [Note.text asc]
    limit: 50
}
```

```jadpo fragment
    GET / {
        output: List<MemoView>
        run: recent_notes()
    }
```

```jadpo check project=e-run-kind-query hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

entity Note {
    id: Uuid
    text: Memo

    policy: {
        read: [Access.authenticated]
    }
}

query recent_notes() -> List<MemoView> {
    from: Note
    order: [Note.text asc]
    limit: 50
}

routes Memos {
    prefix: /memos

    GET / {
        output: List<MemoView>
        run: recent_notes()
    }
}
```

`GET /memos` answers with up to 50 notes and never writes, so repeating
it is safe. A signed-in caller sees the notes `Note`'s `read:` lets them
read.

**An action that works something out** can stay on the `GET`, once it stops
doing what changes something. An action like this one, which reads and
calculates, has nothing to refuse:

```jadpo check project=e-run-kind-action hidden
type Memo = Text {
    max_length: 200
}

type MemoInput = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

type TooShort = Failure {
    kind: FailureKind.rejected
    message: "That memo is too short."
}

action check_memo(input: MemoInput) fails TooShort -> MemoView {
    if (text.length(input.text) < 3) {
        reject TooShort {}
    }
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    GET / {
        authentication: RouteAuthentication.none
        input: MemoInput
        output: MemoView
        run: check_memo(input)
    }
}

policy {
    actions: {
        check_memo: [Access.public]
    }
}
```

```jadpo fragment
action check_memo(input: MemoInput) fails TooShort -> MemoView {
    if (text.length(input.text) < 3) {
        reject TooShort {}
    }
    return MemoView {
        text: input.text
    }
}
```

## Related

- [What a route runs](../language/07-http.md#what-a-route-runs)
- [A `GET` that runs an action](../language/07-http.md#a-get-that-runs-an-action)
- [One input](../language/07-http.md#one-input)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
