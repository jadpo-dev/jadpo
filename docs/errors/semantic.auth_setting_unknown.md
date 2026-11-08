# `semantic.auth_setting_unknown`

A sign-in method has a setting it doesn't take.

## Why Jadpo refuses it

Each sign-in method takes only the settings the language gives it. The
language itself sets the rest, such as a cookie's flags, its lifetime and
the CSRF check, so a setting that looks as if it changes them would do
nothing, and a person reading the code would think it had.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the setting is one the language decides**, remove it:

```jadpo fragment
    browser_session: {
        cookie: "session"
    }
```

```jadpo check project=e-auth-setting-unknown-fix hidden
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
```

The cookie now has the flags the language gives every session cookie.

## Related

- [`browser_session`](../language/08-identity.md#browser_session)
