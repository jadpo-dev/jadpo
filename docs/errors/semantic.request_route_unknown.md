# `semantic.request_route_unknown`

A route test's `request { }` names a method and path that no route in the app answers.

## Why Jadpo refuses it

A route test sends its request through the HTTP layer Jadpo generates
for your routes, so the request has to reach one of them. It names the
route by `method:` and by its path as `jadpo routes` prints it, with each
placeholder by name, such as `/todos/{todo_id}`. A request no route
answers would only ever test the 404, so Jadpo reports it when you
check, before the test runs.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.request_route_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the test meant an existing route**, write its path as `jadpo routes`
lists it. In `main.jadpo`:

```jadpo fragment
        path: "/memos"
```

```jadpo check project=e-request-route-fix-1 hidden
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
            text: "Call the plumber"
        }
    }
    assert response.status == 201
}
```

The request now reaches `POST /memos`, which runs `add_memo` and answers
201, so the test passes.

When the route the test means doesn't exist yet, add the route first.
A placeholder's value goes in `params:`, as in `path: "/todos/{todo_id}"`
with `params: { todo_id: todo.id }`, never in the path itself.

## Related

- [Route tests](../language/13-testing.md#route-tests)
- [Kinds of test](../language/13-testing.md#kinds-of-test)
