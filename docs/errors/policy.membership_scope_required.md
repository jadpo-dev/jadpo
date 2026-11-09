# `policy.membership_scope_required`

A `membership:` doesn't say what the role is held in.

## Why Jadpo refuses it

A membership is what turns an enum into a role someone holds, and it says
three things: where the role is held (`scope:`), who holds it (`member:`) and
which field holds it (`role:`). A role means different things depending on
where it is held: a steward of one company, or a steward of the whole
application. Jadpo won't guess which you meant, because that decides which
records the role reaches.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/policy.membership_scope_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the role is held across the whole application or in
one record.

**If it is held across the whole application**, name
`MembershipScope.application`:

```jadpo fragment
    membership: {
        scope: MembershipScope.application
        ...
    }
```

```jadpo check project=e-membership-scope-required-fix-1 hidden
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

The role now applies everywhere its enum is granted.

**If it is held in one record**, add a reference to that record and name it:

```jadpo fragment
entity StaffMembership {
    ...
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    ...
    membership: {
        scope: company_id
        ...
    }
}
```

```jadpo check project=e-membership-scope-required-fix-2 hidden
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

entity Company {
    id: Uuid
}

entity StaffMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }
}
```

Each membership now holds the role in the one company it points at.

## Related

- [A membership role](../language/09-policy.md#a-membership-role)
- [Using a role on other records](../language/09-policy.md#using-a-role-on-other-records)
