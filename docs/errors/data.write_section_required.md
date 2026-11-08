# `data.write_section_required`

A create or an update doesn't say which fields it sets.

## Why Jadpo refuses it

A create gives every field the store doesn't generate, in `set:`. An
update gives the fields it changes in `set:`, or the request's changes
with `patch:`. A write that names none of these would write nothing, or a
record with empty fields, so it can only be a mistake.

## What it looks like

main.jadpo:

```jadpo check project=e-write-section-required expect=data.write_section_required
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action touch(item: Item) -> Item {
    return Item.update(item)
}
```

```text output
error: `Item.update` doesn't say what it changes
 --> main.jadpo:11:12
   |
11 |     return Item.update(item)
   |            ^^^^^^^^^^^ nothing to change
  reason: An update gives the fields it changes in `set:`, or the request's changes with `patch:`.
  next: Add `set: { ... }` with the fields it changes (agent)
  rule: data.write_section_required
  help: https://jadpo.dev/docs/errors/data.write_section_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A create with no `set:` is refused the same way:

```jadpo check project=e-write-required-create expect=data.write_section_required
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action make() -> Item {
    return Item.create {}
}
```

```text output
error: `Item.create` gives no fields
 --> main.jadpo:11:12
   |
11 |     return Item.create {}
   |            ^^^^^^^^^^^ nothing to change
  reason: A new `Item` gets every field the store doesn't generate, given in `set:`.
  next: Add `set: { ... }` with the record's fields (agent)
  rule: data.write_section_required
  help: https://jadpo.dev/docs/errors/data.write_section_required

check failed: 1 error, 1 file
```

So is a `set:` with no fields in an update:

```jadpo check project=e-write-required-empty expect=data.write_section_required
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action touch(item: Item) -> Item {
    return Item.update(item) {
        set: {}
    }
}
```

```text output
error: `Item.update`'s `set:` gives no fields
 --> main.jadpo:12:9
   |
12 |         set: {}
   |         ^^^ nothing to change
  reason: An update changes at least one field: an empty `set:` would write nothing.
  next: Add the fields it changes to `set:` (agent)
  rule: data.write_section_required
  help: https://jadpo.dev/docs/errors/data.write_section_required

check failed: 1 error, 1 file
```

## How to fix it

**If the write changes a known field**, set it in `set:`:

```jadpo fragment
action rename(item: Item, name: Item.name) -> Item {
    return Item.update(item) {
        set: {
            name: name
        }
    }
}
```

```jadpo check project=e-write-required-fix-1 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action rename(item: Item, name: Item.name) -> Item {
    return Item.update(item) {
        set: {
            name: name
        }
    }
}
```

The item's name now changes to the one given.

**If a request decides what changes**, take its `Input` with `patch:`:

```jadpo fragment
action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```jadpo check project=e-write-required-fix-2 hidden
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

The item now changes only in the fields the request sends.

## Related

- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
- [Patches from a request](../language/06-reads-and-writes.md#patches-from-a-request)
