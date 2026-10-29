# `route.binding_unused`

Everything a route reads from a request reaches what it runs:

```jadpo check project=e-binding-unused expect=route.binding_unused
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

    POST /{memo_id: MemoId} {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos/{memo_id}` reads placeholder `memo_id` and never passes it on
 --> main.jadpo:27:14
   |
27 |         run: add_memo(input)
   |              ^^^^^^^^ `memo_id` not passed
  reason: Everything a route reads from a request reaches what it runs by name; a value read and ignored is a URL or a field that does nothing.
  next: Pass `memo_id` to `add_memo`, or remove it from the route (agent)
  rule: route.binding_unused
  help: docs/errors/route.binding_unused.md

check failed: 1 error, 1 file
```
