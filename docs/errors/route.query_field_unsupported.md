# `route.query_field_unsupported`

A field read from the query string isn't a scalar, an enum or a list of them.

## Why Jadpo refuses it

A `GET` or a `DELETE` reads its input from the query string, which holds
scalars, enums and lists of them, written `?status=open&status=done`. An object
or a map has no agreed way of being written there. An amount of money, a
quantity and a `ZonedTime` are refused too, since JSON sends them as objects.

## What it looks like

main.jadpo:

```jadpo check project=e-query-field-unsupported expect=route.query_field_unsupported
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type ForgetMemos = Input {
    tags: Map<Memo, Memo>
}

action forget_memos(input: ForgetMemos) -> MemoView {
    return MemoView {
        text: "forgotten"
    }
}

policy {
    actions: {
        forget_memos: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    DELETE / {
        authentication: RouteAuthentication.none
        input: ForgetMemos
        output: MemoView
        run: forget_memos(input)
    }
}
```

```text output
error: `ForgetMemos.tags` can't come in a query string
 --> main.jadpo:30:16
   |
30 |         input: ForgetMemos
   |                ^^^^^^^^^^^ `tags` is `Map<Memo, Memo>`
  reason: A `DELETE` route reads its input from the query string, which holds scalars, enums and lists of them (`?status=open&status=done`); nested objects and maps need a body.
  next: Give `tags` a scalar type, or take this input on a `POST` (agent)
  rule: route.query_field_unsupported
  help: https://jadpo.dev/docs/errors/route.query_field_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field is really a list of values**, give it a type the query string
holds:

```jadpo fragment
type ForgetMemos = Input {
    tags: List<Memo>
}
```

```jadpo check project=e-query-field-list hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type ForgetMemos = Input {
    tags: List<Memo>
}

action forget_memos(input: ForgetMemos) -> MemoView {
    return MemoView {
        text: "forgotten"
    }
}

policy {
    actions: {
        forget_memos: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    DELETE / {
        authentication: RouteAuthentication.none
        input: ForgetMemos
        output: MemoView
        run: forget_memos(input)
    }
}
```

A client now sends `?tags=one&tags=two`.

**If the field has to stay an object or a map**, take the input in a body,
with a `POST`:

```jadpo fragment
POST /forgotten {
    input: ForgetMemos
    ...
}
```

```jadpo check project=e-query-field-post hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type ForgetMemos = Input {
    tags: Map<Memo, Memo>
}

action forget_memos(input: ForgetMemos) -> MemoView {
    return MemoView {
        text: "forgotten"
    }
}

policy {
    actions: {
        forget_memos: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /forgotten {
        authentication: RouteAuthentication.none
        input: ForgetMemos
        output: MemoView
        run: forget_memos(input)
    }
}
```

A client now sends `tags` in the JSON body.

## Related

- [The query string](../language/07-http.md#the-query-string)
- [One input](../language/07-http.md#one-input)
