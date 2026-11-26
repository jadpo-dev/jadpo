# `query.include_hidden_unused`

A query says `include_hidden: true` for an entity no lifecycle hides records of.

## Why Jadpo refuses it

`include_hidden: true` lets a query read the records an entity's
lifecycle hides, such as those in the bin, as well as the visible ones.
Only a lifecycle with a `visible:` hides records. Without one, every
record is already read, so the line would read as though the query
reaches records that don't exist. A person's record that erasure kept is
another matter, which `include_erased: true` reaches.

## What it looks like

main.jadpo:

```jadpo check project=e-include-hidden-unused expect=query.include_hidden_unused
type Title = Text {
    max_length: 80
}

entity Note {
    id: Uuid
    title: Title
}

type NoteRow = Output {
    pick: [Note.id, Note.title]
}

query every_note() -> List<NoteRow> {
    from: Note
    order: [Note.title asc]
    limit: 20
    include_hidden: true
}
```

```text output
error: `every_note` includes hidden records, and no lifecycle hides a `Note`
 --> main.jadpo:18:5
   |
18 |     include_hidden: true
   |     ^^^^^^^^^^^^^^ nothing to include
  reason: `include_hidden: true` reaches the records a lifecycle's `visible:` leaves out, and `Note` has no lifecycle with a `visible:`, so every record of it is already read. A person's record erasure kept is reached by `include_erased: true` instead.
  next: Remove `include_hidden: true` (agent)
  rule: query.include_hidden_unused
  help: https://jadpo.dev/docs/errors/query.include_hidden_unused

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether a note should be hidden once deleted.

**If it shouldn't**, remove the line:

```jadpo check project=e-include-hidden-unused-removed
type Title = Text {
    max_length: 80
}

entity Note {
    id: Uuid
    title: Title
}

type NoteRow = Output {
    pick: [Note.id, Note.title]
}

query every_note() -> List<NoteRow> {
    from: Note
    order: [Note.title asc]
    limit: 20
}
```

The query reads every note there is.

**If it should**, give the entity a lifecycle that hides deleted notes
([Lifecycle](../language/05-data.md#lifecycle)):

```jadpo check project=e-include-hidden-unused-lifecycle
type Title = Text {
    max_length: 80
}

entity Note {
    id: Uuid
    title: Title
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
    }
}

type NoteRow = Output {
    pick: [Note.id, Note.title]
}

query every_note() -> List<NoteRow> {
    from: Note
    order: [Note.title asc]
    limit: 20
    include_hidden: true
}
```

The query now reads the deleted notes as well as the others.

## Related

- [Records a lifecycle hides](../language/06-reads-and-writes.md#records-a-lifecycle-hides)
- [`query.include_erased_unused`](query.include_erased_unused.md)
