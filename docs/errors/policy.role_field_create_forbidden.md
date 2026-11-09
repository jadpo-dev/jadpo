# `policy.role_field_create_forbidden`

An action sets the user field a `create:` grant depends on from the request instead of the caller.

## Why Jadpo refuses it

A `create:` that names only a user field says a record is only ever created by
the person it points at. A value that comes from the request can be anyone's,
so the create would fail for every value but the caller's own. Taking the id
from the caller makes the rule true by construction, and leaves nothing for a
client to get wrong.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/policy.role_field_create_forbidden

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether anyone creates this record on someone else's behalf.

**If people only create their own**, take the caller as a parameter and set
the field from it:

```jadpo fragment
action create_note(owner: Principal.user, input: CreateNote) -> Note {
    return Note.create {
        set: {
            owner_id: owner.id
            text: input.text
        }
    }
}
```

```jadpo check project=e-role-field-create-forbidden-fix-1 hidden
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
    text: NoteText
}

action create_note(owner: Principal.user, input: CreateNote) -> Note {
    return Note.create {
        set: {
            owner_id: owner.id
            text: input.text
        }
    }
}
```

A note now always belongs to whoever created it, and the request no longer
carries `owner_id`.

**If someone may create it for another person**, grant `create:` to who may do
that, so the field no longer decides it:

```jadpo fragment
    policy: {
        create: [Access.authenticated]
        ...
    }
```

```jadpo check project=e-role-field-create-forbidden-fix-2 hidden
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
        create: [Access.authenticated]
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

Anyone signed in may now create a note for any user, so choose this only when
that is what you mean.

## Related

- [A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
- [On a create](../language/09-policy.md#on-a-create)
- [Grants](../language/09-policy.md#grants)
