# `type.batch_order_updatable`

A batch's `from:` is a query ordered by a field an update can change.

## Why Jadpo refuses it

A run walks live records a page at a time, each page starting after the
record the one before ended on. If an update changes the field the query
orders by while the run is going, a record can move behind that place
and be missed, or ahead of it and be met twice. An order no update
changes meets every record exactly once: `id`, a field marked
`immutable: true`, or a time generated as the record is created, then
`id`.

Jadpo doesn't look at which updates the program makes, so adding one
later never makes a run's order wrong after the fact. A run can still be
safe in an order an update changes, when every change moves a record
the same way along the walk and meeting a record twice does no harm.
That is a judgement call, so you can decline it with a reason, which
review reads.

## What it looks like

main.jadpo, beside a `stock.jadpo` that declares `Item`, its page
settings `ItemPage` and the action `mark_seen`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the run needs that order.

**If any order will do**, order the query by `id`. In `main.jadpo`:

```jadpo fragment
query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
    ...
}
```

```jadpo check project=e-batch-order-updatable-fix-1 file=main.jadpo hidden
/// The items, a page at a time, in an order no change to an item moves.
query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
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

```jadpo check project=e-batch-order-updatable-fix-1 file=stock.jadpo hidden
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

The run now meets every item exactly once, whatever changes while it
goes.

**If an item's code never changes once it is made**, mark the field
`immutable: true` in `stock.jadpo`, and order by it, then `id`:

```jadpo fragment
entity Item {
    id: Uuid
    code: ShelfCode {
        immutable: true
    }
    ...
}
```

```jadpo check project=e-batch-order-updatable-fix-2 file=stock.jadpo hidden
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
    code: ShelfCode {
        immutable: true
    }
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

In `main.jadpo`:

```jadpo fragment
query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc, Item.id asc]
    ...
}
```

```jadpo check project=e-batch-order-updatable-fix-2 file=main.jadpo hidden
/// The items, a page at a time, in code order.
query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc, Item.id asc]
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

The run keeps code order, and an update that sets `code` is now
`type.field_update_forbidden`, so no item can move along the walk. A
time generated with `Generated.on_create`, then `id`, keeps the order
the items were made in the same way.

**If the order is safe for what the run does**, keep it, and write a
decline line with the reason directly above the action that starts the
run. Here the walk goes from the most stock to the least, while sales
only ever lower an item's stock: a sale moves an item further along the
walk, so it can be met twice but never missed, and marking an item seen
twice does no harm. main.jadpo, beside a `stock.jadpo` whose `Item` has
a `stock` field:

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

`jadpo approval` lists every decline with its reason, so a reviewer sees
why the order is safe.

## Related

- [An order an update can change](../language/12-jobs-and-events.md#an-order-an-update-can-change)
- [Walking records or a provider's list: `batches.each`](../language/12-jobs-and-events.md#walking-records-or-a-providers-list-batcheseach)
- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
