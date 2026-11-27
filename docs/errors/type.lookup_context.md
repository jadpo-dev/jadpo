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

type AddressStanding = Enum {
    account_held
    no_account
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
error: A test can't call `Member.lookup`
 --> main.jadpo:36:26
   |
36 |     var found: Member? = Member.lookup(email: "ann@desk.example")
   |                          ^^^^^^^^^^^^^ outside an action
  reason: `Member.lookup` finds one person by their exact email address behind the permission check of the action that calls it, so only an action calls it, never a function, a query, a test or a schedule's code.
  next: Call `Member.lookup` in an action listed under `policy { actions: }`, and call that action from here (agent)
  rule: type.lookup_context
  help: https://jadpo.dev/docs/errors/type.lookup_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself: the lookup moves into an action.

## How to fix it

**If code outside an action needs to know who has an account**, look the
person up in an action listed under `policy { actions: }`, and call that
action:

```jadpo fragment
action has_account(email: Member.email) -> AddressStanding {
    match Member.lookup(email: email) {
        none -> {
            return AddressStanding.no_account
        }
        some -> {
            return AddressStanding.account_held
        }
    }
}
```

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
        has_account: [Access.authenticated]
    }
}

type Handle = Text {
    max_length: 40
}

type AddressStanding = Enum {
    account_held
    no_account
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
action has_account(email: Member.email) -> AddressStanding {
    match Member.lookup(email: email) {
        none -> {
            return AddressStanding.no_account
        }
        some -> {
            return AddressStanding.account_held
        }
    }
}
```

The lookup now runs behind `has_account`'s line, and a test or a route
calls `has_account` to learn whether an address has an account.

## Related

- [Looking up one person by email](../language/15-personal-data.md#looking-up-one-person-by-email)
- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
