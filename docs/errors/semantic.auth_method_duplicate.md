# `semantic.auth_method_duplicate`

```jadpo check project=e-auth-method-duplicate expect=semantic.auth_method_duplicate
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
    browser_session: {
        cookie: "admin_session"
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
error: `browser_session:` is declared twice
 --> main.jadpo:11:5
   |
 8 |     browser_session: {
   |     --------------- first declared here
11 |     browser_session: {
   |     ^^^^^^^^^^^^^^^ declared again
  reason: Each way to sign in is one block, written once, so its settings are in one place.
  next: Move the settings into the first `browser_session:`, and remove this one (agent)
  rule: semantic.auth_method_duplicate
  help: docs/errors/semantic.auth_method_duplicate.md

check failed: 1 error, 1 file
```
