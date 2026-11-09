# `route.placeholder_duplicate`

A route's path names a placeholder twice, or names one `input`.

## Why Jadpo refuses it

Each placeholder reaches what the route runs by its name, so one name can
only be one value. A path names each placeholder once, its group's prefix
included. `input` is taken too, since it names the route's `input:`.

## What it looks like

main.jadpo:

```jadpo check project=e-placeholder-duplicate expect=route.placeholder_duplicate
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
    prefix: /memos/{memo_id: MemoId}

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: Placeholder `memo_id` appears twice
 --> main.jadpo:32:12
   |
30 |     prefix: /memos/{memo_id: MemoId}
   |                     ------- first named here
32 |     POST /{memo_id: MemoId} {
   |            ^^^^^^^ named again
  reason: Each placeholder reaches what the route runs by its name, so a path names each once, its group's prefix included.
  next: Rename one `memo_id` (agent)
  rule: route.placeholder_duplicate
  help: https://jadpo.dev/docs/errors/route.placeholder_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A placeholder can't be called `input`:

```jadpo check project=e-placeholder-input expect=route.placeholder_duplicate
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

    POST /{input: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(input, input)
    }
}
```

```text output
error: A placeholder can't be called `input`
 --> main.jadpo:32:12
   |
32 |     POST /{input: MemoId} {
   |            ^^^^^ the input's name
  reason: `input` names the route's `input:`, so a placeholder of that name would be two values with one name.
  next: Rename the placeholder (agent)
  rule: route.placeholder_duplicate
  help: https://jadpo.dev/docs/errors/route.placeholder_duplicate

check failed: 1 error, 1 file
```

## How to fix it

**If the two are different values**, rename one. If they are the same value,
remove one:

```jadpo fragment
routes Memos {
    prefix: /shelves/{shelf_id: MemoId}/memos

    POST /{memo_id: MemoId} {
        ...
    }
}
```

```jadpo check project=e-placeholder-duplicate-fixed hidden
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    shelf: MemoId
    text: Memo
}

action tag_memo(shelf_id: MemoId, memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        shelf: shelf_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /shelves/{shelf_id: MemoId}/memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(shelf_id, memo_id, input)
    }
}
```

Each placeholder now reaches what the route runs under its own name.

## Related

- [Paths and placeholders](../language/07-http.md#paths-and-placeholders)
