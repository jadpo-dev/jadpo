# `policy.field_widens_entity`

A field's own `policy:` lets in someone its record's grants don't.

## Why Jadpo refuses it

A field follows its record's grants unless its own `policy:` says
otherwise, and a field's `policy:` can only take away. To read a field, a
caller must first be allowed to read the record it is in. So a field
list naming someone the record's list doesn't name lets nobody extra in,
and it reads as if it does.

Here `Note.summary` names anyone signed in, but only a note's owner may
read the note, so its summary is the owner's too.

## What it looks like

main.jadpo:

```jadpo check project=e-field-widens-entity expect=policy.field_widens_entity
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    summary: NoteText {
        policy: {
            read: [Access.authenticated]
        }
    }

    policy: {
        read: [owner_id]
    }
}
```

```text output
error: `Note.summary` lets `Access.authenticated` read it, which `Note` doesn't
 --> main.jadpo:30:20
   |
30 |             read: [Access.authenticated]
   |                    ^^^^^^^^^^^^^^^^^^^^ not in `Note`'s `read:`
  reason: A field's `policy:` only takes away from its record's grants: it narrows who may read the field, and never lets in anyone `Note`'s `read:` doesn't, which names `owner_id`.
  next: Remove `Access.authenticated` from `summary`'s `read:` (agent)
  rule: policy.field_widens_entity
  help: https://jadpo.dev/docs/errors/policy.field_widens_entity

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is who should read the summary.

**If only the note's owner should**, remove the field's `policy:`, so
`summary` follows `Note`'s grants:

```jadpo fragment
    text: NoteText
    summary: NoteText

    policy: {
        read: [owner_id]
    }
```

```jadpo check project=e-field-widens-entity-follows hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    summary: NoteText

    policy: {
        read: [owner_id]
    }
}
```

Only the owner reads a note and its summary, as before, and the field no
longer reads as if anyone else can.

**If anyone signed in should read the summary**, widen the record's
`read:` to them, and narrow the fields only the owner may read:

```jadpo fragment
    text: NoteText {
        policy: {
            read: [owner_id]
        }
    }
    summary: NoteText

    policy: {
        read: [Access.authenticated]
    }
```

```jadpo check project=e-field-widens-entity-record hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText {
        policy: {
            read: [owner_id]
        }
    }
    summary: NoteText

    policy: {
        read: [Access.authenticated]
    }
}
```

Anyone signed in may now read a note's `id`, `owner_id` and `summary`,
and only its owner may read its `text`. A query whose output carries
`text` is then refused, since not everyone who may read a note may read
its text
([`policy.output_field_unproved`](../errors/policy.output_field_unproved.md)).

## Related

- [Field restrictions](../language/09-policy.md#field-restrictions)
- [Grants](../language/09-policy.md#grants)
- [Anyone signed in, or anyone](../language/09-policy.md#anyone-signed-in-or-anyone)
