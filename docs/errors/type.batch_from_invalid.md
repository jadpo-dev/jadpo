# `type.batch_from_invalid`

A run walks `from:` a page at a time, so it is a call of a named query
giving a `Page`; a list's query, or a page already read, is refused:

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
  help: docs/errors/type.batch_from_invalid.md

check failed: 1 error, 2 files
```
