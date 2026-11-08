# `route.header_name_invalid`

A field's `header:` isn't a valid HTTP header name.

## Why Jadpo refuses it

A field read from a header has to name one HTTP can carry. A name with a
space or another character HTTP doesn't allow could never arrive, so the
field would be empty or missing on every request.

## What it looks like

main.jadpo:

```jadpo check project=e-header-name-invalid expect=route.header_name_invalid
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "Request Tag"
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
error: `Request Tag` isn't a header name
 --> main.jadpo:12:17
   |
12 |         header: "Request Tag"
   |                 ^^^^^^^^^^^^^ not a header name
  reason: A header name is letters, digits and `-` (HTTP allows a few more symbols), with no spaces: `X-Request-Tag`.
  next: Write the header's name without spaces (agent)
  rule: route.header_name_invalid
  help: https://jadpo.dev/docs/errors/route.header_name_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name has a space**, write it as HTTP does, usually with `-`:

```jadpo fragment
type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "Request-Tag"
        optional: true
    }
}
```

```jadpo check project=e-header-name-fixed hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "Request-Tag"
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

The field now reads the `Request-Tag` header.

## Related

- [Headers](../language/07-http.md#headers)
- [One input](../language/07-http.md#one-input)
