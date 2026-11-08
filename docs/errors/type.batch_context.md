# `type.batch_context`

Code outside an action starts a batch, reads how far a run has got or cancels one.

## Why Jadpo refuses it

Starting a run, reading how far it has got and cancelling it each read
or write the row the runtime keeps for the run, in the store the
action's own writes go to. An action does that inside its transaction:
a run it starts begins only once the action commits, and never if the
action rolls back. Code outside an action, such as a function, has no
transaction to hold that row, so it may not touch a run.

## What it looks like

main.jadpo, beside a `stock.jadpo` that declares `Item`, a paged query
`items` and the action `mark_seen`:

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
  help: https://jadpo.dev/docs/errors/type.batch_context

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the code reads or changes a run**, make it an action. In
`main.jadpo`:

```jadpo fragment
/// How far a sweep has got, for a report.
action shelf_report(shelf_run: BatchRun) -> BatchProgress {
    return batches.status(shelf_run)
}
```

```jadpo check project=e-batch-context-fix file=main.jadpo hidden
action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}

/// How far a sweep has got, for a report.
action shelf_report(shelf_run: BatchRun) -> BatchProgress {
    return batches.status(shelf_run)
}
```

```jadpo check project=e-batch-context-fix file=stock.jadpo hidden
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

`shelf_report` now gives the run's `BatchProgress`: the items done, the
estimated total, the problems so far and whether it is running, finished,
failed or cancelled. A function that works something out from that
progress takes the `BatchProgress` as a parameter, and the action passes
it in.

## Related

- [Starting a run, and `key:`](../language/12-jobs-and-events.md#starting-a-run-and-key)
- [Progress, cancelling and the summary](../language/12-jobs-and-events.md#progress-cancelling-and-the-summary)
- [Batches](../language/12-jobs-and-events.md#batches)
