# `route.header_name_invalid`

A field read from a header names it as HTTP writes it:

```jadpo check project=e-header-name-invalid expect=route.header_name_invalid
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
    tag: Memo {
        header: "Request Tag"
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
error: `Request Tag` isn't a header name
 --> main.jadpo:24:17
   |
24 |         header: "Request Tag"
   |                 ^^^^^^^^^^^^^ not a header name
  reason: A header name is letters, digits and `-` (HTTP allows a few more symbols), with no spaces: `X-Request-Tag`.
  next: Write the header's name without spaces (agent)
  rule: route.header_name_invalid
  help: docs/language/07-http.html#route-header-name-invalid

check failed: 1 error, 1 file
```
