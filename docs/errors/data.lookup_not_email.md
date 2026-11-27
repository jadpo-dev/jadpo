# `data.lookup_not_email`

Code looks a record up by a field that doesn't hold an email address, and only an exact email address finds one person.

## Why Jadpo refuses it

A lookup finds one person by the one thing someone is given to find them
by: the exact address of their account. Finding by any other field would
let an action find records by what they hold, past the entity's `read:`.
So `<Entity>.lookup` takes only a field built on `Email`.

## What it looks like

main.jadpo:

```jadpo check project=e-lookup-not-email expect=data.lookup_not_email
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

policy {
    actions: {
        is_member: [Access.authenticated]
    }
}

type Handle = Text {
    max_length: 40
}

type Membership = Enum {
    member
    stranger
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

/// Whether an address has an account.
action is_member(handle: Member.handle) -> Membership {
    match Member.lookup(handle: handle) {
        none -> {
            return Membership.stranger
        }
        some -> {
            return Membership.member
        }
    }
}
```

```text output
error: `Member.lookup` finds by `handle`, which isn't built on `Email`
 --> main.jadpo:43:25
   |
43 |     match Member.lookup(handle: handle) {
   |                         ^^^^^^ `Handle`
  reason: A lookup finds one person by their exact email address, the one thing about them a person is given to find them by. Any other field would let an action find records by what they hold. `Member.handle` is `Handle`.
  next: Find the `Member` by a field built on `Email`, or by its `id` with `Member.get` (agent)
  rule: data.lookup_not_email
  help: https://jadpo.dev/docs/errors/data.lookup_not_email

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself: it names the email field.

## How to fix it

**If you are finding someone by their address**, look them up by the
field that holds it, an `Email` that is `unique: true`:

```jadpo fragment
    match Member.lookup(email: email) {
```

```jadpo check project=e-lookup-not-email-fix hidden
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

policy {
    actions: {
        is_member: [Access.authenticated]
    }
}

type Handle = Text {
    max_length: 40
}

type Membership = Enum {
    member
    stranger
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

/// Whether an address has an account.
action is_member(email: Member.email) -> Membership {
    match Member.lookup(email: email) {
        none -> {
            return Membership.stranger
        }
        some -> {
            return Membership.member
        }
    }
}
```

The lookup now finds the one member with that address, or none.

**If you know the record's `id`**, read it with `Member.get(member_id)`,
which its entity's `read:` decides:

```jadpo fragment
    var member: Member = attempt Member.get(member_id)
```

```jadpo check project=e-lookup-not-email-get hidden
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [Access.authenticated]
    }
}

/// A member's handle, by their `id`.
action handle_of(member_id: Member.id) fails MemberNotFound -> Member.handle {
    var member: Member = attempt Member.get(member_id)
    return member.handle
}
```

A caller then reads only what the entity's `read:` lets them.

## Related

- [Looking up one person by email](../language/15-personal-data.md#looking-up-one-person-by-email)
- [`data.lookup_not_unique`](data.lookup_not_unique.md)
