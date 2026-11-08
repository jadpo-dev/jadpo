# `semantic.request_invalid`

A route test's `request { }` doesn't fit its route, or something only a route test uses appears elsewhere.

## Why Jadpo refuses it

A route test sends its request through the real HTTP boundary, so it has
to be one the route could get: its method, its path, each placeholder in
`params:` and, when the route takes one, its `input:`. A request that
didn't fit would never reach the code it was meant to test.

## What it looks like

The route takes an `AddMemo`, and the request gives none:

main.jadpo:

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
error: `POST /memos` takes an `input:` of type `AddMemo`
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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An item a request can't hold is refused:

```jadpo check project=e-request-unknown-item expect=semantic.request_invalid
test "a memo comes back" {
    var response: Response = request {
        method: HttpMethod.get
        path: "/memos"
        headers: {}
    }
    assert response.status == 200
}
```

```text output
error: `headers` isn't something a request holds
 --> main.jadpo:5:9
  |
5 |         headers: {}
  |         ^^^^^^^ unknown item
  reason: A request holds `method:`, `path:`, `params:`, `input:`, `idempotency_key:`, `as:`, and with `as:` `sign_in:` and `strength:`.
  next: Remove `headers` (agent)
  rule: semantic.request_invalid
  help: https://jadpo.dev/docs/errors/semantic.request_invalid

check failed: 1 error, 1 file
```

A `request { }` only exists in a route test. Application code calls the
action or query itself:

```jadpo check project=e-request-outside-test expect=semantic.request_invalid
action fetch_memos() -> Response {
    return request {
        method: HttpMethod.get
        path: "/memos"
    }
}
```

```text output
error: `request` is only for route tests
 --> main.jadpo:2:12
  |
2 |     return request {
  |            ^^^^^^^ outside a test
  reason: A route test sends a request through the real HTTP boundary; application code calls the action or query a route runs instead.
  next: Call the action or query itself (agent)
  rule: semantic.request_invalid
  help: https://jadpo.dev/docs/errors/semantic.request_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the route takes an input**, give it one:

```jadpo fragment
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos"
        input: AddMemo {
            text: "Buy flour"
        }
    }
```

```jadpo check project=e-request-invalid-fix hidden
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
        input: AddMemo {
            text: "Buy flour"
        }
    }
    assert response.status == 201
}
```

The request now reaches `add_memo` the way a real caller would.

## Related

- [Route tests](../language/13-testing.md#route-tests)
- [Signing in for a route test](../language/13-testing.md#signing-in-for-a-route-test)
