# `route.run_kind`

A `GET` never writes, so it runs a query; every other method runs an action:

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
