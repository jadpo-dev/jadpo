# `type.auth_principal_failure_context`

```jadpo check project=e-auth-principal-failure-context expect=type.auth_principal_failure_context
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

type MemoLocked = Failure {
    kind: FailureKind.conflict
    client: {
        caller: Principal.user
    }
}
```

```text output
error: `MemoLocked.caller` holds a caller
 --> main.jadpo:21:17
   |
21 |         caller: Principal.user
   |                 ^^^^^^^^^^^^^^ a caller
  reason: A failure is logged and its client fields reach the client, while a caller value holds who signed in, so a failure carries the caller's `id` instead.
  next: Type `caller` with the entity's id, such as `User.id` (agent)
  rule: type.auth_principal_failure_context
  help: docs/language/08-identity.html#type-auth-principal-failure-context

check failed: 1 error, 1 file
```
