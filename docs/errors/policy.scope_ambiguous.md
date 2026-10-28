# `policy.scope_ambiguous`

Two references to `Company`, and the grant does not say which one the role applies through; `CompanyRole.editor(source_company_id)` would:

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
  help: docs/errors/policy.scope_ambiguous.md

check failed: 1 error, 1 file
```
