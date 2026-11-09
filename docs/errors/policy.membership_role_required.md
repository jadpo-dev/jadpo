# `policy.membership_role_required`

A `membership:` doesn't say which field holds the role in `role:`.

## Why Jadpo refuses it

A membership is what turns an enum into a role someone holds, and it says
three things: where the role is held (`scope:`), who holds it (`member:`) and
which field holds it (`role:`). `role:` is the field typed with the role's enum. Without it, nobody could work out who holds the
role, so Jadpo can't check any grant that names it.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/policy.membership_role_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the record has a field of the role's enum**, add `role:`:

```jadpo fragment
    membership: {
        ...
        role: role
        ...
    }
```

```jadpo check project=e-membership-role-fix hidden
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

The record now gives its holder the role in `role`.

## Related

- [A membership role](../language/09-policy.md#a-membership-role)
- [Using a role on other records](../language/09-policy.md#using-a-role-on-other-records)
