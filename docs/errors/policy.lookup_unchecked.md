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
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because the choice is yours: who may look people up.

## How to fix it

List the action with who may look people up:

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

An owner who adds people to their workspace by email lists the action
for `[WorkspaceRole.owner]` and gives it the workspace's `id`, so only an
owner of that workspace looks anyone up
([Looking up one person by email](../language/15-personal-data.md#looking-up-one-person-by-email)).

## Related

- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
- [`type.lookup_context`](type.lookup_context.md)
