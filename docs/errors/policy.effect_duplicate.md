# `policy.effect_duplicate`

A policy gives the same grant list twice, or lists one action twice under `actions:`.

## Why Jadpo refuses it

Each effect has one list, so "who may read a note?" is always one line. Two
lists for one effect would need a rule for whether they add up or one replaces
the other, and a reviewer reading either line alone would get the wrong answer.

An action listed twice under `actions:` has the same problem: its permission
comes from one line.

## What it looks like

```jadpo check project=e-effect-duplicate expect=policy.effect_duplicate
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
        read: [Access.authenticated]
    }
}
```

```text output
error: `read:` is given twice
 --> main.jadpo:26:9
   |
25 |         read: [owner_id]
   |         ---- first given here
26 |         read: [Access.authenticated]
   |         ^^^^ given again
  reason: `Note`'s policy gives each list once, so "who may read?" is always one line.
  next: Merge the two `read:` lists into one (agent)
  rule: policy.effect_duplicate
  help: https://jadpo.dev/docs/errors/policy.effect_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An action listed twice under `actions:` is the same mistake:

```jadpo check project=e-effect-duplicate-action expect=policy.effect_duplicate
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
        check_health: [Access.public]
    }
}
```

```text output
error: `check_health` is listed twice
 --> main.jadpo:24:9
   |
23 |         check_health: [Access.public]
   |         ------------ first listed here
24 |         check_health: [Access.public]
   |         ^^^^^^^^^^^^ listed again
  reason: An action takes its permission from one line, so there is never a question of which counts.
  next: Merge the two lines into one (agent)
  rule: policy.effect_duplicate
  help: https://jadpo.dev/docs/errors/policy.effect_duplicate

check failed: 1 error, 1 file
```

`actions:` itself is given once, and so is each list in a field's own
`policy:`.

## How to fix it

**If both lists are wanted**, merge them into one:

```jadpo fragment
    policy: {
        read: [owner_id, Access.authenticated]
    }
```

```jadpo check project=e-effect-duplicate-fix-1 hidden
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
        read: [owner_id, Access.authenticated]
    }
}
```

Anyone signed in can now read a note, which the single list says outright.

**If an action is listed twice**, keep one line, with everyone who may run it:

```jadpo fragment
policy {
    actions: {
        check_health: [Access.public]
    }
}
```

```jadpo check project=e-effect-duplicate-fix-2 hidden
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

The action now takes its permission from that one line.

## Related

- [Grants](../language/09-policy.md#grants)
- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
- [Field restrictions](../language/09-policy.md#field-restrictions)
