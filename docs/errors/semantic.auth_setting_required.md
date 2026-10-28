# `semantic.auth_setting_required`

```jadpo check project=e-auth-setting-required expect=semantic.auth_setting_required
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {}
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
error: `browser_session:` doesn't name its cookie
 --> main.jadpo:8:5
  |
8 |     browser_session: {}
  |     ^^^^^^^^^^^^^^^ no `cookie:`
  reason: A browser session travels in a cookie the application names, so two applications on one domain never share one.
  next: Add `cookie: "<name>"`, such as `cookie: "session"` (agent)
  rule: semantic.auth_setting_required
  help: docs/language/08-identity.html#semantic-auth-setting-required

check failed: 1 error, 1 file
```
