# `route.status_invalid`

A success status is a `SuccessStatus`; failure statuses come from failure kinds:

```jadpo check project=e-status-invalid expect=route.status_invalid
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
        status: 201
        run: add_memo(input)
    }
}
```

```text output
error: `201` isn't a success status
 --> main.jadpo:27:17
   |
27 |         status: 201
   |                 ^^^ not a `SuccessStatus`
  reason: `status:` takes `SuccessStatus.ok`, `.created`, `.accepted` or `.no_content`; failure statuses come from the kinds of the failures the route can produce, never from the route.
  next: Write `SuccessStatus.created` (agent)
  rule: route.status_invalid
  help: docs/language/07-http.html#route-status-invalid

check failed: 1 error, 1 file
```
