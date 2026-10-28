# `route.header_duplicate`

Header names are compared without case:

```jadpo check project=e-header-duplicate expect=route.header_duplicate
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

type TaggedMemo = Input {
    text: Memo
    first: Memo {
        header: "X-Request-Tag"
        optional: true
    }
    second: Memo {
        header: "x-request-tag"
        optional: true
    }
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

```text output
error: Header `x-request-tag` is read twice
 --> main.jadpo:28:17
   |
24 |         header: "X-Request-Tag"
   |                 --------------- first read here
28 |         header: "x-request-tag"
   |                 ^^^^^^^^^^^^^^^ read again
  reason: Header names are compared without case, so two fields reading one header would always hold the same value.
  next: Remove one of the fields (agent)
  rule: route.header_duplicate
  help: docs/errors/route.header_duplicate.md

check failed: 1 error, 1 file
```
