# `semantic.request_invalid`

A request gives what its route takes: each placeholder in `params:` and
the route's `input:`:

```jadpo check project=e-request-invalid expect=semantic.request_invalid
type Memo = Text {
    max_length: 200
}

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
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
    }
}

policy {
    actions: {
        add_memo: [Access.public]
    }
}

test "a memo comes back" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos"
    }
    assert response.status == 201
}
```

```text output
error: `POST /memos` takes `input:`, a `AddMemo`
 --> main.jadpo:38:30
   |
38 |     var response: Response = request {
   |                              ^^^^^^^ doesn't fit the route
  reason: A request gives the route's method, its path as `jadpo routes` prints it, each placeholder in `params:` and, when the route takes one, its `input:`; a request but a `GET` may give the `Idempotency-Key` it sends in `idempotency_key:`.
  next: Give the request what its route takes (agent)
  rule: semantic.request_invalid
  help: https://jadpo.dev/docs/errors/semantic.request_invalid

check failed: 1 error, 1 file
```
