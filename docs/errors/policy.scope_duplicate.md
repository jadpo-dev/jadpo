# `policy.scope_duplicate`

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
