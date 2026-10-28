# `type.auth_principal_input`

```jadpo check project=e-auth-principal-input expect=type.auth_principal_input
principal {
    user: {
        subject: Member.handle
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

type ActAs = Input {
    caller: Principal.user
}
```

```text output
error: `ActAs.caller` holds a caller
 --> main.jadpo:19:13
   |
19 |     caller: Principal.user
   |             ^^^^^^^^^^^^^^ a caller
  reason: A caller comes only from signing in, never from what a request sends, so a route takes it as `current_principal` and passes it on.
  next: Remove `caller`, and pass `current_principal` to what the route runs (agent)
  rule: type.auth_principal_input
  help: docs/language/08-identity.html#type-auth-principal-input

check failed: 1 error, 1 file
```
