# `route.api_unknown`

A group names an API `app.jadpo` declares; with none declared there is one, `main`:

```jadpo check project=e-api-unknown expect=route.api_unknown
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
    api: admin

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos` belongs to API `admin`, which isn't declared
 --> main.jadpo:23:10
   |
23 |     api: admin
   |          ^^^^^ not declared
  reason: With no `apis:` in `app.jadpo` there is one API, `main`, at `/`.
  next: Declare `admin` under `apis:` in `app.jadpo` (agent)
  rule: route.api_unknown
  help: docs/language/07-http.html#route-api-unknown

check failed: 1 error, 1 file
```
