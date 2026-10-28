# `route.input_not_input`

A route accepts only an `Input`:

```jadpo check project=e-input-not-input expect=route.input_not_input
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
        input: MemoView
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `MemoView` isn't an `Input`
 --> main.jadpo:25:16
   |
25 |         input: MemoView
   |                ^^^^^^^^ not an `Input`
  reason: A route accepts only an `Input` type (area 2): the only objects that may leave fields out or fill them by default, and the ones a request's fields are checked against.
  next: Declare `type <Name> = Input { ... }` and take it here (agent)
  rule: route.input_not_input
  help: docs/errors/route.input_not_input.md

check failed: 1 error, 1 file
```
