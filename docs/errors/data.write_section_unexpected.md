# `data.write_section_unexpected`

A write has a section it doesn't take, gives one twice, or gives `conflict:` as anything but a block.

## Why Jadpo refuses it

A create takes `set:` and `conflict:`. An update takes `set:` or `patch:`,
with `when_supplied:` beside `patch:`, and `conflict:`. A delete takes
`conflict:`, and a transition only `conflict:`, since the fields it
changes are fixed in the lifecycle. Each is given once, so there is never
a question of which counts. A record that isn't there reports the entity's
not-found failure itself, so there is nothing for a `missing:` to map.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.write_section_unexpected

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`set:` and `patch:` together are refused the same way:

```jadpo check project=e-write-unexpected-both expect=data.write_section_unexpected
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

action edit(item: Item, name: Item.name, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        set: {
            name: name
        }
        patch: amendment
    }
}
```

```text output
error: `Item.update` takes no `patch:`
 --> main.jadpo:20:9
   |
20 |         patch: amendment
   |         ^^^^^ not a section here
  reason: An update gives its fields in `set:`, or changes the fields a request supplied with `patch:`, never both.
  next: Remove `patch: ...` (agent)
  rule: data.write_section_unexpected
  help: https://jadpo.dev/docs/errors/data.write_section_unexpected

check failed: 1 error, 1 file
```

So is a section given twice:

```jadpo check project=e-write-unexpected-twice expect=data.write_section_unexpected
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
        set: {
            name: name
        }
    }
}
```

```text output
error: `set:` is given twice
 --> main.jadpo:15:9
   |
12 |         set: {
   |         ------ first given here
15 |         set: {
   |         ^^^ given again
  reason: Each section of `Item.update` is given once, so there is never a question of which counts.
  next: Remove one `set:` (agent)
  rule: data.write_section_unexpected
  help: https://jadpo.dev/docs/errors/data.write_section_unexpected

check failed: 1 error, 1 file
```

So is `when_supplied:` with no `patch:`, since nothing was supplied:

```jadpo check project=e-write-unexpected-when expect=data.write_section_unexpected
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
        when_supplied: {
            name: {
                name: name
            }
        }
    }
}
```

```text output
error: `Item.update` has `when_supplied:` and no `patch:`
 --> main.jadpo:15:9
   |
15 |         when_supplied: {
   |         ^^^^^^^^^^^^^ nothing supplied
  reason: `when_supplied:` sets further fields when the patch's request supplied a field; without `patch:` nothing is supplied.
  next: Change the fields with `patch: input`, or give them in `set:` (agent)
  rule: data.write_section_unexpected
  help: https://jadpo.dev/docs/errors/data.write_section_unexpected

check failed: 1 error, 1 file
```

So is a `conflict:` that isn't a block of rules, one per line:

```jadpo check project=e-write-unexpected-conflict expect=data.write_section_unexpected
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemClash = Failure {
    kind: FailureKind.conflict
}

action add_item(name: Item.name) fails ItemClash -> Item {
    return attempt Item.create {
        set: {
            name: name
        }
        conflict: ItemClash
    }
}
```

```text output
error: `conflict:` maps each rule in a block
 --> main.jadpo:19:19
   |
19 |         conflict: ItemClash
   |                   ^^^^^^^^^ one failure for every rule
  reason: Each rule a write can break is mapped on its own line, keyed by the rule, so there is no catch-all.
  next: Write `conflict: { Item.rule: ... }` (agent)
  rule: data.write_section_unexpected
  help: https://jadpo.dev/docs/errors/data.write_section_unexpected

check failed: 1 error, 1 file
```

So is `set:` on a transition, which fixes what it changes in its
lifecycle:

```jadpo check project=e-write-unexpected-transition expect=data.write_section_unexpected
entity Item {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            archive: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}

action archive_it(item: Item, at: Item.archived_at) -> Item {
    return Item.archive(item) {
        set: {
            archived_at: at
        }
    }
}
```

```text output
error: `Item.archive` takes no `set:`
 --> main.jadpo:18:9
   |
18 |         set: {
   |         ^^^ not a section here
  reason: `Item.archive` is a lifecycle transition: the fields it changes are fixed in the lifecycle's `set:`, so it takes only `conflict:`.
  next: Remove `set: ...` (agent)
  rule: data.write_section_unexpected
  help: https://jadpo.dev/docs/errors/data.write_section_unexpected

check failed: 1 error, 1 file
```

## How to fix it

**If the section doesn't belong in this write, or is given twice**, remove
it:

```jadpo fragment
action rename(item_id: Item.id, name: Item.name) fails ItemNotFound -> Item {
    return attempt Item.update(item_id) {
        set: {
            name: name
        }
    }
}
```

```jadpo check project=e-write-unexpected-fix-1 hidden
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action rename(item_id: Item.id, name: Item.name) fails ItemNotFound -> Item {
    return attempt Item.update(item_id) {
        set: {
            name: name
        }
    }
}
```

A record that isn't there now reports `ItemNotFound`, by itself.

**If the write can break a rule**, give `conflict:` a block, with one line
for each rule:

```jadpo fragment
        conflict: {
            Customer.email: EmailTaken
        }
```

```jadpo check project=e-write-unexpected-fix-2 hidden
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
}

type EmailTaken = Failure {
    kind: FailureKind.conflict
}

action register(email: Customer.email) fails EmailTaken -> Customer {
    return attempt Customer.create {
        set: {
            email: email
        }
        conflict: {
            Customer.email: EmailTaken
        }
    }
}
```

A second customer with the same address now gets `EmailTaken`.

## Related

- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
- [When a write is refused](../language/06-reads-and-writes.md#when-a-write-is-refused)
