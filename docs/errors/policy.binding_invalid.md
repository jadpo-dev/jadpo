# `policy.binding_invalid`

A grant, a membership or a `tenant:` names something that can't say who is allowed or where.

## Why Jadpo refuses it

A policy is checked against the caller, so everything in it has to be
something Jadpo can compare with the caller: a field that is a reference to
the principal's user entity, a role a membership holds, or a value of
`Access`. A field typed `Uuid` might hold a user's id, but nothing says it
does, so a grant on it could never be proved.

The same goes for what a membership says. Its `member:` must be the user, its
`scope:` a reference, and its `role:` a field of the role's enum.

Jadpo can't choose what you meant. Who may do something is the point of the
policy, so the line has to say it in terms the compiler can check.

## What it looks like

```jadpo check project=e-binding-invalid expect=policy.binding_invalid
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

entity Note {
    id: Uuid
    owner_id: Uuid

    policy: {
        read: [owner_id]
    }
}
```

```text output
error: `owner_id` doesn't point at a caller
 --> main.jadpo:23:16
   |
23 |         read: [owner_id]
   |                ^^^^^^^^ not a user field
  reason: A field names who may do this when it points at the caller: a reference to `User`, the principal's user entity, typed `User.id`. `Note.owner_id` isn't.
  next: Name a reference to the user, a membership role, or `Access` (agent)
  rule: policy.binding_invalid
  help: https://jadpo.dev/docs/errors/policy.binding_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`member:` is who holds a membership's role, so it has to be a reference to
the user entity, and a field holding some other value can't say who:

```jadpo check project=e-binding-member expect=policy.binding_invalid
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

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    label: Handle
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: label
        role: role
    }
}
```

```text output
error: `member: label` doesn't point at a user
 --> main.jadpo:32:9
   |
32 |         member: label
   |         ^^^^^^ not a reference to the user entity
  reason: A role is held by a user, so a membership's `member:` names its reference to `User`, the entity the principal's `user:` names.
  next: Name the field that refers to the user who holds the role (agent)
  rule: policy.binding_invalid
  help: https://jadpo.dev/docs/errors/policy.binding_invalid

check failed: 1 error, 1 file
```

A grant, `scope:`, `role:`, `tenant:` and each list in a policy are refused
the same way when what they name can't do the job: a role with no such
value, a value of `Access` that doesn't exist, or a grant that isn't a list.

Under `actions:`, an action in a project with domains is named through its
domain, as a call from another domain names it. A bare name that is one
domain's action gets the edit that writes its domain, and the `use` line
that brought it in isn't also reported:

```jadpo check project=e-binding-bare-action file=app.jadpo hidden
application {}
```

```jadpo check project=e-binding-bare-action file=domains/lending/actions/borrow.jadpo hidden
action borrow() -> Unit {}
```

```jadpo check project=e-binding-bare-action file=policy.jadpo expect=policy.binding_invalid
use lending/actions { borrow }

policy {
    actions: {
        borrow: [Access.public]
    }
}
```

```text output
error: `borrow` belongs to domain `lending`
 --> policy.jadpo:5:9
  |
5 |         borrow: [Access.public]
  |         ^^^^^^ needs its domain
  reason: Root `policy { }` names each action through its domain, `domain.action`, as a call from outside the domain does; a bare name is for a single-file project.
  next: Write `lending.borrow`; `jadpo fmt` then imports `lending` in place of `borrow` (agent)
  rule: policy.binding_invalid
  help: https://jadpo.dev/docs/errors/policy.binding_invalid

check failed: 1 error, 3 files
```

## How to fix it

**If the field holds who the record belongs to**, type it as a reference to
the user:

```jadpo fragment
entity Note {
    ...
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    ...
}
```

```jadpo check project=e-binding-invalid-fix-1 hidden
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

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }
}
```

Only the person `owner_id` points at can now read the note.

**If a membership's `member:` names the wrong field**, name the reference to
the user:

```jadpo fragment
    membership: {
        ...
        member: user_id
        ...
    }
```

```jadpo check project=e-binding-member-fix hidden
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

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

Each `StaffMembership` now gives its role to the user it points at.

**If an action under `actions:` has no domain**, write the domain in front of
it. In `policy.jadpo`:

```jadpo fragment
use lending

policy {
    actions: {
        lending.borrow: [Access.public]
    }
}
```

```jadpo check project=e-binding-bare-action-fix file=policy.jadpo hidden
use lending

policy {
    actions: {
        lending.borrow: [Access.public]
    }
}
```

```jadpo check project=e-binding-bare-action-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-binding-bare-action-fix file=domains/lending/actions/borrow.jadpo hidden
action borrow() -> Unit {}
```

The line now names the action as a call from another domain would, and
`jadpo fmt` writes the `use` line for you.

## Related

- [A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
- [A membership role](../language/09-policy.md#a-membership-role)
- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
- [Anyone signed in, or anyone](../language/09-policy.md#anyone-signed-in-or-anyone)
