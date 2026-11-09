# `route.run_invalid`

A route's `run:` isn't a call of a named action or query.

## Why Jadpo refuses it

A route is a thin door: it runs one named action, or for a `GET` a named query
or an action that only reads. Keeping logic out of the route means it has a
name, can be tested and shows up in the graph. A function is called from an
action or a query, not run by a route, so the route doesn't take on a
transaction nobody declared.

## What it looks like

main.jadpo:

```jadpo check project=e-run-invalid expect=route.run_invalid
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
        run: true
    }
}
```

```text output
error: `run: true` isn't a call
 --> main.jadpo:32:14
   |
32 |         run: true
   |              ^^^^ not a call
  reason: `run:` calls one named action, or for a `GET` a named query or an action that only reads, passing the route's placeholders and `input`; there is no inline action in a route.
  next: Write `run: <name>(...)`, moving any logic into an action (agent)
  rule: route.run_invalid
  help: https://jadpo.dev/docs/errors/route.run_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A function is called from an action, so a route can't run one:

```jadpo check project=e-run-function expect=route.run_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

function shout(memo: Memo) -> MemoView {
    return MemoView {
        text: memo
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        output: MemoView
        run: shout("Hello")
    }
}
```

```text output
error: `shout` is a function
 --> main.jadpo:25:14
   |
25 |         run: shout("Hello")
   |              ^^^^^ a function here
  reason: A route runs a named action, or for a `GET` a named query or an action that only reads; a function is called from those, so the route stays a thin door.
  next: Call `shout` from an action, and run that (agent)
  rule: route.run_invalid
  help: https://jadpo.dev/docs/errors/route.run_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If there is an action to run**, call it:

```jadpo fragment
POST / {
    run: add_memo(input)
    ...
}
```

```jadpo check project=e-run-invalid-fixed hidden
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

The route now runs `add_memo` with what the client sent.

**If the logic is a function**, call it from an action and run that:

```jadpo fragment
action shout_memo(input: AddMemo) -> MemoView {
    return shout(input.text)
}
```

```jadpo fragment
POST / {
    run: shout_memo(input)
    ...
}
```

```jadpo check project=e-run-invalid-action hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

function shout(memo: Memo) -> MemoView {
    return MemoView {
        text: memo
    }
}

action shout_memo(input: AddMemo) -> MemoView {
    return shout(input.text)
}

policy {
    actions: {
        shout_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: shout_memo(input)
    }
}
```

The route now runs `shout_memo`, which owns the transaction and calls the
function.

## Related

- [What a route runs](../language/07-http.md#what-a-route-runs)
