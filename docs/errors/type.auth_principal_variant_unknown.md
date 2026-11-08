# `type.auth_principal_variant_unknown`

A route reads `current_principal.user` or `current_principal.service` for a kind of caller that `principal { }` doesn't declare.

## Why Jadpo refuses it

`principal { }` says which kinds of caller sign in to the app: `user:`
for people, `service:` for other systems. A route that selects a kind it
doesn't declare asks for a caller who can never sign in. The mistake is
usually the wrong word, or a `service:` not yet declared.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the route is for people**, select `current_principal.user`. `whoami`
reads a `Member`, so `Member` also grants each person `read` of their own
record:

```jadpo fragment
    GET /me {
        output: MemberView
        run: whoami(current_principal.user)
    }
```

```jadpo fragment
    policy: {
        read: [id]
    }
```

```jadpo check project=e-auth-principal-variant-unknown-fix hidden
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

    policy: {
        read: [id]
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
        run: whoami(current_principal.user)
    }
}
```

`GET /members/me` answers a signed-in member with their own handle. A
service calling it is refused with 403.

## Related

- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
- [The principal](../language/08-identity.md#the-principal)
- [Services](../language/08-identity.md#services)
