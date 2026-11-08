# `type.auth_sessions_context`

Code calls a session, API token or service key operation, such as `sessions.end`, somewhere other than an action, or without the way of signing in it needs.

## Why Jadpo refuses it

`sessions.start`, `sessions.end`, `sessions.issue_token` and the
operations on a service's keys change who is signed in. Each keeps what
it changes in the action's transaction, and the route that ran the
action sets or clears the cookies on its response. A function, a query or
a test has neither, so only an action calls them. The token operations
also need `api_token:` in `authentication { }`, and a service's key
operations `service_key:`.

## What it looks like

main.jadpo:

```jadpo check project=e-auth-sessions-context expect=type.auth_sessions_context
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

test "signing out ends the session" {
    sessions.end()
}
```

```text output
error: A test can't call `sessions.end`
 --> main.jadpo:25:5
   |
25 |     sessions.end()
   |     ^^^^^^^^^^^^ outside an action
  reason: `sessions.end` ends the session of the request a route answers, so only an action calls it.
  next: Call `sessions.end` in the action a sign-in or sign-out route runs (agent)
  rule: type.auth_sessions_context
  help: https://jadpo.dev/docs/errors/type.auth_sessions_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If people sign out**, call `sessions.end` in an action, and run the
action from a route:

```jadpo fragment
action sign_out() -> Unit {
    sessions.end()
}

routes Account {
    prefix: /account

    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }
}
```

```jadpo check project=e-auth-sessions-context-fix hidden
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

/// Ends the session the request came with.
action sign_out() -> Unit {
    sessions.end()
}

routes Account {
    prefix: /account

    /// Ends the session and clears its cookies.
    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }
}
```

`POST /account/sign-out` ends the session the request came with, and its
response clears the cookies. A test signs out through the route, or calls
`sign_out()`.

## Related

- [Starting and ending a session](../language/08-identity.md#starting-and-ending-a-session)
- [API tokens](../language/08-identity.md#api-tokens)
- [Service credentials](../language/08-identity.md#service-credentials)
