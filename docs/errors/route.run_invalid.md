# `route.run_invalid`

There is no inline action in a route, and a function is called from an action, not run by a route:

```jadpo check project=e-run-invalid expect=route.run_invalid
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
        run: true
    }
}
```

```text output
error: `run: true` isn't a call
 --> main.jadpo:27:14
   |
27 |         run: true
   |              ^^^^ not a call
  reason: `run:` calls one named action, or a named query for a `GET`, passing the route's placeholders and `input`; there is no inline action in a route.
  next: Write `run: <name>(...)`, moving any logic into an action (agent)
  rule: route.run_invalid
  help: https://jadpo.dev/docs/errors/route.run_invalid

check failed: 1 error, 1 file
```
