# `type.auth_credential_stored`

An API token or a service key is shown once and kept only as its verifier,
so no entity can keep one:

```jadpo check project=e-auth-credential-stored expect=type.auth_credential_stored
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    last_token: ApiToken
}
```

```text output
error: `Member.last_token` keeps an API token
 --> main.jadpo:20:17
   |
20 |     last_token: ApiToken
   |                 ^^^^^^^^ a secret, stored
  reason: An API token is shown once, in the output of the action that issues it, and the runtime keeps only its verifier, so a copy of the database signs no one in.
  next: Remove `last_token`, and give the token in the output of the action that issues it (agent)
  rule: type.auth_credential_stored
  help: https://jadpo.dev/docs/errors/type.auth_credential_stored

check failed: 1 error, 1 file
```
