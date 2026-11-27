# `data.lookup_not_unique`

Code looks a record up by an email field that isn't `unique: true` across the whole table, so an address could find several.

## Why Jadpo refuses it

A lookup gives one record or none. If several records could hold one
address, which came back would show more than one person's. So the field
must be `unique: true`, and its entity one every tenant shares: a
tenant's unique rule holds only within the tenant, so two tenants'
records may hold the same address.

## What it looks like

main.jadpo:

```jadpo check project=e-lookup-not-unique expect=data.lookup_not_unique
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
    email: Email

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

```text output
error: `Member.lookup` finds by `email`, which isn't `unique: true`
 --> main.jadpo:41:25
   |
41 |     match Member.lookup(email: email) {
   |                         ^^^^^ not unique
  reason: A lookup gives one record or none. Without `unique: true` several `Member` records could hold one address, and which came back would show more than one person's.
  next: Make `Member.email` `unique: true`, if one address is one person's (human) (agent)
  rule: data.lookup_not_unique
  help: https://jadpo.dev/docs/errors/data.lookup_not_unique

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because the choice is yours: whether one address is one
person's.

## How to fix it

**If one address is one person's**, make the field `unique: true`:

```jadpo fragment
    email: Email {
        unique: true
    }
```

```jadpo check project=e-lookup-not-unique-fix hidden
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

**If several records may share an address**, find them some other way,
such as by a record that refers to them.

## Related

- [Looking up one person by email](../language/15-personal-data.md#looking-up-one-person-by-email)
- [A unique rule holds within its tenant](../language/09-policy.md#a-unique-rule-holds-within-its-tenant)
