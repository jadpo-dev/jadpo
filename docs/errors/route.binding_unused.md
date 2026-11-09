# `route.binding_unused`

A route reads a placeholder or an `input:` it never passes on.

## Why Jadpo refuses it

Everything a route reads from a request reaches what it runs by name. A
placeholder or an input the route ignores is part of the URL or the request
that does nothing, and a client would be told it matters. Jadpo can't tell
whether the value should be used or the route shouldn't ask for it.

## What it looks like

main.jadpo:

```jadpo check project=e-binding-unused expect=route.binding_unused
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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos/{memo_id}` reads placeholder `memo_id` and never passes it on
 --> main.jadpo:34:14
   |
34 |         run: add_memo(input)
   |              ^^^^^^^^ `memo_id` not passed
  reason: Everything a route reads from a request reaches what it runs by name; a value read and ignored is a URL or a field that does nothing.
  next: Pass `memo_id` to `add_memo`, or remove it from the route (agent)
  rule: route.binding_unused
  help: https://jadpo.dev/docs/errors/route.binding_unused

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `input:` nothing receives is refused the same way:

```jadpo check project=e-binding-unused-input expect=route.binding_unused
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action welcome() -> MemoView {
    return MemoView {
        text: "Hello"
    }
}

policy {
    actions: {
        welcome: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: welcome()
    }
}
```

```text output
error: `POST /memos` reads `input` and never passes it on
 --> main.jadpo:32:14
   |
32 |         run: welcome()
   |              ^^^^^^^ `input` not passed
  reason: Everything a route reads from a request reaches what it runs by name; a value read and ignored is a URL or a field that does nothing.
  next: Pass `input` to `welcome`, or remove it from the route (agent)
  rule: route.binding_unused
  help: https://jadpo.dev/docs/errors/route.binding_unused

check failed: 1 error, 1 file
```

## How to fix it

**If what it runs needs the value**, pass it on. In the action, take it as
a parameter:

```jadpo fragment
action add_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}
```

```jadpo fragment
POST /{memo_id: MemoId} {
    run: add_memo(memo_id, input)
    ...
}
```

```jadpo check project=e-binding-unused-passed hidden
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action add_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(memo_id, input)
    }
}
```

`add_memo` now receives the `memo_id` from the URL.

**If nothing needs it**, take it out of the route:

```jadpo fragment
POST / {
    run: add_memo(input)
    ...
}
```

```jadpo check project=e-binding-unused-removed hidden
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
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

The route is now `POST /memos`, with nothing in the URL that does nothing.

## Related

- [What a route runs](../language/07-http.md#what-a-route-runs)
- [Paths and placeholders](../language/07-http.md#paths-and-placeholders)
