# `syntax.expected_http_method`

A route is written with a word other than `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.

## Why Jadpo refuses it

A route is written with the HTTP method it answers, one of five, so a
route's method means the same on every route. Another framework's word, such
as `CREATE`, `FETCH` or `REMOVE`, gets a `next:` naming the method it
usually means. `HEAD` and `OPTIONS` are answered for every route by the
runtime, so they are never written.

## What it looks like

main.jadpo:

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    CREATE / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `CREATE` isn't a method a route is written with
 --> main.jadpo:30:5
   |
30 |     CREATE / {
   |     ^^^^^^ not a route method
  reason: Routes are written with `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.
  next: Use `POST` instead of `CREATE` (agent)
  rule: syntax.expected_http_method
  help: https://jadpo.dev/docs/errors/syntax.expected_http_method

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`HEAD` and `OPTIONS` are this error too, and its `next:` tells you to remove
the route:

main.jadpo:

```jadpo check project=e-expected-http-method-2 expect=syntax.expected_http_method
routes Pings {
    prefix: /pings

    HEAD / {
    }
}
```

```text output
error: `HEAD` isn't a method a route is written with
 --> main.jadpo:4:5
  |
4 |     HEAD / {
  |     ^^^^ answered by the runtime
  reason: Routes are written with `GET`, `POST`, `PUT`, `PATCH` or `DELETE`; the runtime answers `HEAD` for every route itself.
  next: Remove the `HEAD` route (agent)
  rule: syntax.expected_http_method
  help: https://jadpo.dev/docs/errors/syntax.expected_http_method

check failed: 1 error, 1 file
```

## How to fix it

**If the route creates something**, write `POST`:

```jadpo fragment
routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```jadpo check project=e-expected-http-method-fix-1 hidden
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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

The route now answers `POST /memos`.

**If the route answers `HEAD` or `OPTIONS`**, remove it:

```jadpo check project=e-expected-http-method-fix-2
routes Pings {
    prefix: /pings
}
```

The runtime still answers `HEAD` and `OPTIONS` for every route, so nothing
is lost.

## Related

- [Route groups](../language/07-http.md#route-groups)
- [What a route runs](../language/07-http.md#what-a-route-runs)
