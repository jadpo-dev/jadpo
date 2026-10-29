# `semantic.request_route_unknown`

A request names a route the project has, by its method and its path as
`jadpo routes` prints it:

```jadpo check project=e-request-route expect=semantic.request_route_unknown
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
        path: "/memo"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.status == 201
}
```

```text output
error: No route answers `POST /memo`
 --> main.jadpo:40:15
   |
40 |         path: "/memo"
   |               ^^^^^^^ no such route
  reason: A request names a route by its method and its path as `jadpo routes` prints it, placeholders by name; the `POST` routes are `/memos`.
  next: Write the path of the route this test exercises, as `jadpo routes` lists it (agent)
  rule: semantic.request_route_unknown
  help: docs/errors/semantic.request_route_unknown.md

check failed: 1 error, 1 file
```
