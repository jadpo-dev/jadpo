# `policy.membership_scope_required`

A membership says where its role is held: a reference, or `MembershipScope.application`:

```jadpo check project=e-membership-scope-required expect=policy.membership_scope_required
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
        member: user_id
        role: role
    }
}
```

```text output
error: `StaffMembership`'s membership has no `scope:`
 --> main.jadpo:29:5
   |
29 |     membership: {
   |     ^^^^^^^^^^ no `scope:`
  reason: A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `scope:` is what the role is held in: a field typed `<Entity>.id`, or `MembershipScope.application` for a role held across the whole application.
  next: Add `scope:` to the membership (agent)
  rule: policy.membership_scope_required
  help: docs/errors/policy.membership_scope_required.md

check failed: 1 error, 1 file
```
