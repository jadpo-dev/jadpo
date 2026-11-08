# `data.patch_field_required`

A patch's `Input` has a field that isn't optional.

## Why Jadpo refuses it

A patch changes only the fields a request supplied: one left out keeps its
value. A required field of the `Input` would have to be sent every time,
so it could never be left out, and the patch wouldn't be a patch for that
field.

## What it looks like

main.jadpo:

```jadpo check project=e-patch-field-required expect=data.patch_field_required
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemChanges = Input {
    pick: [Item.name]
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```text output
error: `ItemChanges.name` isn't optional, so a patch can't leave it out
 --> main.jadpo:16:16
   |
16 |         patch: amendment
   |                ^^^^^^^^^ `ItemChanges`
  reason: A patch changes only the fields a request supplied, so each field of its input is optional: one left out keeps its value. A required `name` would have to be sent every time.
  next: Make `name` optional in `ItemChanges` (`optional: true`, or `optional: all`), or set it with `set:` (agent)
  rule: data.patch_field_required
  help: https://jadpo.dev/docs/errors/data.patch_field_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a request may leave the field out**, make the input's fields
optional:

```jadpo fragment
type ItemChanges = Input {
    pick: [Item.name]
    optional: all
}
```

```jadpo check project=e-patch-field-required-fix-1 hidden
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

A request that leaves `name` out now keeps the stored name.

**If the field always changes**, set it with `set:`:

```jadpo fragment
action edit(item: Item, name: Item.name) -> Item {
    return Item.update(item) {
        set: {
            name: name
        }
    }
}
```

```jadpo check project=e-patch-field-required-fix-2 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action edit(item: Item, name: Item.name) -> Item {
    return Item.update(item) {
        set: {
            name: name
        }
    }
}
```

Every call now gives a new name.

## Related

- [Patches from a request](../language/06-reads-and-writes.md#patches-from-a-request)
