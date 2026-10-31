# `type.auth_public_principal`

A route that anyone may call, with `authentication: RouteAuthentication.none`, passes on the signed-in caller, though it never looks for one.

## Why Jadpo refuses it

`authentication: RouteAuthentication.none` lets anyone call a route, and
the route doesn't look for a session or token. So it never knows who is
calling, and `current_principal`, the signed-in caller, has nothing in
it. A query or action that takes a caller would be handed nobody.

Jadpo won't choose the fix for you. Either callers sign in, so the route
knows who they are, or the route stays open and stops depending on who
is calling, and that decides who can reach the data it reads.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.auth_public_principal

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether people must sign in to read a member.

## How to fix it

The question is whether the route is about who is calling.

**If it is, so callers must sign in**, remove the
`authentication: RouteAuthentication.none` line. `whoami` reads a
`Member`, so `Member` also needs a grant that lets each person read their
own record:

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

```jadpo check project=e-auth-public-principal-signed-in hidden
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

`GET /members/me` now needs a session cookie, and answers each caller
with their own handle. A request without a valid session gets 401.

**If anyone may read a member, signed in or not**, keep the route open and
name the member in the path instead of taking the caller. Its query finds
the member by `id`, and `Member` grants `read` to `Access.public`:

```jadpo fragment
query find_member(member_id: Member.id) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == member_id
    missing: MemberNotFound
}
```

```jadpo fragment
    GET /{member_id: Member.id} {
        authentication: RouteAuthentication.none
        output: MemberView
        run: find_member(member_id)
    }
```

```jadpo fragment
    policy: {
        read: [Access.public]
    }
```

```jadpo check project=e-auth-public-principal-open hidden
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
        read: [Access.public]
    }
}

type MemberView = Output {
    handle: Handle
}

type MemberNotFound = Failure {
    kind: FailureKind.not_found
}

query find_member(member_id: Member.id) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == member_id
    missing: MemberNotFound
}

routes Members {
    prefix: /members

    GET /{member_id: Member.id} {
        authentication: RouteAuthentication.none
        output: MemberView
        run: find_member(member_id)
    }
}
```

`GET /members/{member_id}` answers anyone with that member's handle, and
never knows who is asking.

## Related

- [Open to anyone: `RouteAuthentication.none`](../language/08-identity.md#open-to-anyone-routeauthenticationnone)
- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
- [Anyone signed in, or anyone](../language/09-policy.md#anyone-signed-in-or-anyone)
- [A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
