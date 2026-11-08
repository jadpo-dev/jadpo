# `semantic.auth_setting_required`

A way of signing in is missing a setting it needs, or code relies on one the app hasn't set up.

## Why Jadpo refuses it

Sign-in methods depend on each other: a magic link needs a browser session
to start, a passkey needs a way back in when a device is lost, an external
token needs an issuer to check against. Without the setting, nobody could
sign in that way, so Jadpo says so when you check.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A passkey needs a magic link beside it, so someone who loses every device
can still get back in:

```jadpo check project=e-auth-passkey-recovery file=config.jadpo hidden
config {
    browser_origin: Url {
        default: "http://localhost:3000"
    }
}
```

```jadpo check project=e-auth-passkey-recovery file=main.jadpo expect=semantic.auth_setting_required
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
        origin: config.browser_origin
    }
    sign_in: {
        passkey: {}
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
error: `passkey:` needs `magic_link:` beside it
 --> main.jadpo:13:9
   |
13 |         passkey: {}
   |         ^^^^^^^ no way back in
  reason: A person who loses every device holding a passkey signs in with a magic link or its code and adds a new one; with no `magic_link:` in the same `sign_in:`, nothing gets them back in.
  next: Add `magic_link: { link: "<path>" }` to `sign_in:`, beside `passkey:` (agent)
  rule: semantic.auth_setting_required
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_required

check failed: 1 error, 2 files
```

An external token has to say whose tokens it accepts:

```jadpo check project=e-auth-external-issuer file=config.jadpo hidden
config {
    token_audience: Text {
        default: "app"
    }
}
```

```jadpo check project=e-auth-external-issuer file=main.jadpo expect=semantic.auth_setting_required
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    external_token: {
        audience: config.token_audience
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
error: `external_token:` has no `issuer:`
 --> main.jadpo:8:5
  |
8 |     external_token: {
  |     ^^^^^^^^^^^^^^ no `issuer:`
  reason: An external token is checked against its configured issuer and audience before its subject identifies an active user.
  next: Add `issuer: config.<name>`, reading a `Url` setting (agent)
  rule: semantic.auth_setting_required
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_required

check failed: 1 error, 2 files
```

## How to fix it

**If the block is missing a setting**, add it:

```jadpo fragment
    browser_session: {
        cookie: "session"
    }
```

```jadpo check project=e-auth-setting-required-fix hidden
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

The browser session now travels in the cookie it names.

## Related

- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
- [Recovery](../language/08-identity.md#recovery)
