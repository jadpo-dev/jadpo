# `policy.scope_duplicate`

A membership gives `scope:` twice, or a policy gives `tenant:` twice.

## Why Jadpo refuses it

A membership is held in one place, and a record belongs to one tenant, so each
says it once. Two lines would need a rule for which one counts, and that rule
would decide which records someone can reach.

## What it looks like

```jadpo check project=e-scope-duplicate expect=policy.scope_duplicate
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
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

```text output
error: `scope:` is given twice
 --> main.jadpo:31:9
   |
30 |         scope: MembershipScope.application
   |         ----- first given here
31 |         scope: MembershipScope.application
   |         ^^^^^ given again
  reason: A membership says once where its role is held, who holds it and which field holds it, so there is never a question of which counts.
  next: Remove one `scope:` (agent)
  rule: policy.scope_duplicate
  help: https://jadpo.dev/docs/errors/policy.scope_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`tenant:` is given once too, in an entity's `policy:` or in root
`policy { }`:

```jadpo check project=e-scope-duplicate-tenant expect=policy.scope_duplicate
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
    tenant: Company
}
```

```text output
error: `tenant:` is given twice
 --> main.jadpo:51:5
   |
50 |     tenant: Company
   |     ------ first given here
51 |     tenant: Company
   |     ^^^^^^ given again
  reason: Root `policy { }` says once what the tenant is and which actions are exceptions, so there is never a question of which counts.
  next: Remove one `tenant:` (agent)
  rule: policy.scope_duplicate
  help: https://jadpo.dev/docs/errors/policy.scope_duplicate

check failed: 1 error, 1 file
```

## How to fix it

**If the two lines say the same thing**, keep one:

```jadpo fragment
    membership: {
        scope: MembershipScope.application
        ...
    }
```

```jadpo check project=e-scope-duplicate-fix-1 hidden
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

The role is now held in one place.

**If `tenant:` is given twice**, keep the one you mean:

```jadpo fragment
policy {
    tenant: Company
}
```

```jadpo check project=e-scope-duplicate-fix-2 hidden
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

The app now names its tenant once.

## Related

- [A membership role](../language/09-policy.md#a-membership-role)
- [Naming the tenant](../language/09-policy.md#naming-the-tenant)
