# `type.lookup_context`

Code outside an action looks a person up by email with `<Entity>.lookup`.

## Why Jadpo refuses it

`<Entity>.lookup(email: address)` finds the one person whose account has
an address, whoever may read their record, so it runs only behind the
permission check of the action that calls it: that action's line under
root `policy { actions: }`. A function, a query, a test and a schedule's
code have no such line, so only an action calls it.

## What it looks like

main.jadpo:

```jadpo check project=e-lookup-context expect=type.lookup_context
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

test "someone has an account" {
    var found: Member? = Member.lookup(email: "ann@desk.example")
    assert found != none
}
```

```text output
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

Look the person up in an action listed under `policy { actions: }`, and
call that action:

```jadpo fragment
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

A test then calls `is_member`, as a route does, and proves what it
gives.

```jadpo check project=e-lookup-context-fix hidden
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

## Related

- [Looking up one person by email](../language/15-personal-data.md#looking-up-one-person-by-email)
- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
