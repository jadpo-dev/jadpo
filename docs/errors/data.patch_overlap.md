# `data.patch_overlap`

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
