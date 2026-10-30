# `type.batch_order_updatable`

A run walks live records a page at a time, each page after the record
the one before ended on, so a `from:` whose query orders by a field an
update can change can miss a record whose field moved behind that place
mid-run, or meet it twice. An order no update changes keeps every record
in exactly once: `id`, a field marked `immutable: true`, or a time
generated as the record is created, then `id` ("How a run is
kept"):

```jadpo check project=e-batch-order-updatable file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type StockCount = Int {
    min: 0
    max: 100000
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
    stock: StockCount
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
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

```jadpo check project=e-batch-order-updatable expect=type.batch_order_updatable
/// The items, a page at a time, in code order.
query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

/// Starts marking every item seen.
action sweep_shelves(page: ItemPage) -> BatchRun {
    return batches.each(from: items_by_code(page), each: item -> mark_seen(item))
}
```

```text output
error: `batches.each` walks `items_by_code` in an order an update can change
 --> main.jadpo:13:31
   |
13 |     return batches.each(from: items_by_code(page), each: item -> mark_seen(item))
   |                               ^^^^^^^^^^^^^^^^^^^ ordered by `Item.code`, which an update can change
  reason: A run walks live records a page at a time, each page after the record the one before ended on, so a record whose `Item.code` changes mid-run can move behind that place and be missed, or ahead of it and be met twice. An order no update changes meets every record once: `id`, or a time generated as the record is created, then `id`.
  next: Order `items_by_code` by `Item.id`, or by a field marked `immutable: true` or generated with `Generated.on_create` and then `Item.id`, or write `decline type.batch_order_updatable: "<why>"` above it if this is deliberate (agent)
  rule: type.batch_order_updatable
  help: https://jadpo.dev/docs/errors/type.batch_order_updatable

check failed: 1 error, 2 files
```

It is a judgement call, so it can be declined on the action, with the
reason the order is safe. Here the walk goes from the most stock to the
least while sales only ever lower an item's stock: a sale moves an item
further along the walk, so it can be met twice but never missed, and
marking an item seen twice does no harm:

```jadpo check project=batch-order-declined file=stock.jadpo hidden
type StockCount = Int {
    min: 0
    max: 100000
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
    stock: StockCount
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
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

```jadpo check project=batch-order-declined
/// The items, a page at a time, the most stock first.
query items_by_stock(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.stock desc]
    page: {
        size: page.page_size
        after: page.after
    }
}

/// Starts marking every item seen, the best stocked first.
decline type.batch_order_updatable: "Sales only lower stock, which moves an item further along the walk, never past it; marking one seen twice does no harm"
action sweep_by_stock(page: ItemPage) -> BatchRun {
    return batches.each(from: items_by_stock(page), each: item -> mark_seen(item))
}
```
