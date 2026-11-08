# `semantic.auth_setting_invalid`

A sign-in setting has a value it can't take.

## Why Jadpo refuses it

Each sign-in setting says something exact: a cookie's name, an action that
decides a first sign-in, a duration inside its allowed range, a setting
read from configuration. A value it can't take would otherwise fail when
someone tries to sign in, or quietly leave sign-in weaker than you meant.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A way of signing in that isn't a block of settings is refused:

```jadpo check project=e-auth-setting-not-block expect=semantic.auth_setting_invalid
authentication {
    sign_in: {
        passkey: true
    }
}
```

```text output
error: `passkey:` isn't a block
 --> main.jadpo:3:9
  |
3 |         passkey: true
  |         ^^^^^^^ a block goes here
  reason: Passkeys are declared as a block of their settings, empty when the defaults suit.
  next: Write `passkey: {}` (agent)
  rule: semantic.auth_setting_invalid
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_invalid

check failed: 1 error, 1 file
```

So is a duration outside what a setting allows. A passkey's challenge
lasts from one to ten minutes:

```jadpo check project=e-auth-setting-duration expect=semantic.auth_setting_invalid
authentication {
    sign_in: {
        passkey: {
            expires_after: 1h
        }
    }
}
```

```text output
error: `1h` can't be how long a passkey's challenge lasts
 --> main.jadpo:4:28
  |
4 |             expires_after: 1h
  |                            ^^ not 1m to 10m
  reason: `expires_after:` is how long a device has to answer a passkey's challenge, from `1m` to `10m`: long enough for the browser's own prompt, short enough that a challenge seen by someone else is soon of no use.
  next: Write a duration from `1m` to `10m`, such as `5m` (agent)
  rule: semantic.auth_setting_invalid
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_invalid

check failed: 1 error, 1 file
```

An external token's issuer and audience come from configuration, never
from source:

```jadpo check project=e-auth-setting-literal file=app.jadpo expect=semantic.auth_setting_invalid
authentication {
    external_token: {
        issuer: "https://issuer.example"
        audience: config.token_audience
    }
}
```

```text output
error: `issuer:` doesn't read a setting
 --> app.jadpo:3:17
  |
3 |         issuer: "https://issuer.example"
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^ not `config.<name>`
  reason: An external token's issuer and audience come from deployment configuration, never source or the token itself.
  next: Declare `<name>: Url` in `config.jadpo`, and write `issuer: config.<name>` (agent)
  rule: semantic.auth_setting_invalid
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_invalid

check failed: 1 error, 2 files
```

```jadpo check project=e-auth-setting-literal file=config.jadpo hidden
config {
    token_audience: Text {
        default: "app"
    }
}
```

## How to fix it

**If the value is wrong**, write one the setting takes:

```jadpo fragment
    browser_session: {
        cookie: "my_session"
    }
```

```jadpo check project=e-auth-setting-invalid-fix hidden
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "my_session"
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

The session now travels in a cookie named `my_session`.

## Related

- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
- [Magic links](../language/08-identity.md#magic-links)
- [Passkeys](../language/08-identity.md#passkeys)
