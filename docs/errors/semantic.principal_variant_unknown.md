# `semantic.principal_variant_unknown`

A role is a field of the user's entity, which policy reads (area 9), never
a kind of caller:

```jadpo check project=e-principal-variant-unknown expect=semantic.principal_variant_unknown
principal {
    user: {
        subject: Member.handle
    }
    admin: {
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
```

```text output
error: `admin` isn't a kind of caller
 --> main.jadpo:5:5
  |
5 |     admin: {
  |     ^^^^^ not `user` or `service`
  reason: A principal declares `user:`, a person, and `service:`, another system; a role or a tenant is a field of the user's entity, which policy reads (area 9).
  next: Remove `admin` (agent)
  rule: semantic.principal_variant_unknown
  help: docs/language/08-identity.html#semantic-principal-variant-unknown

check failed: 1 error, 1 file
```
