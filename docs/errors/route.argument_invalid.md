# `route.argument_invalid`

A route passes its placeholders, `input` and their fields; anything worked out goes in what it runs:

```jadpo check project=e-argument-invalid expect=route.argument_invalid
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
 --> main.jadpo:27:13
   |
27 |             AddMemo {
   |             ^^^^^^^^^ a value built in the route
  reason: A route is a thin door: what it runs gets the route's placeholders, its `input` and their fields as written, and works out anything else itself.
  next: Pass a placeholder, `input` or a field of them, and move the rest into what the route runs (agent)
  rule: route.argument_invalid
  help: docs/language/07-http.html#route-argument-invalid

check failed: 1 error, 1 file
```
