# `route.header_not_scalar`

A header holds one value:

```jadpo check project=e-header-not-scalar expect=route.header_not_scalar
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
    tags: List<Memo> {
        header: "X-Tags"
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
error: A header can't hold `List<Memo>`
 --> main.jadpo:24:17
   |
24 |         header: "X-Tags"
   |                 ^^^^^^^^ one value per header
  reason: A header holds one value, so a field read from one is a scalar or an enum without data; lists and objects come in the body.
  next: Move the field to the body, or give it a scalar type (agent)
  rule: route.header_not_scalar
  help: docs/language/07-http.html#route-header-not-scalar

check failed: 1 error, 1 file
```
