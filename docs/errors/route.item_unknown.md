# `route.item_unknown`

A route or a route group has an item it doesn't take.

## Why Jadpo refuses it

A route holds `authentication:`, `confirm:`, `rate_limit:`, `input:`,
`output:`, `sends:`, `status:`, `cache:`, `deadline:`, `run:` and `settle:`.
Anything else would do nothing, so a mistyped word would look like a setting
while changing nothing. `success:`, `query:`, `path:` and `headers:` aren't
route items.

## What it looks like

main.jadpo:

```jadpo check project=e-item-unknown expect=route.item_unknown
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
        success: SuccessStatus.created
    }
}
```

```text output
error: `success` isn't something a route holds
 --> main.jadpo:33:9
   |
33 |         success: SuccessStatus.created
   |         ^^^^^^^ unknown item
  reason: `success:` is now `status:`, which takes a `SuccessStatus`. A route holds `authentication`, `confirm`, `rate_limit`, `input`, `output`, `sends`, `status`, `cache`, `deadline`, `run` and `settle`.
  next: Use `status` instead of `success` (agent)
  rule: route.item_unknown
  help: https://jadpo.dev/docs/errors/route.item_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A route group holds only its `prefix:`, `api:` and its routes, so it is
refused there too:

```jadpo check project=e-item-unknown-group expect=route.item_unknown
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
    version: 2

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `version` isn't something a route group holds
 --> main.jadpo:27:5
   |
27 |     version: 2
   |     ^^^^^^^ unknown item
  reason: A route group holds `prefix` and `api` and its routes.
  next: Remove `version` (agent)
  rule: route.item_unknown
  help: https://jadpo.dev/docs/errors/route.item_unknown

check failed: 1 error, 1 file
```

And routes sit side by side, so a route inside a route is refused:

```jadpo check project=e-item-unknown-nested expect=route.item_unknown
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

        POST /archived {
            authentication: RouteAuthentication.none
            input: AddMemo
            output: MemoView
            run: add_memo(input)
        }
    }
}
```

```text output
error: A route can't hold another route
 --> main.jadpo:34:9
   |
34 |         POST /archived {
   |         ^^^^^^^^^^^^^^^^ a route inside a route
  reason: Routes sit side by side in their group; a path that shares a start is written in full.
  next: Move the inner route out to the group, with its full path after the prefix (agent)
  rule: route.item_unknown
  help: https://jadpo.dev/docs/errors/route.item_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the word is an old or mistyped name**, write the one the route takes:

```jadpo fragment
POST / {
    status: SuccessStatus.created
    ...
}
```

```jadpo check project=e-item-unknown-fixed hidden
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
        status: SuccessStatus.created
        run: add_memo(input)
    }
}
```

The route now answers 201.

**If a route is inside another**, move it out, with its full path after the
prefix:

```jadpo fragment
POST / {
    ...
}

POST /archived {
    ...
}
```

```jadpo check project=e-item-unknown-side hidden
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

    POST /archived {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

Both routes now belong to the group, one beside the other.

## Related

- [Route groups](../language/07-http.md#route-groups)
- [What a route runs](../language/07-http.md#what-a-route-runs)
