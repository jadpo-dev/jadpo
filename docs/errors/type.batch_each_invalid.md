# `type.batch_each_invalid`

A batch's `each:` or `finished:` is something other than one call of an action in its domain, or a file write's `each:` something other than one inline function giving the row.

## Why Jadpo refuses it

A run calls `each:`'s action for every item long after the action that
started it has finished, so it can keep only two kinds of argument: the
item itself, or a field inside it, which it reads from each item; and a
value that reads nothing of the item, which it works out as it starts
and saves. Anything worked out of the item is neither, so it has no
place in the run. That work belongs in the action, where it is checked
and runs in the item's transaction with the rest. A file helper's write
is the other way round: its `each:` is one inline function giving the
row to write for each item.

## What it looks like

main.jadpo, beside a `stock.jadpo` that declares `Item`, its page
settings `ItemPage`, a paged query `items` and the action `mark_seen`:

```jadpo check project=e-batch-each-invalid file=stock.jadpo hidden
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

```jadpo check project=e-batch-each-invalid expect=type.batch_each_invalid
action mark_code(code: ShelfCode) -> Unit {
    assert code == code
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_code(ShelfCode(item.code)))
}
```

```text output
error: `batches.each`'s `each:` isn't one call of an action
 --> main.jadpo:6:68
  |
6 |     return batches.each(from: items(page), each: item -> mark_code(ShelfCode(item.code)))
  |                                                                    ^^^^^^^^^^^^^^^^^^^^ the item and more
  reason: An argument reading `item` is `item` itself or a field inside it, which the run reads from each item; one that reads nothing of it is worked out now and saved with the run. Work the rest out in the action it calls.
  next: Write `each: item -> <action>(...)`, calling an action in this domain (agent)
  rule: type.batch_each_invalid
  help: https://jadpo.dev/docs/errors/type.batch_each_invalid

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is what the action needs from each item.

**If it needs a field of the item as it is**, pass the field:

```jadpo fragment
action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_code(item.code))
}
```

```jadpo check project=e-batch-each-invalid-fix-1 file=main.jadpo hidden
action mark_code(code: ShelfCode) -> Unit {
    assert code == code
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_code(item.code))
}
```

```jadpo check project=e-batch-each-invalid-fix-1 file=stock.jadpo hidden
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

The run reads `code` from each item as it reaches it and passes it to
`mark_code`.

**If it works something out of the item**, pass the item and do the
work in the action:

```jadpo fragment
action mark_code(item: Item) -> Unit {
    var code: ShelfCode = ShelfCode(item.code)
    assert code == code
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_code(item))
}
```

```jadpo check project=e-batch-each-invalid-fix-2 file=main.jadpo hidden
action mark_code(item: Item) -> Unit {
    var code: ShelfCode = ShelfCode(item.code)
    assert code == code
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_code(item))
}
```

```jadpo check project=e-batch-each-invalid-fix-2 file=stock.jadpo hidden
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

The work now runs inside the item's own transaction, so an item it
fails for is undone on its own and becomes a problem in the run's
summary, while the rest carry on.

## Related

- [`each:` and `finished:`](../language/12-jobs-and-events.md#each-and-finished)
- [Saved values](../language/12-jobs-and-events.md#saved-values)
- [Writing a file](../language/12-jobs-and-events.md#writing-a-file)
- [Problems never stop a run](../language/12-jobs-and-events.md#problems-never-stop-a-run)
