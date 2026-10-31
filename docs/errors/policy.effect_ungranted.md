# `policy.effect_ungranted`

A route runs an action that nobody is allowed to call, because nothing in the app says who may.

## Why Jadpo refuses it

An action takes its permission from the stored records it reads and
changes: each entity's grants say who may create, read, update or delete
its records. An action that reaches no entity, such as a health check,
has no grants to take, so nobody may call it. The same happens when a
route reaches a create, read, update or delete that no grant names.
A route nobody may call answers every request with 403, so Jadpo refuses
it when you check, rather than when someone calls it.

Jadpo won't choose who may call the action for you. Root `policy { }`
lists such actions under `actions:`, each with who may run it, and that
list decides who can reach your app's data and behaviour.

## What it looks like

main.jadpo:

```jadpo check project=e-effect-ungranted expect=policy.effect_ungranted
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
```

```text output
error: `POST /health` runs `check_health`, which nobody may call
 --> main.jadpo:14:5
   |
14 |     POST / {
   |     ^^^^^^ reaches no entity
  reason: An action takes its permission from the entities it reaches, and `check_health` reaches none, so it is refused to everyone unless root `policy { }` lists who may call it under `actions:`.
  next: List `check_health: [...]` under `actions:` in `policy.jadpo` (human)
  rule: policy.effect_ungranted
  help: https://jadpo.dev/docs/errors/policy.effect_ungranted

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may call `check_health`.

A line under `actions:` can also let nobody in. A user field such as
`Note.owner_id`, or a role held in a record, is checked on the records
the action itself creates, changes or deletes. An action that changes
none of them, such as a summary of the caller's notes, passes nobody
through such a line, so its route could never succeed. Here,
`note_tidings` changes no note.

main.jadpo:

```jadpo check project=e-listed-line-unheld expect=policy.effect_ungranted
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

authentication {
    api_token: {}
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }
}

type Tidings = Output {
    ok: Bool
}

action note_tidings() -> Tidings {
    return Tidings {
        ok: true
    }
}

routes Bulletin {
    prefix: /tidings

    POST / {
        output: Tidings
        run: note_tidings()
    }
}

policy {
    actions: {
        note_tidings: [Note.owner_id]
    }
}
```

```text output
error: `POST /tidings` runs `note_tidings`, which nobody may call
 --> main.jadpo:46:5
   |
46 |     POST / {
   |     ^^^^^^ nobody passes its line
  reason: `note_tidings`'s line under `actions:` names `Note.owner_id`, the caller a `Note` the action changes points at, and `note_tidings` changes no such record itself, so no caller passes the line as the action starts and this route could never succeed.
  next: List `Access.authenticated` or a role held across the whole application for `note_tidings` under `actions:` (human)
  rule: policy.effect_ungranted
  help: https://jadpo.dev/docs/errors/policy.effect_ungranted

check failed: 1 error, 1 file
```

## How to fix it

The question is who should be able to call the action.

**If anyone may call it, signed in or not**, list it for `Access.public`.
In a project, this goes in `policy.jadpo`, beside `app.jadpo`, with the
action's domain in front of its name (`monitoring.check_health`):

```jadpo fragment
policy {
    actions: {
        check_health: [Access.public]
    }
}
```

```jadpo check project=e-effect-ungranted-public hidden
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

`POST /health` now answers anyone. Its route says
`authentication: RouteAuthentication.none`, so no caller has to sign in.

**If anyone signed in may call it**, list it for `Access.authenticated`:

```jadpo fragment
policy {
    actions: {
        note_tidings: [Access.authenticated]
    }
}
```

```jadpo check project=e-listed-line-authenticated hidden
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

authentication {
    api_token: {}
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }
}

type Tidings = Output {
    ok: Bool
}

action note_tidings() -> Tidings {
    return Tidings {
        ok: true
    }
}

routes Bulletin {
    prefix: /tidings

    POST / {
        output: Tidings
        run: note_tidings()
    }
}

policy {
    actions: {
        note_tidings: [Access.authenticated]
    }
}
```

Every signed-in caller may now call `POST /tidings`. Its reads still
follow `Note`'s grants, so each caller sees only the notes they own.

To let in only some people, list a role they hold across the whole
application, such as `StaffRole.operator`, a membership whose scope is
the application ([A membership role](../language/09-policy.md#a-membership-role)).
A role held in one record lets nobody in here, for the reason above.

## Related

- [Actions](../language/09-policy.md#actions)
- [A membership role](../language/09-policy.md#a-membership-role)
- [Lines that depend on a record](../language/09-policy.md#lines-that-depend-on-a-record)
- [Anyone signed in, or anyone](../language/09-policy.md#anyone-signed-in-or-anyone)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
