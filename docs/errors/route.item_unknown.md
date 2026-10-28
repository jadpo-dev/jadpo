# `route.item_unknown`

A route holds `authentication:` (area 8), `input:`, `output:`, `status:`, `deadline:`, `run:`, `rate_limit:` and `settle:`; `success:`, `query:`, `path:` and `headers:` are not route items:

```jadpo check project=e-item-unknown expect=route.item_unknown
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
        run: add_memo(input)
        success: SuccessStatus.created
    }
}
```

```text output
error: `success` isn't something a route holds
 --> main.jadpo:28:9
   |
28 |         success: SuccessStatus.created
   |         ^^^^^^^ unknown item
  reason: `success:` is now `status:`, which takes a `SuccessStatus`. A route holds `authentication`, `rate_limit`, `input`, `output`, `status`, `deadline`, `run` and `settle`.
  next: Use `status` instead of `success` (agent)
  rule: route.item_unknown
  help: docs/language/07-http.html#route-item-unknown

check failed: 1 error, 1 file
```
