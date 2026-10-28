# `type.auth_principal_construction`

A caller only comes from signing in; tests, fixtures and seeds make one
from a record:

```jadpo check project=e-auth-principal-construction expect=type.auth_principal_construction
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

action act_as(member: Member) -> Principal.user {
    return Principal.user(member)
}
```

```text output
error: An action can't make a caller
 --> main.jadpo:19:12
   |
19 |     return Principal.user(member)
   |            ^^^^^^^^^^^^^^ made from a record
  reason: A caller only comes from signing in; tests, fixtures and seeds make one from a record with `Principal.user(...)`, and application code takes it as a parameter a route passes.
  next: Take a `Principal.user` parameter, which the route passes as `current_principal.user` (agent)
  rule: type.auth_principal_construction
  help: docs/language/08-identity.html#type-auth-principal-construction

check failed: 1 error, 1 file
```
