# `route.path_overlap`

Two routes could answer the same request, or a route takes a path the runtime answers itself.

## Why Jadpo refuses it

Each request is answered by exactly one route, so a URL from a log leads to
one place. Where two paths differ, one has to have fixed text where the other
has a placeholder, since a fixed segment beats a placeholder. `/memos/archived`
can sit beside `/memos/{memo_id}`, but two placeholders at one position can't
be told apart. The runtime also answers some paths itself, such as each API's
`/openapi.json` and `/flags`, and a route there would never run.

## What it looks like

main.jadpo:

```jadpo check project=e-path-overlap expect=route.path_overlap
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }

    POST /{shelf_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(shelf_id, input)
    }
}
```

```text output
error: `POST /memos/{shelf_id}` could answer the same requests as `POST /memos/{memo_id}`
 --> main.jadpo:39:5
   |
32 |     POST /{memo_id: MemoId} {
   |     ----------------------- already answered here
39 |     POST /{shelf_id: MemoId} {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^ answers the same requests
  reason: A request such as `/memos/…` matches both `POST /memos/{memo_id}` and `POST /memos/{shelf_id}`: where two paths differ, one must have fixed text where the other has a placeholder, since a fixed segment beats a placeholder.
  next: Give one of the paths fixed text where the other has a placeholder (agent)
  rule: route.path_overlap
  help: https://jadpo.dev/docs/errors/route.path_overlap

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A path the runtime answers itself is refused the same way:

```jadpo check project=e-path-overlap-runtime expect=route.path_overlap
type Hello = Output {
    greeting: Text
}

action hello() -> Hello {
    return Hello {
        greeting: "Hello"
    }
}

policy {
    actions: {
        hello: [Access.public]
    }
}

routes Root {
    prefix: /

    GET /openapi.json {
        authentication: RouteAuthentication.none
        output: Hello
        run: hello()
    }
}
```

```text output
error: `GET /openapi.json` is where API `main` serves its OpenAPI document
 --> main.jadpo:20:5
   |
20 |     GET /openapi.json {
   |     ^^^^^^^^^^^^^^^^^ answered by the runtime
  reason: Each API serves its OpenAPI document at `/openapi.json` unless its entry in `apis:` says `serve_openapi: false`, so this route would never run.
  next: Give `GET /openapi.json` in `Root` another path (agent)
  rule: route.path_overlap
  help: https://jadpo.dev/docs/errors/route.path_overlap

check failed: 1 error, 1 file
```

## How to fix it

**If both routes are needed**, give one a fixed segment where the other has
a placeholder:

```jadpo fragment
POST /shelves/{shelf_id: MemoId} {
    ...
}
```

```jadpo check project=e-path-overlap-fixed hidden
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }

    POST /shelves/{shelf_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(shelf_id, input)
    }
}
```

A request to `/memos/shelves/…` now reaches the second route, and anything
else under `/memos/` the first.

**If one is left over**, remove it:

```jadpo fragment
POST /{memo_id: MemoId} {
    ...
}
```

```jadpo check project=e-path-overlap-one hidden
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

Every request to `/memos/{memo_id}` now reaches the one route.

**If the runtime answers that path**, give the route another path:

```jadpo fragment
GET /hello.json {
    ...
}
```

```jadpo check project=e-path-overlap-runtime-fixed hidden
type Hello = Output {
    greeting: Text
}

action hello() -> Hello {
    return Hello {
        greeting: "Hello"
    }
}

policy {
    actions: {
        hello: [Access.public]
    }
}

routes Root {
    prefix: /

    GET /hello.json {
        authentication: RouteAuthentication.none
        output: Hello
        run: hello()
    }
}
```

The runtime keeps its own path, and the route now answers beside it.

## Related

- [Paths and placeholders](../language/07-http.md#paths-and-placeholders)
- [Serving the document](../language/07-http.md#serving-the-document)
