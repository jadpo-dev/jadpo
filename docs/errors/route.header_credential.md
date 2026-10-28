# `route.header_credential`

Credentials and Jadpo's own headers belong to authentication (area 8),
and `Idempotency-Key` to the runtime, which reads it before anything runs:

```jadpo check project=e-header-credential expect=route.header_credential
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
    token: Memo {
        header: "Authorization"
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
error: `Authorization` belongs to authentication
 --> main.jadpo:24:17
   |
24 |         header: "Authorization"
   |                 ^^^^^^^^^^^^^^^ read by authentication
  reason: Credentials (`Authorization`, cookies) and Jadpo's own `X-Jadpo-` headers are read by authentication (area 8), so a route never sees them as input.
  next: Remove the field, and take the caller as the route's `current_principal` (agent)
  rule: route.header_credential
  help: docs/language/07-http.html#route-header-credential

check failed: 1 error, 1 file
```
