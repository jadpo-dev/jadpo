# `route.prefix_required`

A route group has no `prefix:`, or its prefix isn't a path.

## Why Jadpo refuses it

Every route in a group follows the group's prefix, and the prefix's fixed
segments give the file its name. That is how a URL from a log leads to one
file, so a group with no prefix, or one that isn't a path, has nowhere to go.

## What it looks like

main.jadpo:

```jadpo check project=e-prefix-required expect=route.prefix_required
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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos` has no prefix
 --> main.jadpo:25:8
   |
25 | routes Memos {
   |        ^^^^^ no `prefix:`
  reason: Every route in a group follows the group's prefix, and the prefix's fixed segments give the file its name, so a URL from a log leads to one file.
  next: Add `prefix: /<path>` as the group's first line (agent)
  rule: route.prefix_required
  help: https://jadpo.dev/docs/errors/route.prefix_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A prefix is written as a path, not as text:

```jadpo check project=e-prefix-not-path expect=route.prefix_required
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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: "memos"

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `prefix:` takes a path
 --> main.jadpo:26:13
   |
26 |     prefix: "memos"
   |             ^^^^^^^ not a path
  reason: A group's prefix is the path its routes follow, written as a path starting with `/`.
  next: Write the prefix as a path starting with `/` (agent)
  rule: route.prefix_required
  help: https://jadpo.dev/docs/errors/route.prefix_required

check failed: 1 error, 1 file
```

## How to fix it

**If the prefix is missing or written as text**, write it as a path:

```jadpo fragment
routes Memos {
    prefix: /memos
    ...
}
```

```jadpo check project=e-prefix-required-fixed hidden
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

The route is now `POST /memos`.

## Related

- [Route groups](../language/07-http.md#route-groups)
