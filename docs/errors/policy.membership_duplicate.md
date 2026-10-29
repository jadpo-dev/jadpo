# `policy.membership_duplicate`

An entity stores one role, held by one member in one place:

```jadpo check project=e-membership-duplicate expect=policy.membership_duplicate
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

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

```text output
error: `StaffMembership` declares `membership:` twice
 --> main.jadpo:35:5
   |
29 |     membership: {
   |     ---------- first declared here
35 |     membership: {
   |     ^^^^^^^^^^ declared again
  reason: A membership entity stores one role, held in one place by one member, so it says so once.
  next: Remove one `membership:`, or keep the second role in an entity of its own (agent)
  rule: policy.membership_duplicate
  help: docs/errors/policy.membership_duplicate.md

check failed: 1 error, 1 file
```
