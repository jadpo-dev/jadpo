# `semantic.auth_principal_required`

A sign-in method signs in a kind of caller the principal doesn't declare.

## Why Jadpo refuses it

A sign-in method finds its caller through the principal: `user:` for a
person, `service:` for another system, each naming the entity and the
unique field that identify one. With neither declared for the method's
kind of caller, nobody could sign in.

## What it looks like

main.jadpo:

```jadpo check project=e-auth-principal-required expect=semantic.auth_principal_required
authentication {
    browser_session: {
        cookie: "session"
    }
}
```

```text output
error: `browser_session:` signs in users, but no `user:` is declared
 --> main.jadpo:2:5
  |
2 |     browser_session: {
  |     ^^^^^^^^^^^^^^^ nobody to sign in
  reason: A browser session signs in a user, looked up through the principal's `user:` in `app.jadpo`, which names the entity and the unique field that identify one.
  next: Declare `principal { user: { subject: <Entity>.<field> } }` in `app.jadpo` (agent)
  rule: semantic.auth_principal_required
  help: https://jadpo.dev/docs/errors/semantic.auth_principal_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `service_key:` needs a `service:` in the same way:

```jadpo check project=e-auth-service-required expect=semantic.auth_principal_required
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    service_key: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `service_key:` signs in services, but no `service:` is declared
 --> main.jadpo:8:5
  |
8 |     service_key: {}
  |     ^^^^^^^^^^^ nobody to sign in
  reason: A service key signs in a service, looked up through the principal's `service:` in `app.jadpo`, which names the entity and the unique field that identify one.
  next: Add `service: { subject: <Entity>.<field> }` to `principal { }` (agent)
  rule: semantic.auth_principal_required
  help: https://jadpo.dev/docs/errors/semantic.auth_principal_required

check failed: 1 error, 1 file
```

## How to fix it

**If the app should sign people in**, declare who they are:

```jadpo check project=e-auth-principal-required-fix
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

A browser session now signs in a `Member`, found by its `handle`.

## Related

- [The principal](../language/08-identity.md#the-principal)
- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
