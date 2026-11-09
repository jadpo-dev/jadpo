# `policy.effect_unknown`

A `policy:`, a `membership:` or root `policy { }` holds something that isn't one of its settings.

## Why Jadpo refuses it

An entity's `policy:` has four grants, `create`, `read`, `update` and `delete`,
and may name its `tenant:`. Every action takes its permission from those four,
so a new word like `publish` would be a grant nothing ever checks: it would
read like protection and protect nothing.

The other blocks are as fixed. A membership holds `scope:`, `member:`, `role:`
and `requires:`, and root `policy { }` holds `tenant:` and `actions:`.

## What it looks like

```jadpo check project=e-effect-unknown expect=policy.effect_unknown
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

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        publish: [owner_id]
    }
}
```

```text output
error: `publish` isn't something a policy grants
 --> main.jadpo:25:9
   |
25 |         publish: [owner_id]
   |         ^^^^^^^ not a grant or a setting
  reason: An entity's `policy:` lists who may `create`, `read`, `update` and `delete` its records, and may say which `tenant:` they belong to. Every action takes its permission from these; one that needs others is listed under `actions:` in `policy.jadpo`.
  next: Remove `publish` (agent)
  rule: policy.effect_unknown
  help: https://jadpo.dev/docs/errors/policy.effect_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A membership holds only its own parts. A word close to one of them gets a
suggestion:

```jadpo check project=e-effect-unknown-membership expect=policy.effect_unknown
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
        rol: role
        role: role
    }
}
```

```text output
error: `rol` isn't part of a membership
 --> main.jadpo:32:9
   |
32 |         rol: role
   |         ^^^ not a part of a membership
  reason: A membership holds `scope:`, `member:`, `role:` and, optionally, `requires:`. Did you mean `role`?
  next: Use `role` (agent)
  rule: policy.effect_unknown
  help: https://jadpo.dev/docs/errors/policy.effect_unknown

check failed: 1 error, 1 file
```

Root `policy { }` holds only `tenant:` and `actions:`:

```jadpo check project=e-effect-unknown-root expect=policy.effect_unknown
type Health = Output {
    ok: Bool
}

action check_health() -> Health {
    return Health {
        ok: true
    }
}

routes Checkup {
    prefix: /health

    POST / {
        authentication: RouteAuthentication.none
        output: Health
        run: check_health()
    }
}

policy {
    actions: {
        check_health: [Access.public]
    }
    owners: []
}
```

```text output
error: `owners` isn't part of root `policy { }`
 --> main.jadpo:25:5
   |
25 |     owners: []
   |     ^^^^^^ not part of root policy
  reason: Root `policy { }` holds `tenant:` and `actions:`; everything else is written beside each entity.
  next: Remove `owners`, or write it in the entity's `policy:` (agent)
  rule: policy.effect_unknown
  help: https://jadpo.dev/docs/errors/policy.effect_unknown

check failed: 1 error, 1 file
```

A field's own `policy:` narrows `create`, `read` and `update`, or says
`FieldAccess.server_only`, and anything else is refused the same way.

## How to fix it

**If what you want is a change to the record**, grant `update:` and let the
action make the change:

```jadpo fragment
    policy: {
        update: [owner_id]
    }
```

```jadpo check project=e-effect-unknown-fix-1 hidden
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

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
        update: [owner_id]
    }
}
```

An action that changes a note now takes its permission from `update:`, so its
owner can run it and nobody else can.

**If the action needs people the record's grants don't name**, list it under
`actions:` in root `policy { }`:

```jadpo fragment
policy {
    actions: {
        check_health: [Access.public]
    }
}
```

```jadpo check project=e-effect-unknown-fix-2 hidden
type Health = Output {
    ok: Bool
}

action check_health() -> Health {
    return Health {
        ok: true
    }
}

routes Checkup {
    prefix: /health

    POST / {
        authentication: RouteAuthentication.none
        output: Health
        run: check_health()
    }
}

policy {
    actions: {
        check_health: [Access.public]
    }
}
```

The listed line now says who may run the action, separately from the records
it reaches.

## Related

- [Grants](../language/09-policy.md#grants)
- [Which grant an action needs](../language/09-policy.md#which-grant-an-action-needs)
- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
- [Field restrictions](../language/09-policy.md#field-restrictions)
