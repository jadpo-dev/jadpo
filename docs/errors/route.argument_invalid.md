# `route.argument_invalid`

A route passes what it runs something it didn't read from the request.

## Why Jadpo refuses it

A route only connects a request to what it runs, so everything a call
receives comes from the request by name. A value built or worked out in the
route is logic no test of the action covers and no reader looks for there.

## What it looks like

main.jadpo:

```jadpo check project=e-argument-invalid expect=route.argument_invalid
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
        output: MemoView
        run: add_memo(
            AddMemo {
                text: "Hello"
            }
        )
    }
}
```

```text output
error: A route passes its placeholders and input, not a value built in the route
 --> main.jadpo:32:13
   |
32 |             AddMemo {
   |             ^^^^^^^^^ a value built in the route
  reason: A route is a thin door: what it runs gets the route's placeholders, its `input` and their fields as written, and works out anything else itself.
  next: Pass a placeholder, `input` or a field of them, and move the rest into what the route runs (agent)
  rule: route.argument_invalid
  help: https://jadpo.dev/docs/errors/route.argument_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A route passes its arguments by position, so a named one is refused too:

```jadpo check project=e-argument-named expect=route.argument_invalid
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
        run: add_memo(input: input)
    }
}
```

```text output
error: A route passes its placeholders and input, not a named argument
 --> main.jadpo:32:30
   |
32 |         run: add_memo(input: input)
   |                              ^^^^^ a named argument
  reason: A route is a thin door: what it runs gets the route's placeholders, its `input` and their fields as written, and works out anything else itself.
  next: Pass a placeholder, `input` or a field of them, and move the rest into what the route runs (agent)
  rule: route.argument_invalid
  help: https://jadpo.dev/docs/errors/route.argument_invalid

check failed: 1 error, 1 file
```

## How to fix it

Pass what the request holds, and work out anything else in what the route
runs.

**If the value comes from the request**, take it as `input:` and pass
that:

```jadpo fragment
POST / {
    input: AddMemo
    run: add_memo(input)
    ...
}
```

```jadpo check project=e-argument-invalid-input hidden
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

`add_memo` now gets the `AddMemo` the client sent.

**If the route should always pass the same value**, build it in an action
and run that:

```jadpo fragment
action add_welcome_memo() -> MemoView {
    return MemoView {
        text: "Hello"
    }
}
```

```jadpo fragment
POST / {
    run: add_welcome_memo()
    ...
}
```

```jadpo check project=e-argument-invalid-moved hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

action add_welcome_memo() -> MemoView {
    return MemoView {
        text: "Hello"
    }
}

policy {
    actions: {
        add_welcome_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        output: MemoView
        run: add_welcome_memo()
    }
}
```

The fixed text now lives in `add_welcome_memo`, where a test can reach it.

## Related

- [What a route runs](../language/07-http.md#what-a-route-runs)
