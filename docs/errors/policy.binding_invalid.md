# `policy.binding_invalid`

A field names who may do something only when it points at the caller, a reference to the principal's user entity; the same rule covers a membership's `scope:`, `member:` and `role:`, `tenant:`, and the lines under `actions:`:

```jadpo check project=e-binding-invalid expect=policy.binding_invalid
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
    owner_id: Uuid

    policy: {
        read: [owner_id]
    }
}
```

```text output
error: `owner_id` doesn't point at a caller
 --> main.jadpo:23:16
   |
23 |         read: [owner_id]
   |                ^^^^^^^^ not a user field
  reason: A field names who may do this when it points at the caller: a reference to `User`, the principal's user entity (area 8), typed `User.id`. `Note.owner_id` isn't.
  next: Name a reference to the user, a membership role, or `Access` (agent)
  rule: policy.binding_invalid
  help: https://jadpo.dev/docs/errors/policy.binding_invalid

check failed: 1 error, 1 file
```

Under `actions:`, an action in a project with domains is named through its
domain, as a call from another domain names it. A bare name that is one
domain's action gets the edit that writes its domain, and the `use` line
that brought it in isn't also reported:

```jadpo check project=e-binding-bare-action file=app.jadpo hidden
application {}
```

```jadpo check project=e-binding-bare-action file=domains/lending/actions/borrow.jadpo hidden
action borrow() -> Unit {}
```

```jadpo check project=e-binding-bare-action file=policy.jadpo expect=policy.binding_invalid
use lending/actions { borrow }

policy {
    actions: {
        borrow: [Access.public]
    }
}
```

```text output
error: `borrow` belongs to domain `lending`
 --> policy.jadpo:5:9
  |
5 |         borrow: [Access.public]
  |         ^^^^^^ needs its domain
  reason: Root `policy { }` names each action through its domain, `domain.action`, as a call from outside the domain does; a bare name is for a single-file project.
  next: Write `lending.borrow`; `jadpo fmt` then imports `lending` in place of `borrow` (agent)
  rule: policy.binding_invalid
  help: https://jadpo.dev/docs/errors/policy.binding_invalid

check failed: 1 error, 3 files
```
