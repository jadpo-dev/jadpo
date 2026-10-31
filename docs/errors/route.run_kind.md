# `route.run_kind`

A `GET` route runs an action, or a route of another method runs a query.

## Why Jadpo refuses it

HTTP promises that a `GET` only reads, so a browser, a cache or a client
library may send one again without asking. Jadpo keeps that promise by
what a route runs. A query can only read, so a `GET` runs a named query.
An action can write, so `POST`, `PUT`, `PATCH` and `DELETE` run one.

A `GET` that runs an action could change something each time it is
repeated, and Jadpo can't know whether you meant the route to read or to
write.

## What it looks like

main.jadpo:

```jadpo check project=e-run-kind expect=route.run_kind
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    GET / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `GET /memos` runs action `add_memo`
 --> main.jadpo:27:14
   |
27 |         run: add_memo(input)
   |              ^^^^^^^^ an action
  reason: A `GET` is safe to repeat and never writes, so it runs a named query; every other method runs an action.
  next: Run a query here, or answer `/memos` with another method (agent)
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
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
```

```jadpo check project=e-run-kind-post hidden
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

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}

policy {
    actions: {
        add_memo: [Access.authenticated]
    }
}
```

`POST /memos` runs `add_memo`, and a client sends the memo as JSON. The
route needs a signed-in caller, so the checked project also signs people
in, and lists `add_memo`, which reaches no stored record, for anyone
signed in under `actions:`.

**If the route only reads**, run a query that reads what it shows. Here
the memos are stored as `Note` records, and the route lists them:

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

## Related

- [What a route runs](../language/07-http.md#what-a-route-runs)
- [One input](../language/07-http.md#one-input)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
