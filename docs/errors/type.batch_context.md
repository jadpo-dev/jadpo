# `type.batch_context`

Only an action starts a run, reads its progress or cancels it, keeping
what it does in the action's transaction:

```jadpo check project=e-batch-context file=stock.jadpo hidden
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

```jadpo check project=e-batch-context expect=type.batch_context
action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}

/// How far a sweep has got, for a report.
function shelf_report(shelf_run: BatchRun) -> BatchProgress {
    return batches.status(shelf_run)
}
```

```text output
error: A function can't call `batches.status`
 --> main.jadpo:7:12
  |
7 |     return batches.status(shelf_run)
  |            ^^^^^^^^^^^^^^ outside an action
  reason: `batches.status` reads a run the runtime keeps in the store an action writes, so only an action calls it.
  next: Call `batches.status` in an action (agent)
  rule: type.batch_context
  help: docs/errors/type.batch_context.md

check failed: 1 error, 2 files
```
