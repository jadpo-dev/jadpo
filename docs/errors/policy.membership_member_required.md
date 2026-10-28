# `policy.membership_member_required`

```jadpo check project=e-membership-member-required expect=policy.membership_member_required
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
        role: role
    }
}
```

```text output
error: `StaffMembership`'s membership has no `member:`
 --> main.jadpo:29:5
   |
29 |     membership: {
   |     ^^^^^^^^^^ no `member:`
  reason: A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `member:` is who holds it: a reference to the user entity.
  next: Add `member:` to the membership (agent)
  rule: policy.membership_member_required
  help: docs/errors/policy.membership_member_required.md

check failed: 1 error, 1 file
```
