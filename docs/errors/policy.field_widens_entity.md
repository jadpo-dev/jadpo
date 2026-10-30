# `policy.field_widens_entity`

A field's `policy:` only takes away from its record's grants; here anyone signed in could read a note's summary when only its owner may read the note:

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
