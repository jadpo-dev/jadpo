# `data.lookup_not_email`

Code looks a record up by a field that isn't built on `Email`, and only an exact email address finds one person.

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
the fix itself.

## How to fix it

Find the person by their address, a field built on `Email` and `unique:
true`:

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

To read a record you know the `id` of, use `Member.get`, which its
entity's `read:` decides.

## Related

- [Looking up one person by email](../language/15-personal-data.md#looking-up-one-person-by-email)
- [`data.lookup_not_unique`](data.lookup_not_unique.md)
