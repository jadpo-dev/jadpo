# `semantic.auth_setting_unknown`

The language owns the cookie's flags, its lifetime and the CSRF check:

```jadpo check project=e-auth-setting-unknown expect=semantic.auth_setting_unknown
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
        same_site: "lax"
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
error: `same_site` isn't a `browser_session` setting
 --> main.jadpo:10:9
   |
10 |         same_site: "lax"
   |         ^^^^^^^^^ unknown setting
  reason: A browser session takes `cookie:`, its cookie's name, and `origin:`; the language sets the cookie's flags, its lifetime and the CSRF check.
  next: Remove `same_site:` (agent)
  rule: semantic.auth_setting_unknown
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_unknown

check failed: 1 error, 1 file
```
