# `type.auth_principal_output`

```jadpo check project=e-auth-principal-output expect=type.auth_principal_output
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

type Whoami = Output {
    caller: Principal.user
}
```

```text output
error: `Whoami.caller` holds a caller
 --> main.jadpo:19:13
   |
19 |     caller: Principal.user
   |             ^^^^^^^^^^^^^^ a caller
  reason: A caller value is who signed in, as the runtime holds it, and is never sent to a client; an output sends the fields a client needs.
  next: Type `caller` with the entity's id, such as `User.id`, or send the fields a client needs (agent)
  rule: type.auth_principal_output
  help: https://jadpo.dev/docs/errors/type.auth_principal_output

check failed: 1 error, 1 file
```
