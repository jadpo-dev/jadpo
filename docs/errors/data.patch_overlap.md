# `data.patch_overlap`

A `when_supplied:` block sets a field that the patch or another of its entries already sets.

## Why Jadpo refuses it

`when_supplied:` sets further fields when a request supplied a given one.
A field the patch already supplies would have two values, the request's
and this one, and the same goes for a field set under two supplied fields
when the request supplies both.

## What it looks like

main.jadpo:

```jadpo check project=e-patch-overlap expect=data.patch_overlap
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    nickname: ItemName?
}

type ItemChanges = Input {
    pick: [Item.name, Item.nickname]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
        when_supplied: {
            name: {
                nickname: none
            }
        }
    }
}
```

```text output
error: `nickname` is set by the patch and by `when_supplied:`
 --> main.jadpo:21:17
   |
21 |                 nickname: none
   |                 ^^^^^^^^ set twice
  reason: `ItemChanges` supplies `nickname` already, so a value here would compete with the request's.
  next: Remove `nickname` from `when_supplied:`, or from `ItemChanges` (agent)
  rule: data.patch_overlap
  help: https://jadpo.dev/docs/errors/data.patch_overlap

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field set under two supplied fields is refused the same way:

```jadpo check project=e-patch-overlap-twice expect=data.patch_overlap
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    nickname: ItemName?
    checked_at: Instant?
}

type ItemChanges = Input {
    pick: [Item.name, Item.nickname]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
        when_supplied: {
            name: {
                checked_at: none
            }
            nickname: {
                checked_at: none
            }
        }
    }
}
```

```text output
error: `checked_at` is set twice by `when_supplied:`
 --> main.jadpo:25:17
   |
22 |                 checked_at: none
   |                 ---------- first set here
25 |                 checked_at: none
   |                 ^^^^^^^^^^ set again
  reason: When the request supplies both fields, two values would compete for `checked_at`.
  next: Set `checked_at` under one supplied field (agent)
  rule: data.patch_overlap
  help: https://jadpo.dev/docs/errors/data.patch_overlap

check failed: 1 error, 1 file
```

## How to fix it

**If the request should decide the field**, remove it from
`when_supplied:`:

```jadpo fragment
action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```jadpo check project=e-patch-overlap-fix-1 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    nickname: ItemName?
}

type ItemChanges = Input {
    pick: [Item.name, Item.nickname]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

`nickname` now changes only when a request sends one.

**If your code should decide the field**, take it out of the input:

```jadpo fragment
type ItemChanges = Input {
    pick: [Item.name]
    optional: all
}
```

```jadpo check project=e-patch-overlap-fix-2 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    nickname: ItemName?
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
                nickname: none
            }
        }
    }
}
```

A new name now clears the nickname, and a request can't set one.

## Related

- [Patches from a request](../language/06-reads-and-writes.md#patches-from-a-request)
