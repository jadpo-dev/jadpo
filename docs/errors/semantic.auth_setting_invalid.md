# `semantic.auth_setting_invalid`

```jadpo check project=e-auth-setting-invalid expect=semantic.auth_setting_invalid
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "my session"
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
error: `cookie:` isn't a cookie's name
 --> main.jadpo:9:17
  |
9 |         cookie: "my session"
  |                 ^^^^^^^^^^^^ not a cookie name
  reason: A browser session's cookie is named in text: 1 to 64 letters, digits, `_` and `-`, starting with a letter, such as `"session"`.
  next: Write the cookie's name in quotes, such as `cookie: "session"` (agent)
  rule: semantic.auth_setting_invalid
  help: docs/errors/semantic.auth_setting_invalid.md

check failed: 1 error, 1 file
```
