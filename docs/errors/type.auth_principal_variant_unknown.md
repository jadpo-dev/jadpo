# `type.auth_principal_variant_unknown`

```jadpo check project=e-auth-principal-variant-unknown expect=type.auth_principal_variant_unknown
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

type MemberView = Output {
    handle: Handle
}

type MemberNotFound = Failure {
    kind: FailureKind.not_found
}

query whoami(caller: Principal.user) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == caller.id
    missing: MemberNotFound
}

routes Members {
    prefix: /members

    GET /me {
        output: MemberView
        run: whoami(current_principal.service)
    }
}
```

```text output
error: `current_principal.service` isn't a kind of caller
 --> main.jadpo:43:39
   |
43 |         run: whoami(current_principal.service)
   |                                       ^^^^^^^ not declared
  reason: `principal { }` declares no `service:`, so no service ever signs in; it declares `user`.
  next: Write `current_principal.user` instead of `current_principal.service` (agent)
  rule: type.auth_principal_variant_unknown
  help: https://jadpo.dev/docs/errors/type.auth_principal_variant_unknown

check failed: 1 error, 1 file
```
