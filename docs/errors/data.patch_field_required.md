# `data.patch_field_required`

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
  help: docs/language/06-reads-and-writes.html#data-patch-field-required

check failed: 1 error, 1 file
```
