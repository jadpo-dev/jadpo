# `policy.lookup_unchecked`

An action looks a person up by email with no permission check of its own before it: it isn't listed under `actions:`, its line names `Access.public`, or its line can't be checked before the lookup.

## Why Jadpo refuses it

`<Entity>.lookup(email: address)` finds a person whoever may read their
record, and whether it finds one tells the caller whether the address
has an account. So it runs only behind its action's own line under root
`policy { actions: }`, which says who may look people up, checked as the
action starts:

- anyone signed in, or a role held across the whole application;
- or a role held in a record, such as `WorkspaceRole.owner`, checked in
  the record whose `id` the action is given as its one parameter of
  that type.

An action that isn't listed, a line naming `Access.public`, and a line
naming only who a record the action hasn't found yet decides, such as
`Todo.owner_id`, leave nothing to check before the lookup.

## What it looks like

main.jadpo:

```jadpo check project=e-lookup-unchecked expect=policy.lookup_unchecked
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
error: `is_member` looks a `Member` up by email with no permission check before it
 --> main.jadpo:37:11
   |
37 |     match Member.lookup(email: email) {
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^ no permission check of its own
  reason: `Member.lookup` finds a person by their exact address whoever may read `Member`, so it runs only behind its action's own line under root `policy { actions: }`, which says who may look people up. `is_member` isn't listed.
  next: List `is_member` under `policy { actions: }` with who may look people up, such as `[<Role>.owner]` (human) (human)
  rule: policy.lookup_unchecked
  help: https://jadpo.dev/docs/errors/policy.lookup_unchecked

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may learn whether an address has an account.

## How to fix it

**If anyone signed in may look people up**, list the action for them:

```jadpo fragment
policy {
    actions: {
        is_member: [Access.authenticated]
    }
}
```

```jadpo check project=e-lookup-unchecked-fix hidden
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

Only someone signed in now reaches the lookup.

## Related

- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
- [`type.lookup_context`](type.lookup_context.md)
