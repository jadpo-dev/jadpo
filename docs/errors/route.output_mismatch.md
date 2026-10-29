# `route.output_mismatch`

`output:` is exactly what the route runs returns:

```jadpo check project=e-output-mismatch expect=route.output_mismatch
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

type MemoSummary = Output {
    text: Memo
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoSummary
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` sends `MemoSummary`, but `add_memo` returns `MemoView`
 --> main.jadpo:30:17
   |
30 |         output: MemoSummary
   |                 ^^^^^^^^^^^ not `MemoView`
  reason: A route's `output:` is exactly what it runs returns, so the route shows its contract and never sends a field its type doesn't declare.
  next: Write `output: MemoView` (agent)
  rule: route.output_mismatch
  help: docs/errors/route.output_mismatch.md

check failed: 1 error, 1 file
```
