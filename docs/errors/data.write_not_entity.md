# `data.write_not_entity`

Only an entity is stored, so only an entity has `create`, `update` and
`delete`:

```jadpo check project=e-write-not-entity expect=data.write_not_entity
type DraftName = Text {
    max_length: 80
}

type Draft = Input {
    name: DraftName
}

action save_draft(name: DraftName) -> Draft {
    return Draft.create {
        set: {
            name: name
        }
    }
}
```

```text output
error: `Draft` isn't an entity, so it has no `create`
 --> main.jadpo:10:12
   |
10 |     return Draft.create {
   |            ^^^^^^^^^^^^ an input
  reason: Only an entity is stored, so only an entity has `create`, `update` and `delete`; `Draft` is an input, a value built with `Draft { ... }`.
  next: Write the entity this value is stored as, or build the value with `Draft { ... }` (agent)
  rule: data.write_not_entity
  help: docs/errors/data.write_not_entity.md

check failed: 1 error, 1 file
```
