# `policy.role_field_create_forbidden`

A note's `create:` names only `owner_id`, so a note is only ever created by its owner, and `owner_id` is set from the caller rather than the request:

```jadpo check project=e-role-field-create-forbidden expect=policy.role_field_create_forbidden
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
        create: [owner_id]
        read: [owner_id]
    }
}

type CreateNote = Input {
    owner_id: User.id
    text: NoteText
}

action create_note(input: CreateNote) fails UserNotFound -> Note {
    return attempt Note.create {
        set: {
            owner_id: input.owner_id
            text: input.text
        }
    }
}
```

```text output
error: `create_note` sets `Note.owner_id` from the request, not the caller
 --> main.jadpo:43:23
   |
43 |             owner_id: input.owner_id
   |                       ^^^^^^^^^^^^^^ not the caller
  reason: `Note`'s `create:` names only `owner_id`, so a `Note` is only ever created by the caller it points at. Set from the request, the create would be refused for every value but the caller's own, so it is set from the caller: the `id` of a `Principal.user` the action takes.
  next: Take the caller as a parameter typed `Principal.user`, and set `owner_id` to its `id` (agent)
  rule: policy.role_field_create_forbidden
  help: docs/errors/policy.role_field_create_forbidden.md

check failed: 1 error, 1 file
```

The fix takes the caller, `create_note(owner: Principal.user, input: CreateNote)`, and sets `owner_id: owner.id`.
