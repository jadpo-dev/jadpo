# `data.patch_field_unknown`

Each field of a patch's input is one of the entity's, and `when_supplied:`
names the input's fields:

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
