# `policy.effect_ungranted`

A route that runs an action reaching no entity, which `actions:` in root `policy { }` does not list; a route that reaches a create, read, update or delete no grant names is refused the same way:

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

A line under `actions:` naming only a user field or a role held in a record lets nobody call an action that changes no such record, such as a summary of the caller's notes, so its route could never succeed:

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
