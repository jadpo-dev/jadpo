# `policy.role_unbound`

A grant names a role no membership holds, or the app's tenant has no membership held in it.

## Why Jadpo refuses it

A role exists only where a membership stores it, because a membership is what
says who holds it. Declaring the enum gives the role to nobody, so a grant on
it could never let anyone in, and the app would refuse everyone without saying
why.

A tenant is the same. Every grant on a tenant's records applies only within
the tenants where the caller has a membership, so a tenant nobody can belong
to would make every record in the app unreachable.

## What it looks like

```jadpo check project=e-role-unbound expect=policy.role_unbound
type NoteRole = Enum {
    owner
}

entity Note {
    id: Uuid

    policy: {
        read: [NoteRole.owner]
    }
}
```

```text output
error: No membership holds a `NoteRole`
 --> main.jadpo:9:16
  |
9 |         read: [NoteRole.owner]
  |                ^^^^^^^^^^^^^^ a role nobody can hold
  reason: A role exists only where it is stored: in a membership entity whose `membership:` names a field typed `NoteRole` as its `role:`. Declaring the enum alone gives the role to nobody.
  next: Add a membership entity holding `NoteRole`, or name someone else in the grant (agent)
  rule: policy.role_unbound
  help: https://jadpo.dev/docs/errors/policy.role_unbound

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`tenant:` needs a membership held in the tenant:

```jadpo check project=e-role-unbound-tenant expect=policy.role_unbound
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Company {
    id: Uuid
}

policy {
    tenant: Company
}
```

```text output
error: No membership is held in `Company`
 --> main.jadpo:28:13
   |
28 |     tenant: Company
   |             ^^^^^^^ a tenant nobody belongs to
  reason: Every grant on a tenant's records applies only within the `Company`s where the caller has a membership, so the tenant needs a membership entity whose `scope:` refers to `Company`; without one nobody could reach anything.
  next: Add a membership entity with `scope:` a reference to `Company` (agent)
  rule: policy.role_unbound
  help: https://jadpo.dev/docs/errors/policy.role_unbound

check failed: 1 error, 1 file
```

## How to fix it

The question is whether this role, or tenant, should exist at all.

**If people should hold the role**, add a membership entity holding it:

```jadpo fragment
entity NoteMembership {
    ...
    membership: {
        scope: note_id
        member: user_id
        role: role
    }
}
```

```jadpo check project=e-role-unbound-fix-1 hidden
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

type NoteRole = Enum {
    owner
}

entity Note {
    id: Uuid

    policy: {
        read: [NoteRole.owner]
    }
}

entity NoteMembership {
    id: Uuid
    note_id: Note.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: NoteRole

    membership: {
        scope: note_id
        member: user_id
        role: role
    }
}
```

A person now has the role on each note a membership record ties them to.

**If nobody needs the role**, name someone else in the grant:

```jadpo fragment
    policy: {
        read: [Access.authenticated]
    }
```

```jadpo check project=e-role-unbound-fix-2 hidden
entity Note {
    id: Uuid

    policy: {
        read: [Access.authenticated]
    }
}
```

Anyone signed in can now read a note, and the unused enum can go.

**If the tenant has no membership**, add one whose `scope:` refers to it:

```jadpo fragment
entity CompanyMembership {
    ...
    membership: {
        scope: company_id
        member: user_id
        role: role
    }
}
```

```jadpo check project=e-role-unbound-fix-3 hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type CompanyRole = Enum {
    owner
    editor
}

entity Company {
    id: Uuid
}

entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }
}

policy {
    tenant: Company
}
```

People can now belong to a company, and reach that company's records through
the grants on them.

## Related

- [A membership role](../language/09-policy.md#a-membership-role)
- [Tenancy](../language/09-policy.md#tenancy)
- [Naming the tenant](../language/09-policy.md#naming-the-tenant)
