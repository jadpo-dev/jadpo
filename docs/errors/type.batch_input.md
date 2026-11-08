# `type.batch_input`

An `Input` or a route's placeholder holds a batch type such as `BatchRun`, a `File` or a `FileContents`, or what a provider or a queue sends holds a `File` or a `FileContents`, each of which only the runtime builds.

## Why Jadpo refuses it

`batches.status` and `batches.cancel` find a run by its `id` alone, so a
request that could name a `BatchRun` could read or stop anyone's run. A
`File` names a stored file's store and key, so a request that carried
one could reach any stored file, and a `FileContents` is only ever what
`store.read` gives. So only the runtime builds them. A request reaches a
run or a file through the record that keeps it, and bytes from outside
arrive as `Bytes`.

That holds however deep the type is: in a list, a map or an option, in
an object, or through an entity a field holds. The same goes for what
arrives from outside in other ways: an operation's `output:` or
`wire_output:`, a webhook's event and an event a queue lists.

## What it looks like

main.jadpo, beside a `stock.jadpo` that declares `Item` and the action
`sweep_shelf` that starts a run over them:

```jadpo check project=e-batch-input file=stock.jadpo hidden
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

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

```jadpo check project=e-batch-input expect=type.batch_input
type StopSweep = Input {
    shelf_run: BatchRun
}

action stop_sweep(request: StopSweep) -> Unit {
    batches.cancel(request.shelf_run)
}
```

```text output
error: `StopSweep.shelf_run` holds a `BatchRun`, which no request carries
 --> main.jadpo:2:16
  |
2 |     shelf_run: BatchRun
  |                ^^^^^^^^ a `BatchRun`
  reason: Only the runtime builds a `BatchRun`: a run comes from `batches.each`, or from a field the caller may read where the application kept it, and `batches.status` and `batches.cancel` find it by its `id` alone. A request that named one could read or stop any run.
  next: Take the `id` of the record that keeps the run, and read the `BatchRun` from its field (agent)
  rule: type.batch_input
  help: https://jadpo.dev/docs/errors/type.batch_input

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the request is about a run the application started**, keep the run
on a record when it starts, and have the request name that record by its
`id`. The action reads the `BatchRun` from the record's field:

```jadpo fragment
entity Stocktake {
    id: Uuid
    shelf_run: BatchRun
    ...
}

type StopSweep = Input {
    stocktake_id: Stocktake.id
}

action stop_sweep(request: StopSweep) fails StocktakeNotFound -> Unit {
    var stocktake: Stocktake = attempt Stocktake.get(request.stocktake_id)
    batches.cancel(stocktake.shelf_run)
}
```

```jadpo check project=e-batch-input-fix file=main.jadpo hidden
entity Stocktake {
    id: Uuid
    shelf_run: BatchRun

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}

type StopSweep = Input {
    stocktake_id: Stocktake.id
}

action stop_sweep(request: StopSweep) fails StocktakeNotFound -> Unit {
    var stocktake: Stocktake = attempt Stocktake.get(request.stocktake_id)
    batches.cancel(stocktake.shelf_run)
}
```

```jadpo check project=e-batch-input-fix file=stock.jadpo hidden
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

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

A request can now stop only a run the application kept on a stocktake,
which it names by the stocktake's `id`; an `id` with no stocktake fails
with `StocktakeNotFound`.

## Related

- [No request carries a batch type](../language/12-jobs-and-events.md#no-request-carries-a-batch-type)
- [Progress, cancelling and the summary](../language/12-jobs-and-events.md#progress-cancelling-and-the-summary)
- [Other built-in types](../language/02-values-and-types.md#other-built-in-types)
