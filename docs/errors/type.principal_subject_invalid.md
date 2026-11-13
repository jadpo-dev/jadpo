# `type.principal_subject_invalid`

The `subject:` of `principal { }` isn't written as a field of a stored entity, `<Entity>.<field>`, or with `external_token` names one whose type a token's text can't be read as.

## Why Jadpo refuses it

Jadpo finds which record a signed-in caller is by looking them up in one
field, so `subject:` has to name that field, written `<Entity>.<field>`.
An entity's name alone doesn't say which field to look in.

A provider's token names the person as text, which Jadpo reads as the
field's type reads text. With `external_token`, a field whose type has no
text form, such as an object kept as JSON, could never be named by a
token.

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

With `external_token`, a badge kept as JSON has no text a token could
send:

```jadpo check project=e-principal-subject-text expect=type.principal_subject_invalid
principal {
    user: {
        subject: Member.badge
    }
}

config {
    issuer: Url
    audience: Text
}

authentication {
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}

type BadgeNumber = Text {
    max_length: 20
}

type Badge = Object {
    site: BadgeNumber
    number: BadgeNumber
}

entity Member {
    id: Uuid
    badge: Badge {
        unique: true
    }
}
```

```text output
error: A token's subject can't be read as `Member.badge`
 --> main.jadpo:13:5
   |
13 |     external_token: {
   |     ^^^^^^^^^^^^^^ a token names its subject as text
  reason: A provider's token names the person as text, read as the `subject:` field's type reads it, and `Badge` has no text form, so no token could name a user.
  next: Make `subject:` a unique field whose type is read from text, such as `Text` or a type built on it (agent)
  rule: type.principal_subject_invalid
  help: https://jadpo.dev/docs/errors/type.principal_subject_invalid

check failed: 1 error, 1 file
```

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

**If a provider's token names the person**, name the field that holds the
subject the provider sends, as text:

```jadpo fragment
principal {
    user: {
        subject: Member.provider_subject
    }
}

entity Member {
    id: Uuid
    provider_subject: Text {
        unique: true
    }
    badge: Badge
}
```

```jadpo check project=e-principal-subject-text-fix hidden
principal {
    user: {
        subject: Member.provider_subject
    }
}

config {
    issuer: Url
    audience: Text
}

authentication {
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}

type BadgeNumber = Text {
    max_length: 20
}

type Badge = Object {
    site: BadgeNumber
    number: BadgeNumber
}

entity Member {
    id: Uuid
    provider_subject: Text {
        unique: true
    }
    badge: Badge
}
```

## Related

- [`subject:`](../language/08-identity.md#subject)
- [The principal](../language/08-identity.md#the-principal)
- [`external_token`](../language/08-identity.md#external_token)
