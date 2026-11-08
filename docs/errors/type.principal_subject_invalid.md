# `type.principal_subject_invalid`

The `subject:` of `principal { }` isn't written as a field of a stored entity, `<Entity>.<field>`.

## Why Jadpo refuses it

Jadpo finds which record a signed-in caller is by looking them up in one
field, so `subject:` has to name that field, written `<Entity>.<field>`.
An entity's name alone doesn't say which field to look in.

## What it looks like

main.jadpo:

```jadpo check project=e-principal-subject-invalid expect=type.principal_subject_invalid
principal {
    user: {
        subject: Member
    }
}

entity Member {
    id: Uuid
}
```

```text output
error: `Member` isn't an entity's field
 --> main.jadpo:3:18
  |
3 |         subject: Member
  |                  ^^^^^^ not `<Entity>.<field>`
  reason: A user is looked up by a unique field of the entity it is stored as, written `<Entity>.<field>`, such as `User.email`.
  next: Name a unique field of the entity, such as `User.email` (agent)
  rule: type.principal_subject_invalid
  help: https://jadpo.dev/docs/errors/type.principal_subject_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the entity has a field that identifies one person**, name it. Here
`Member` needs one first, and it has to be unique:

```jadpo fragment
principal {
    user: {
        subject: Member.handle
    }
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```jadpo check project=e-principal-subject-invalid-fix hidden
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

Jadpo now finds a signed-in caller's record by their `handle`.

## Related

- [`subject:`](../language/08-identity.md#subject)
- [The principal](../language/08-identity.md#the-principal)
