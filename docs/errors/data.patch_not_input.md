# `data.patch_not_input`

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
