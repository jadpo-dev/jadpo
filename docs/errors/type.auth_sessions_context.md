# `type.auth_sessions_context`

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
  help: docs/errors/type.auth_sessions_context.md

check failed: 1 error, 1 file
```
