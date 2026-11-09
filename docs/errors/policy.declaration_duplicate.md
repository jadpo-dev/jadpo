# `policy.declaration_duplicate`

An entity declares `policy:` twice, or the app declares root `policy { }` twice.

## Why Jadpo refuses it

Each entity's grants are in one `policy:`, and the app's tenant and its listed
actions are in one root `policy { }`. Then "who can delete this?" is always one
line in one place, for you, for a reviewer and for the permissions report.

Two declarations would need a rule for which one counts, and any such rule
would let a line that looks like a grant quietly mean nothing.

## What it looks like

```jadpo check project=e-declaration-duplicate expect=policy.declaration_duplicate
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
    }

    policy: {
        update: [owner_id]
    }
}
```

```text output
error: `Note` declares `policy:` twice
 --> main.jadpo:28:5
   |
24 |     policy: {
   |     ------ first declared here
28 |     policy: {
   |     ^^^^^^ declared again
  reason: An entity's grants are one list per effect in one `policy:`, so "who can delete this?" is always one line.
  next: Move what the second `policy:` holds into the first, and remove it (agent)
  rule: policy.declaration_duplicate
  help: https://jadpo.dev/docs/errors/policy.declaration_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Root `policy { }` is declared once too:

```jadpo check project=e-declaration-duplicate-root expect=policy.declaration_duplicate
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

policy {
    actions: {
        check_health: [Access.public]
    }
}
```

```text output
error: `policy { }` is declared twice
 --> main.jadpo:27:1
   |
21 | policy {
   | ------ first declared here
27 | policy {
   | ^^^^^^ declared again
  reason: Application-wide policy is declared once, in `policy.jadpo`, so one place says what the tenant is and which actions are exceptions.
  next: Move what this `policy` holds into the first, and remove it (agent)
  rule: policy.declaration_duplicate
  help: https://jadpo.dev/docs/errors/policy.declaration_duplicate

check failed: 1 error, 1 file
```

## How to fix it

**If the second `policy:` adds grants**, move them into the first and remove
the second:

```jadpo fragment
    policy: {
        read: [owner_id]
        update: [owner_id]
    }
```

```jadpo check project=e-declaration-duplicate-fix-1 hidden
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

The entity now says who may read and who may update it in one place.

**If root `policy { }` is written twice**, keep one, holding every `actions:`
line. In a project it lives in `policy.jadpo`:

```jadpo fragment
policy {
    actions: {
        check_health: [Access.public]
    }
}
```

```jadpo check project=e-declaration-duplicate-fix-2 hidden
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

One block now says which actions are listed and who may run them.

## Related

- [Grants](../language/09-policy.md#grants)
- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
