# `data.patch_not_input`

A patch is given something other than an `Input`.

## Why Jadpo refuses it

A patch changes only the fields a request supplied, which is something
only an `Input` remembers: one field left out keeps its value, and one
sent as `none` is cleared. A record or an object has every field, so there
is no telling what should change.

## What it looks like

main.jadpo:

```jadpo check project=e-patch-not-input expect=data.patch_not_input
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action copy_name(item: Item, from: Item) -> Item {
    return Item.update(item) {
        patch: from
    }
}
```

```text output
error: `patch:` takes an `Input`, and this is `Item`
 --> main.jadpo:12:16
   |
12 |         patch: from
   |                ^^^^ this is `Item`
  reason: A patch changes only the fields a request supplied, so it takes an `Input` whose fields are optional and named after `Item`'s: one left out keeps its value, and one sent as `none` is cleared.
  next: Pass an `Input` such as `type PatchItem = Input { pick: [...]  optional: all }` (agent)
  rule: data.patch_not_input
  help: https://jadpo.dev/docs/errors/data.patch_not_input

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the changes come from a request**, take an `Input` of the fields it
may change:

```jadpo fragment
type ItemChanges = Input {
    pick: [Item.name]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```jadpo check project=e-patch-not-input-fix-1 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemChanges = Input {
    pick: [Item.name]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

A request now changes only the fields it sends.

**If your code decides the new values**, set them with `set:`:

```jadpo fragment
action copy_name(item: Item, from: Item) -> Item {
    return Item.update(item) {
        set: {
            name: from.name
        }
    }
}
```

```jadpo check project=e-patch-not-input-fix-2 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action copy_name(item: Item, from: Item) -> Item {
    return Item.update(item) {
        set: {
            name: from.name
        }
    }
}
```

The item now takes the other item's name.

## Related

- [Patches from a request](../language/06-reads-and-writes.md#patches-from-a-request)
