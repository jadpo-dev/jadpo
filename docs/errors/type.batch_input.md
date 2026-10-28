# `type.batch_input`

Only the runtime builds a batch type, and `batches.status` and
`batches.cancel` find a run by its `id` alone, so no request carries one:
an `Input`, read from a body or a query string, and a route's placeholder
may not hold a `BatchRun`, a `BatchProgress`, a `RowProblem` or an
`ImportSummary`, however deep. A run comes from
`batches.each`, or from a field the caller may read where the
application kept it:

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
  help: docs/language/12-jobs-and-events.html#type-batch-input

check failed: 1 error, 2 files
```
