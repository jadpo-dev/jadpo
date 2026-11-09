# `route.input_not_input`

A route's `input:` isn't an `Input` type.

## Why Jadpo refuses it

A request's fields are checked against an `Input` type. Only an `Input` may
leave fields out or fill them by default, and it says which fields a client
may send. Any other type wasn't written for that, so taking one would let a
client set fields nobody meant it to.

## What it looks like

main.jadpo:

```jadpo check project=e-input-not-input expect=route.input_not_input
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
        input: MemoView
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `MemoView` isn't an `Input`
 --> main.jadpo:30:16
   |
30 |         input: MemoView
   |                ^^^^^^^^ not an `Input`
  reason: A route accepts only an `Input` type (area 2): the only objects that may leave fields out or fill them by default, and the ones a request's fields are checked against.
  next: Declare `type <Name> = Input { ... }` and take it here (agent)
  rule: route.input_not_input
  help: https://jadpo.dev/docs/errors/route.input_not_input

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If an `Input` exists for what the client sends**, take that one:

```jadpo fragment
POST / {
    input: AddMemo
    ...
}
```

```jadpo check project=e-input-not-input-fixed hidden
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

The route now checks what the client sends against `AddMemo`.

## Related

- [One input](../language/07-http.md#one-input)
- [Named types](../language/02-values-and-types.md#named-types)
