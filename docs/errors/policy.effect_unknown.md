# `policy.effect_unknown`

A policy grants `create`, `read`, `update` and `delete`; an action needing other people is listed under `actions:`:

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
