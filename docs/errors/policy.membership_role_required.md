# `policy.membership_role_required`

```jadpo check project=e-membership-role-required expect=policy.membership_role_required
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
    }
}
```

```text output
error: `StaffMembership`'s membership has no `role:`
 --> main.jadpo:29:5
   |
29 |     membership: {
   |     ^^^^^^^^^^ no `role:`
  reason: A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `role:` is which field holds the role, a field typed with the role's enum.
  next: Add `role:` to the membership (agent)
  rule: policy.membership_role_required
  help: docs/errors/policy.membership_role_required.md

check failed: 1 error, 1 file
```
