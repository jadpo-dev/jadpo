# `route.query_field_unsupported`

A `GET` or `DELETE` reads its input from the query string, which holds scalars, enums and lists of them (an amount of money, a quantity and a `ZonedTime`, which JSON sends as objects, are objects here too):

```jadpo check project=e-query-field-unsupported expect=route.query_field_unsupported
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

type ForgetMemos = Input {
    tags: Map<Memo, Memo>
}

action forget_memos(input: ForgetMemos) -> MemoView {
    return MemoView {
        text: "forgotten"
    }
}

routes Memos {
    prefix: /memos

    DELETE / {
        input: ForgetMemos
        output: MemoView
        run: forget_memos(input)
    }
}
```

```text output
error: `ForgetMemos.tags` can't come in a query string
 --> main.jadpo:35:16
   |
35 |         input: ForgetMemos
   |                ^^^^^^^^^^^ `tags` is `Map<Memo, Memo>`
  reason: A `DELETE` route reads its input from the query string, which holds scalars, enums and lists of them (`?status=open&status=done`); nested objects and maps need a body.
  next: Give `tags` a scalar type, or take this input on a `POST` (agent)
  rule: route.query_field_unsupported
  help: docs/language/07-http.html#route-query-field-unsupported

check failed: 1 error, 1 file
```
