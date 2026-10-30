# `route.path_overlap`

Two routes that could answer one request are an error; a fixed segment beats a placeholder, so `/memos/archived` could sit beside `/memos/{memo_id}`:

```jadpo check project=e-path-overlap expect=route.path_overlap
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
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }

    POST /{shelf_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(shelf_id, input)
    }
}
```

```text output
error: `POST /memos/{shelf_id}` could answer the same requests as `POST /memos/{memo_id}`
 --> main.jadpo:37:5
   |
30 |     POST /{memo_id: MemoId} {
   |     ----------------------- already answered here
37 |     POST /{shelf_id: MemoId} {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^ answers the same requests
  reason: A request such as `/memos/…` matches both `POST /memos/{memo_id}` and `POST /memos/{shelf_id}`: where two paths differ, one must have fixed text where the other has a placeholder, since a fixed segment beats a placeholder.
  next: Give one of the paths fixed text where the other has a placeholder (agent)
  rule: route.path_overlap
  help: https://jadpo.dev/docs/errors/route.path_overlap

check failed: 1 error, 1 file
```
