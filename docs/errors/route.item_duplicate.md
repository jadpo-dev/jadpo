# `route.item_duplicate`

Each item is given once:

```jadpo check project=e-item-duplicate expect=route.item_duplicate
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
        run: add_memo(input)
    }
}
```

```text output
error: `run:` is given twice
 --> main.jadpo:28:9
   |
27 |         run: add_memo(input)
   |         --- first given here
28 |         run: add_memo(input)
   |         ^^^ given again
  reason: A route has one `run:`, so there is never a question of which counts.
  next: Remove one `run:` (agent)
  rule: route.item_duplicate
  help: https://jadpo.dev/docs/errors/route.item_duplicate

check failed: 1 error, 1 file
```
