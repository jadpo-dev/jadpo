# `type.batch_from_invalid`

A batch's `from:` is something other than a call of a named query that gives a `Page`, such as a query that gives a `List`.

## Why Jadpo refuses it

A batch, such as `batches.each` or a file's write helper, walks `from:`
a page at a time in the background, after the action that starts it
commits. The run keeps the cursor of the next page, so a crash or a
deploy carries on where it stopped. A query that gives a `List`, or a page already read, has no
cursor to keep, so the run couldn't keep its place.

## What it looks like

main.jadpo, beside a `stock.jadpo` that declares `Item`, a paged query
`items` and the action `mark_seen`:

```jadpo check project=e-batch-from-invalid file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

```jadpo check project=e-batch-from-invalid expect=type.batch_from_invalid
query first_items() -> List<Item> {
    from: Item
    order: [Item.code asc]
    limit: 10
}

action sweep_shelf() -> BatchRun {
    return batches.each(from: first_items(), each: item -> mark_seen(item))
}
```

```text output
error: `batches.each`'s `from:` isn't a call of a named query giving a `Page`
 --> main.jadpo:8:31
  |
8 |     return batches.each(from: first_items(), each: item -> mark_seen(item))
  |                               ^^^^^^^^^^^^^ gives `List<Item>`
  reason: A run walks `from:` a page at a time, after the action that starts it commits: it calls a named query that gives a `Page<T>`, and each item is a `T`; the query's arguments are worked out now and saved with the run.
  next: Write `batches.each(from: <query>(...), ...)`, with a query whose result is `Page<T>` (agent)
  rule: type.batch_from_invalid
  help: https://jadpo.dev/docs/errors/type.batch_from_invalid

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a query giving a `Page` of the items already exists**, give the
action that query's page settings and call it in `from:`. In
`main.jadpo`, which no longer needs `first_items`:

```jadpo fragment
action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

```jadpo check project=e-batch-from-invalid-fix-1 file=main.jadpo hidden
action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

```jadpo check project=e-batch-from-invalid-fix-1 file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

The run now reads `Item`s in pages of the size `page.page_size` gives, 25
by default, and calls `mark_seen` for each. It orders them by `id`, which
no update changes, so each item is seen exactly once.

When there is only a list's query, make it give a `Page` instead: an
`order:` it can page through and a `page:` with its size and cursor, in
place of `limit:`. That is the shape of `items` in `stock.jadpo`.

## Related

- [Walking records or a provider's list: `batches.each`](../language/12-jobs-and-events.md#walking-records-or-a-providers-list-batcheseach)
- [A batch is a page](../language/12-jobs-and-events.md#a-batch-is-a-page)
- [Named queries](../language/06-reads-and-writes.md#named-queries)
- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
