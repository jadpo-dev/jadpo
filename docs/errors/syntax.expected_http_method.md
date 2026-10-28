# `syntax.expected_http_method`

A route is written with `GET`, `POST`, `PUT`, `PATCH` or `DELETE`; a word another framework uses is read as the method it means:

```jadpo check project=e-expected-http-method expect=syntax.expected_http_method
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

    CREATE / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `CREATE` isn't a method a route is written with
 --> main.jadpo:24:5
   |
24 |     CREATE / {
   |     ^^^^^^ not a route method
  reason: Routes are written with `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.
  next: Use `POST` instead of `CREATE` (agent)
  rule: syntax.expected_http_method
  help: docs/language/07-http.html#syntax-expected-http-method

check failed: 1 error, 1 file
```
