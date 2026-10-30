# `type.principal_active_invalid`

```jadpo check project=e-principal-active-invalid expect=type.principal_active_invalid
principal {
    user: {
        subject: Member.handle
        active: Member.suspended
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
    suspended: Bool
}
```

```text output
error: `Member.suspended` isn't a condition `active:` can use
 --> main.jadpo:4:17
  |
4 |         active: Member.suspended
  |                 ^^^^^^^^^^^^^^^^ not a comparison
  reason: `active:` is a condition on `Member`'s own fields, such as `Member.<field> == <value>`; a field alone says nothing to compare it with.
  next: Compare a field of `Member` with a value (agent)
  rule: type.principal_active_invalid
  help: https://jadpo.dev/docs/errors/type.principal_active_invalid

check failed: 1 error, 1 file
```
