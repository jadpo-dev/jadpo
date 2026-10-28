# `route.settle_invalid`

`settle:` holds `answer:`, a named query given `result` or its fields,
and `within:`, shorter than the route's deadline, on a route that runs
an action and answers `.ok` or `.created`:

```jadpo check project=e-settle-invalid expect=route.settle_invalid
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

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
        settle: {
            answer: add_memo(input)
            within: 5s
        }
    }
}
```

```text output
error: `answer: add_memo` is an action
 --> main.jadpo:28:21
   |
28 |             answer: add_memo(input)
   |                     ^^^^^^^^ an action here
  reason: `settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.
  next: Answer with a named query, which reads how far the work has got (agent)
  rule: route.settle_invalid
  help: docs/language/07-http.html#route-settle-invalid

check failed: 1 error, 1 file
```
