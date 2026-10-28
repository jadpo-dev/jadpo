# `type.field_update_forbidden`

A user field named in policy is never changed by an ordinary update; handing the note to someone else is an action listed under `actions:`. A field marked `immutable: true` is refused the same way:

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
  help: docs/language/09-policy.html#type-field-update-forbidden

check failed: 1 error, 1 file
```
