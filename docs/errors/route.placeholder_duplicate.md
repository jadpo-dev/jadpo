# `route.placeholder_duplicate`

Each placeholder is named once in a path, its group's prefix included:

```jadpo check project=e-placeholder-duplicate expect=route.placeholder_duplicate
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

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos/{memo_id: MemoId}

    POST /{memo_id: MemoId} {
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: Placeholder `memo_id` appears twice
 --> main.jadpo:30:12
   |
28 |     prefix: /memos/{memo_id: MemoId}
   |                     ------- first named here
30 |     POST /{memo_id: MemoId} {
   |            ^^^^^^^ named again
  reason: Each placeholder reaches what the route runs by its name, so a path names each once, its group's prefix included.
  next: Rename one `memo_id` (agent)
  rule: route.placeholder_duplicate
  help: docs/errors/route.placeholder_duplicate.md

check failed: 1 error, 1 file
```
