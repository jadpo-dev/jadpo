# `data.write_section_unexpected`

A create takes `set:` and `conflict:`, an update `set:` or `patch:` (with
`when_supplied:`) and `conflict:`, a delete `conflict:`, each once. A
record that isn't there reports the entity's not-found failure itself:

```jadpo check project=e-write-section-unexpected expect=data.write_section_unexpected
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemMissing = Failure {
    kind: FailureKind.not_found
}

action rename(item_id: Item.id, name: Item.name) fails ItemNotFound -> Item {
    return attempt Item.update(item_id) {
        set: {
            name: name
        }
        missing: ItemMissing
    }
}
```

```text output
error: `Item.update` takes no `missing:`
 --> main.jadpo:19:9
   |
19 |         missing: ItemMissing
   |         ^^^^^^^ not a section here
  reason: A record that isn't there reports `ItemNotFound` itself; `missing:` maps a query's "nothing matched".
  next: Remove `missing: ...` (agent)
  rule: data.write_section_unexpected
  help: docs/errors/data.write_section_unexpected.md

check failed: 1 error, 1 file
```
