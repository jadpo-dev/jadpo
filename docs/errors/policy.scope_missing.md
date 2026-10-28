# `policy.scope_missing`

`Ledger` names a company role but has no reference to `Company`, which the role is held in; an entity with no reference to the tenant is refused the same way:

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
  help: docs/language/09-policy.html#policy-scope-missing

check failed: 1 error, 1 file
```
