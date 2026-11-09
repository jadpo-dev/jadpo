# `route.header_not_scalar`

A field read from a header isn't a single value.

## Why Jadpo refuses it

A header holds one value, so a field read from one is a scalar or an enum
without data. There is no agreed way to put a list or an object in a header,
and Jadpo won't invent one that clients would have to guess.

## What it looks like

main.jadpo:

```jadpo check project=e-header-not-scalar expect=route.header_not_scalar
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    tags: List<Memo> {
        header: "X-Tags"
        optional: true
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
error: A header can't hold `List<Memo>`
 --> main.jadpo:12:17
   |
12 |         header: "X-Tags"
   |                 ^^^^^^^^ one value per header
  reason: A header holds one value, so a field read from one is a scalar or an enum without data; lists and objects come in the body.
  next: Move the field to the body, or give it a scalar type (agent)
  rule: route.header_not_scalar
  help: https://jadpo.dev/docs/errors/route.header_not_scalar

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field holds several values**, take it in the body, without
`header:`:

```jadpo fragment
type TaggedMemo = Input {
    text: Memo
    tags: List<Memo> {
        optional: true
    }
}
```

```jadpo check project=e-header-not-scalar-body hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    tags: List<Memo> {
        optional: true
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

A client now sends `tags` as a list in the JSON body.

**If it holds one value**, give it a scalar type:

```jadpo fragment
type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "X-Tag"
        optional: true
    }
}
```

```jadpo check project=e-header-not-scalar-one hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "X-Tag"
        optional: true
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

The field now reads one value from the `X-Tag` header.

## Related

- [Headers](../language/07-http.md#headers)
- [One input](../language/07-http.md#one-input)
