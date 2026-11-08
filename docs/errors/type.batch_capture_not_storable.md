# `type.batch_capture_not_storable`

What a batch's `each:` or `finished:` captures, or its `key:`, is a value an entity field couldn't hold, such as a request's `Input`.

## Why Jadpo refuses it

A run carries on after a crash or a deploy, long after the action that
started it has finished, so what `each:` and `finished:` capture from
that action, and `key:`, are saved with the run as their types' JSON.
They are saved the way an entity field holds a value, so a value no
entity field could hold has no saved form for the run to read back. An
`Input` is the shape a request arrives in, which a record keeps as an
`Object`, so a run can't save one.

## What it looks like

main.jadpo, beside a `stock.jadpo` that declares `Item`, its page
settings `ItemPage`, a paged query `items` and the action `mark_seen`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`from:`'s query arguments are saved as a request carries them, so
`items(page)` may take the `Input`; only what `each:` and `finished:`
capture, and `key:`, are held to what an entity field could hold.

## How to fix it

The question is what the action that runs for each item needs.

**If it needs part of the value**, pass that part, whose type an entity
field could hold, and have the action take it. In `main.jadpo`:

```jadpo fragment
action mark_on(item: Item, page_size: PageSize) -> Unit {
    assert item.seen == item.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_on(item, page.page_size))
}
```

```jadpo check project=e-batch-capture-fix-1 file=main.jadpo hidden
action mark_on(item: Item, page_size: PageSize) -> Unit {
    assert item.seen == item.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_on(item, page.page_size))
}
```

```jadpo check project=e-batch-capture-fix-1 file=stock.jadpo hidden
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

The page size is saved with the run as it starts, and every item's call
gets the same one, however long the run goes on.

**If it doesn't need the value**, stop passing it:

```jadpo fragment
action mark_on(item: Item) -> Unit {
    assert item.seen == item.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_on(item))
}
```

```jadpo check project=e-batch-capture-fix-2 file=main.jadpo hidden
action mark_on(item: Item) -> Unit {
    assert item.seen == item.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_on(item))
}
```

```jadpo check project=e-batch-capture-fix-2 file=stock.jadpo hidden
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

The run now saves nothing from `sweep_shelf` but the query's arguments,
and calls `mark_on` with each item alone.

## Related

- [Saved values](../language/12-jobs-and-events.md#saved-values)
- [`each:` and `finished:`](../language/12-jobs-and-events.md#each-and-finished)
- [How a run is kept](../language/12-jobs-and-events.md#how-a-run-is-kept)
- [Objects](../language/02-values-and-types.md#objects)
