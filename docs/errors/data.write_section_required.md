# `data.write_section_required`

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
  help: docs/errors/data.write_section_required.md

check failed: 1 error, 1 file
```
