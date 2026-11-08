# `data.write_not_entity`

A write calls `create`, `update` or `delete` on a type that isn't an entity.

## Why Jadpo refuses it

Only an entity is stored, so only an entity has `create`, `update` and
`delete`. `Draft` is a value that exists while your code runs, and there
is nowhere for it to be written.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.write_not_entity

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you only need the value**, build it with its type's name:

```jadpo fragment
action save_draft(name: DraftName) -> Draft {
    return Draft {
        name: name
    }
}
```

```jadpo check project=e-write-not-entity-fix-1 hidden
type DraftName = Text {
    max_length: 80
}

type Draft = Input {
    name: DraftName
}

action save_draft(name: DraftName) -> Draft {
    return Draft {
        name: name
    }
}
```

The action now returns the value and stores nothing.

**If the value should be kept**, make it an entity:

```jadpo fragment
entity Draft {
    id: Uuid
    name: DraftName
}
```

```jadpo check project=e-write-not-entity-fix-2 hidden
type DraftName = Text {
    max_length: 80
}

entity Draft {
    id: Uuid
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

Each call now adds a stored `Draft`, with an `id`.

## Related

- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
