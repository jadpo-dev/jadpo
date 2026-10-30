# `type.batch_each_invalid`

`each:` and `finished:` are one call of an action in the same domain,
whose arguments are the item, a field inside it, or a value saved with
the run. (A file helper's write's `each:` is one inline function giving
the item's row instead, refused when it is anything else.)
One that works something out of the item is refused:

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
