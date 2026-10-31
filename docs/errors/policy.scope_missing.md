# `policy.scope_missing`

An entity names a role held in another record, or belongs to the app's tenant, but has no reference to that record.

## Why Jadpo refuses it

A membership role is held in a record, its scope, such as an editor of a
company. A role applies to another entity's record through that record's
reference to the scope: an editor of a company may read the ledgers whose
`company_id` names that company. A `Ledger` with no `company_id` has no
company, so `CompanyRole.editor` could never let anyone in.

Naming a tenant in root `policy { }` works the same way. Every grant on a
tenant's records applies only within the tenants where the caller has a
membership, so each entity needs its own reference to the tenant, or
says that every tenant shares it.

## What it looks like

main.jadpo:

```jadpo check project=e-scope-missing expect=policy.scope_missing
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

entity Ledger {
    id: Uuid

    policy: {
        read: [CompanyRole.editor]
    }
}
```

```text output
error: `Ledger` has no reference to `Company`
 --> main.jadpo:48:16
   |
48 |         read: [CompanyRole.editor]
   |                ^^^^^^^^^^^^^^^^^^ a role held in a `Company`
  reason: `CompanyRole` is held in a `Company` (`CompanyMembership`'s `scope:`), and a role applies to a record through the record's reference to its scope, so `Ledger` needs a field typed `Company.id`.
  next: Add a reference to `Company` to `Ledger`, or name someone else (agent)
  rule: policy.scope_missing
  help: https://jadpo.dev/docs/errors/policy.scope_missing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

When root `policy { }` names `Company` as the tenant, an entity with no
reference to `Company` is refused the same way, whatever its grants name:

```jadpo check project=e-scope-missing-tenant expect=policy.scope_missing
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
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

entity Ledger {
    id: Uuid

    policy: {
        read: [Access.authenticated]
    }
}

policy {
    tenant: Company
}
```

```text output
error: `Ledger` doesn't belong to a `Company`
 --> main.jadpo:48:8
   |
48 | entity Ledger {
   |        ^^^^^^ no reference to `Company`
  reason: Root `policy { }` names `Company` as the tenant, so every entity carries its own reference to it and its grants apply only within the `Company`s where the caller has a membership; an entity every tenant shares says `tenant: Tenancy.shared`.
  next: Add a field typed `Company.id` to `Ledger`, or `tenant: Tenancy.shared` to its `policy:` (agent)
  rule: policy.scope_missing
  help: https://jadpo.dev/docs/errors/policy.scope_missing

check failed: 1 error, 1 file
```

## How to fix it

The question is whether a ledger belongs to a company.

**If each ledger belongs to a company**, add a reference to `Company`.
The same field is the ledger's tenancy reference when `Company` is the
tenant:

```jadpo fragment
entity Ledger {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    ...
}
```

```jadpo check project=e-scope-missing-reference hidden
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

entity Ledger {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [CompanyRole.editor]
    }
}
```

A company's editors may now read the ledgers whose `company_id` names
that company, and no one else may. Deleting a company deletes its
ledgers.

**If ledgers belong to no company, in an app with no tenant**, name
someone the grant can check without one, such as anyone signed in:

```jadpo fragment
        read: [Access.authenticated]
```

```jadpo check project=e-scope-missing-someone-else hidden
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

entity Ledger {
    id: Uuid

    policy: {
        read: [Access.authenticated]
    }
}
```

Anyone signed in may read every ledger.

**If every tenant shares the ledgers**, say so with
`tenant: Tenancy.shared` in `Ledger`'s `policy:`:

```jadpo fragment
    policy: {
        tenant: Tenancy.shared
        read: [Access.authenticated]
    }
```

```jadpo check project=e-scope-missing-shared hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
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

entity Ledger {
    id: Uuid

    policy: {
        tenant: Tenancy.shared
        read: [Access.authenticated]
    }
}

policy {
    tenant: Company
}
```

A ledger belongs to no company, and its grants apply as they would
without tenancy, so anyone signed in may read every ledger. A role held
in a company still can't be named in its grants, since a ledger has no
company.

## Related

- [Using a role on other records](../language/09-policy.md#using-a-role-on-other-records)
- [A membership role](../language/09-policy.md#a-membership-role)
- [Naming the tenant](../language/09-policy.md#naming-the-tenant)
- [Tenancy](../language/09-policy.md#tenancy)
