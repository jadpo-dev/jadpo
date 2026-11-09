# `policy.membership_member_required`

A `membership:` doesn't say who holds the role in `member:`.

## Why Jadpo refuses it

A membership is what turns an enum into a role someone holds, and it says
three things: where the role is held (`scope:`), who holds it (`member:`) and
which field holds it (`role:`). `member:` is the reference to the user who has the role. Without it, nobody could work out who holds the
role, so Jadpo can't check any grant that names it.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/policy.membership_member_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the record has a reference to the user**, add `member:`:

```jadpo fragment
    membership: {
        ...
        member: user_id
        ...
    }
```

```jadpo check project=e-membership-member-fix hidden
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

The role now goes to the user `user_id` points at.

## Related

- [A membership role](../language/09-policy.md#a-membership-role)
- [Using a role on other records](../language/09-policy.md#using-a-role-on-other-records)
