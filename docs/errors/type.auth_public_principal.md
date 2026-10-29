# `type.auth_public_principal`

```jadpo check project=e-auth-public-principal expect=type.auth_public_principal
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
        authentication: RouteAuthentication.none
        output: MemberView
        run: whoami(current_principal.user)
    }
}
```

```text output
error: A route anyone may call passes `current_principal`
 --> main.jadpo:44:21
   |
44 |         run: whoami(current_principal.user)
   |                     ^^^^^^^^^^^^^^^^^ no one is signed in here
  reason: `authentication: RouteAuthentication.none` reads no credential, so the route has no caller to pass on, and nothing it runs may use one.
  next: Remove `authentication: RouteAuthentication.none` so callers sign in, or stop passing `current_principal` (human)
  rule: type.auth_public_principal
  help: docs/errors/type.auth_public_principal.md

check failed: 1 error, 1 file
```
