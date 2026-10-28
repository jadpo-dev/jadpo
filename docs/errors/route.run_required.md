# `route.run_required`

A route runs a named action, or a named query for a `GET`:

```jadpo check project=e-run-required expect=route.run_required
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
    }
}
```

```text output
error: `POST /memos` doesn't say what it runs
 --> main.jadpo:24:5
   |
24 |     POST / {
   |     ^^^^^^ no `run:`
  reason: A route runs one named action, or a named query for a `GET`, with `run:`; the logic lives there, not in the route.
  next: Add `run: <action>(...)` (agent)
  rule: route.run_required
  help: docs/language/07-http.html#route-run-required

check failed: 1 error, 1 file
```
