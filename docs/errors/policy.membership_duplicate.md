# `policy.membership_duplicate`

An entity declares `membership:` twice, or gives its `member:`, `role:` or `requires:` twice.

## Why Jadpo refuses it

A membership entity stores one role, held by one member in one place, and
says so once. Two `membership:` sections, or two `member:` lines, leave open
which one counts when someone's access is worked out.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/policy.membership_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A part of a membership is given once too:

```jadpo check project=e-membership-duplicate-part expect=policy.membership_duplicate
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
        member: user_id
        role: role
    }
}
```

```text output
error: `member:` is given twice
 --> main.jadpo:32:9
   |
31 |         member: user_id
   |         ------ first given here
32 |         member: user_id
   |         ^^^^^^ given again
  reason: A membership says once where its role is held, who holds it and which field holds it, so there is never a question of which counts.
  next: Remove one `member:` (agent)
  rule: policy.membership_duplicate
  help: https://jadpo.dev/docs/errors/policy.membership_duplicate

check failed: 1 error, 1 file
```

`scope:` given twice is its own rule,
[`policy.scope_duplicate`](policy.scope_duplicate.md).

## How to fix it

**If the two say the same thing**, keep one:

```jadpo fragment
    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }
```

```jadpo check project=e-membership-duplicate-fix-1 hidden
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

The record now holds one role, for one member, across the application.

**If a person can hold a second kind of role**, keep it in an entity of its
own:

```jadpo fragment
entity ReviewerMembership {
    ...
    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

```jadpo check project=e-membership-duplicate-fix-2 hidden
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

type ReviewerRole = Enum {
    reviewer
}

entity ReviewerMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: ReviewerRole

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

Each kind of role now has its own record, and a person can hold both.

## Related

- [A membership role](../language/09-policy.md#a-membership-role)
- [Using a role on other records](../language/09-policy.md#using-a-role-on-other-records)
