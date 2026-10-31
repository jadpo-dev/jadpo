# `type.field_update_forbidden`

An update changes a field that stays fixed once its record exists, such as an owner field a grant names or a field marked `immutable: true`.

## Why Jadpo refuses it

A user field that a grant names, such as `Note.owner_id` in
`read: [owner_id]`, decides who a record belongs to. Changing it in an
ordinary update would hand the record to someone else, under the grants
for editing it. Handing a record over is a decision of its own, so it is
a named action, listed under `actions:` with who may run it.

A field marked `immutable: true` keeps the value its record was created
with, so no update may change it either.

## What it looks like

main.jadpo:

```jadpo check project=e-field-update-forbidden expect=type.field_update_forbidden
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

    policy: {
        read: [owner_id]
        update: [owner_id]
    }
}

action give_note(note_id: Note.id, to: User.id) fails NoteNotFound, UserNotFound -> Note {
    return attempt Note.update(note_id) {
        set: {
            owner_id: to
        }
    }
}
```

```text output
error: `Note.update` changes `Note.owner_id`, which an update can't change
 --> main.jadpo:38:13
   |
38 |             owner_id: to
   |             ^^^^^^^^ named in policy
  reason: `Note`'s policy reaches the caller or the tenant through `owner_id`, so changing it hands the record to someone else. That is a named action, listed with who may call it under `actions:` in `policy.jadpo`.
  next: Remove `owner_id` from the update, or list `give_note` under `actions:` in `policy.jadpo` (agent)
  rule: type.field_update_forbidden
  help: https://jadpo.dev/docs/errors/type.field_update_forbidden

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field marked `immutable: true` is refused the same way:

```jadpo check project=e-field-update-immutable expect=type.field_update_forbidden
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
    reference: NoteText {
        immutable: true
    }

    policy: {
        read: [owner_id]
        update: [owner_id]
    }
}

action renumber_note(note_id: Note.id, reference: NoteText) fails NoteNotFound -> Note {
    return attempt Note.update(note_id) {
        set: {
            reference: reference
        }
    }
}
```

```text output
error: `Note.update` changes `Note.reference`, which an update can't change
 --> main.jadpo:41:13
   |
41 |             reference: reference
   |             ^^^^^^^^^ immutable
  reason: `Note.reference` says `immutable: true`: it keeps the value its record was created with.
  next: Remove `reference` from the update, or `immutable: true` from the field (agent)
  rule: type.field_update_forbidden
  help: https://jadpo.dev/docs/errors/type.field_update_forbidden

check failed: 1 error, 1 file
```

## How to fix it

If the update wasn't meant to change the field, remove the field from its
`set:`. Otherwise the fix depends on the field.

**If the action hands a note to someone else**, list it under `actions:`
with who may hand it over, here the note's current owner. In a project
this goes in `policy.jadpo`:

```jadpo fragment
policy {
    actions: {
        give_note: [Note.owner_id]
    }
}
```

```jadpo check project=e-field-update-forbidden-listed hidden
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

    policy: {
        read: [owner_id]
        update: [owner_id]
    }
}

action give_note(note_id: Note.id, to: User.id) fails NoteNotFound, UserNotFound -> Note {
    return attempt Note.update(note_id) {
        set: {
            owner_id: to
        }
    }
}

policy {
    actions: {
        give_note: [Note.owner_id]
    }
}
```

The owner of a note may now give it to another user. On `give_note`'s
own update, its line replaces `Note`'s `update:` grant, so nobody else
may hand the note over.

**If a field marked `immutable: true` should be able to change**, remove
`immutable: true` from it:

```jadpo fragment
    reference: NoteText
```

```jadpo check project=e-field-update-immutable-removed hidden
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
    reference: NoteText

    policy: {
        read: [owner_id]
        update: [owner_id]
    }
}

action renumber_note(note_id: Note.id, reference: NoteText) fails NoteNotFound -> Note {
    return attempt Note.update(note_id) {
        set: {
            reference: reference
        }
    }
}
```

`renumber_note` may now change a note's `reference`, for anyone `Note`'s
`update:` lets in.

## Related

- [The field can't be changed by an update](../language/09-policy.md#the-field-cant-be-changed-by-an-update)
- [A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
- [Actions](../language/09-policy.md#actions)
