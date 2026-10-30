# `type.batch_capture_not_storable`

What a run saves (`from:`'s query's arguments, what `each:` and
`finished:` capture, and `key:`) is kept as its type's JSON, so the run can
resume after a crash or a deploy; a value an entity field couldn't hold,
such as a request's `Input`, is refused, naming it:

```jadpo check project=e-batch-capture-not-storable file=stock.jadpo hidden
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

```jadpo check project=e-batch-capture-not-storable expect=type.batch_capture_not_storable
action mark_on(item: Item, page: ItemPage) -> Unit {
    assert item.seen == item.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_on(item, page))
}
```

```text output
error: `batches.each` can't save `page`
 --> main.jadpo:6:72
  |
6 |     return batches.each(from: items(page), each: item -> mark_on(item, page))
  |                                                                        ^^^^ `ItemPage` has no stored form
  reason: What `each:` captures is worked out when the run starts and saved with it, as its type's JSON, so the run can resume after a crash or a deploy; `page` is `ItemPage`, an `Input`, the shape a request arrives in, where a record keeps an `Object`.
  next: Pass something an entity field could hold instead, such as the `id` that finds `page` (agent)
  rule: type.batch_capture_not_storable
  help: https://jadpo.dev/docs/errors/type.batch_capture_not_storable

check failed: 1 error, 2 files
```
