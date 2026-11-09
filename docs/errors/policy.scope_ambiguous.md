# `policy.scope_ambiguous`

An entity has two references a role or the tenant could apply through, and doesn't say which.

## Why Jadpo refuses it

A role held in a `Company` applies to a record through its reference to that
company. When a record has two such references, the source and the destination
of a transfer say, the role could apply through either, and which one you mean
decides who can see the record. Jadpo won't pick, because the wrong guess
exposes data.

The tenant has the same question: a record belongs to one tenant, so a record
with two references to the tenant must say which one is its.

## What it looks like

```jadpo check project=e-scope-ambiguous expect=policy.scope_ambiguous
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

entity InvoiceTransfer {
    id: Uuid
    source_company_id: Company.id {
        on_delete: OnDelete.restricted
    }
    destination_company_id: Company.id {
        on_delete: OnDelete.restricted
    }

    policy: {
        read: [CompanyRole.editor]
    }
}
```

```text output
error: `InvoiceTransfer` refers to `Company` more than once
 --> main.jadpo:54:16
   |
54 |         read: [CompanyRole.editor]
   |                ^^^^^^^^^^^^^^^^^^ through which reference?
  reason: `CompanyRole` is held in a `Company`, and `InvoiceTransfer` has 2 references to `Company` (`source_company_id` and `destination_company_id`), so the grant names the one it means.
  next: Write `CompanyRole.editor(source_company_id)` (agent)
  rule: policy.scope_ambiguous
  help: https://jadpo.dev/docs/errors/policy.scope_ambiguous

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The tenant is named the same way. With a tenant of `Company` and two
references to it, the record says which is its tenant:

```jadpo check project=e-scope-ambiguous-tenant expect=policy.scope_ambiguous
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

entity InvoiceTransfer {
    id: Uuid
    source_company_id: Company.id {
        on_delete: OnDelete.restricted
    }
    destination_company_id: Company.id {
        on_delete: OnDelete.restricted
    }

    policy: {
        read: [Access.authenticated]
    }
}

policy {
    tenant: Company
}
```

```text output
error: `InvoiceTransfer` refers to `Company` twice
 --> main.jadpo:49:8
   |
49 | entity InvoiceTransfer {
   |        ^^^^^^^^^^^^^^^ which one is its tenant?
  reason: `InvoiceTransfer` has 2 references to `Company`, the tenant, so its `policy:` says which one its records belong to.
  next: Add `tenant: source_company_id` to its `policy:` (agent)
  rule: policy.scope_ambiguous
  help: https://jadpo.dev/docs/errors/policy.scope_ambiguous

check failed: 1 error, 1 file
```

## How to fix it

**If the role applies through one of the references**, name it in the grant:

```jadpo fragment
    policy: {
        read: [CompanyRole.editor(source_company_id)]
    }
```

```jadpo check project=e-scope-ambiguous-fix-1 hidden
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

entity InvoiceTransfer {
    id: Uuid
    source_company_id: Company.id {
        on_delete: OnDelete.restricted
    }
    destination_company_id: Company.id {
        on_delete: OnDelete.restricted
    }

    policy: {
        read: [CompanyRole.editor(source_company_id)]
    }
}
```

Editors of the source company can now read the transfer, and editors of the
destination can't.

**If the record belongs to a tenant**, add `tenant:` to its `policy:`:

```jadpo fragment
    policy: {
        tenant: source_company_id
        ...
    }
```

```jadpo check project=e-scope-ambiguous-fix-2 hidden
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

entity InvoiceTransfer {
    id: Uuid
    source_company_id: Company.id {
        on_delete: OnDelete.restricted
    }
    destination_company_id: Company.id {
        on_delete: OnDelete.restricted
    }

    policy: {
        tenant: source_company_id
        read: [Access.authenticated]
    }
}

policy {
    tenant: Company
}
```

The transfer now belongs to the source company.

## Related

- [Using a role on other records](../language/09-policy.md#using-a-role-on-other-records)
- [Naming the tenant](../language/09-policy.md#naming-the-tenant)
- [Tenancy](../language/09-policy.md#tenancy)
