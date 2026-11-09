# `route.header_duplicate`

Two fields of an input read the same header.

## Why Jadpo refuses it

HTTP compares header names without case, so `X-Request-Tag` and
`x-request-tag` are one header. Two fields reading it would always hold the
same value, which is either a mistake or a second name for one thing, and
Jadpo can't tell which field you meant to keep.

## What it looks like

main.jadpo:

```jadpo check project=e-header-duplicate expect=route.header_duplicate
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    first: Memo {
        header: "X-Request-Tag"
        optional: true
    }
    second: Memo {
        header: "x-request-tag"
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
error: Header `x-request-tag` is read twice
 --> main.jadpo:16:17
   |
12 |         header: "X-Request-Tag"
   |                 --------------- first read here
16 |         header: "x-request-tag"
   |                 ^^^^^^^^^^^^^^^ read again
  reason: Header names are compared without case, so two fields reading one header would always hold the same value.
  next: Remove one of the fields (agent)
  rule: route.header_duplicate
  help: https://jadpo.dev/docs/errors/route.header_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the fields mean the same thing**, keep one:

```jadpo fragment
type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "X-Request-Tag"
        optional: true
    }
}
```

```jadpo check project=e-header-duplicate-one hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "X-Request-Tag"
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

The action reads the header once, through `input.tag`.

**If they are different headers**, name each one:

```jadpo fragment
type TaggedMemo = Input {
    text: Memo
    first: Memo {
        header: "X-Request-Tag"
        optional: true
    }
    second: Memo {
        header: "X-Trace-Tag"
        optional: true
    }
}
```

```jadpo check project=e-header-duplicate-two hidden
type Memo = Text {
    max_length: 200
}

type MemoView = Output {
    text: Memo
}

type TaggedMemo = Input {
    text: Memo
    first: Memo {
        header: "X-Request-Tag"
        optional: true
    }
    second: Memo {
        header: "X-Trace-Tag"
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

Each field now holds the header it names.

## Related

- [Headers](../language/07-http.md#headers)
- [One input](../language/07-http.md#one-input)
