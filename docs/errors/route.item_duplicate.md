# `route.item_duplicate`

A route or a route group gives an item twice.

## Why Jadpo refuses it

A route or a route group gives each item once. If an item appeared twice,
Jadpo would have to pick which one counts, and a reader couldn't tell from
the route what it does.

## What it looks like

main.jadpo:

```jadpo check project=e-item-duplicate expect=route.item_duplicate
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
        run: add_memo(input)
    }
}
```

```text output
error: `run:` is given twice
 --> main.jadpo:33:9
   |
32 |         run: add_memo(input)
   |         --- first given here
33 |         run: add_memo(input)
   |         ^^^ given again
  reason: A route has one `run:`, so there is never a question of which counts.
  next: Remove one `run:` (agent)
  rule: route.item_duplicate
  help: https://jadpo.dev/docs/errors/route.item_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A route group is held to the same rule:

```jadpo check project=e-item-duplicate-group expect=route.item_duplicate
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
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `prefix:` is given twice
 --> main.jadpo:27:5
   |
26 |     prefix: /memos
   |     ------ first given here
27 |     prefix: /notes
   |     ^^^^^^ given again
  reason: A route group has one `prefix:`, so there is never a question of which counts.
  next: Remove one `prefix:` (agent)
  rule: route.item_duplicate
  help: https://jadpo.dev/docs/errors/route.item_duplicate

check failed: 1 error, 1 file
```

## How to fix it

**If the two are the same**, or one is left over, remove one:

```jadpo fragment
POST / {
    run: add_memo(input)
    ...
}
```

```jadpo check project=e-item-duplicate-fixed hidden
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

The route now has one `run:`.

## Related

- [Route groups](../language/07-http.md#route-groups)
- [What a route runs](../language/07-http.md#what-a-route-runs)
