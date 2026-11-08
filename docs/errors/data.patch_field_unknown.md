# `data.patch_field_unknown`

A patch's `Input` has a field the entity doesn't, or its `when_supplied:` names a field the `Input` doesn't have.

## Why Jadpo refuses it

A patch sets each field its input supplies on the entity's field of the
same name, so every field of the input has to be one of the entity's.
`when_supplied:` acts when the request supplied one of the patch's fields,
so it has to name one of the input's.

## What it looks like

main.jadpo:

```jadpo check project=e-patch-field-unknown expect=data.patch_field_unknown
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemChanges = Input {
    title: ItemName {
        optional: true
    }
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```text output
error: `ItemChanges` has `title`, which `Item` doesn't
 --> main.jadpo:18:16
   |
18 |         patch: amendment
   |                ^^^^^^^^^ `ItemChanges`
  reason: A patch sets each field its input supplies on the field of `Item` with the same name, so every field of `ItemChanges` is one of `Item`'s.
  next: Remove `title` from `ItemChanges` (agent)
  rule: data.patch_field_unknown
  help: https://jadpo.dev/docs/errors/data.patch_field_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `when_supplied:` that names a field of no `Input` is refused the same
way:

```jadpo check project=e-patch-unknown-when expect=data.patch_field_unknown
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    checked_at: Instant?
}

type ItemChanges = Input {
    pick: [Item.name]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
        when_supplied: {
            title: {
                checked_at: none
            }
        }
    }
}
```

```text output
error: `when_supplied:` names `title`, which `ItemChanges` doesn't have
 --> main.jadpo:20:13
   |
20 |             title: {
   |             ^^^^^ not a field of the input
  reason: `when_supplied:` acts when the request supplied one of the patch's fields; `ItemChanges` has no `title`.
  next: Name one of `ItemChanges`'s fields (agent)
  rule: data.patch_field_unknown
  help: https://jadpo.dev/docs/errors/data.patch_field_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the input's field means one of the entity's**, give it that name:

```jadpo fragment
type ItemChanges = Input {
    name: ItemName {
        optional: true
    }
}
```

```jadpo check project=e-patch-field-unknown-fix-1 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemChanges = Input {
    name: ItemName {
        optional: true
    }
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

A request that supplies `name` now changes the item's name.

**If `when_supplied:` names the wrong field**, name one the input has:

```jadpo fragment
        when_supplied: {
            name: {
                checked_at: none
            }
        }
```

```jadpo check project=e-patch-field-unknown-fix-2 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    checked_at: Instant?
}

type ItemChanges = Input {
    pick: [Item.name]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
        when_supplied: {
            name: {
                checked_at: none
            }
        }
    }
}
```

A request that supplies `name` now also clears `checked_at`.

## Related

- [Patches from a request](../language/06-reads-and-writes.md#patches-from-a-request)
