# `type.auth_principal_input`

An `Input` has a field that holds a caller, a `Principal.user` or a `Principal.service`, as if a request could send who is calling.

## Why Jadpo refuses it

A caller is who Jadpo checked when they signed in. A field of an `Input`
is whatever the request sends, so a caller read from it would let anyone
claim to be anyone. A route reads the real caller as
`current_principal`, and passes it to what it runs as an ordinary
argument. This holds however deeply a field holds the caller: in a list,
a map, an option, or another object's field.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.auth_principal_input

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If what the route runs needs who is calling**, remove the field, give
the query or action a `Principal.user` parameter, and pass
`current_principal.user` from the route:

```jadpo fragment
query whoami(caller: Principal.user) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == caller.id
    missing: MemberNotFound
}
```

```jadpo fragment
    GET /me {
        output: MemberView
        run: whoami(current_principal.user)
    }
```

```jadpo check project=e-auth-principal-input-fix hidden
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

The caller is now whoever signed in, read from their session, and a
request can't name anyone else.

## Related

- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
