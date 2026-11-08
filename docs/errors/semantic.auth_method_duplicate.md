# `semantic.auth_method_duplicate`

A way of signing in is declared twice in `authentication { }` or `sign_in:`.

## Why Jadpo refuses it

Each way to sign in is one block, written once, so its settings are in one
place. Jadpo won't merge two, because it can't tell which `cookie:` you
meant.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.auth_method_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A way of checking a person inside `sign_in:` is refused the same way:

```jadpo check project=e-auth-sign-in-duplicate file=config.jadpo hidden
config {
    browser_origin: Url {
        default: "http://localhost:3000"
    }
}
```

```jadpo check project=e-auth-sign-in-duplicate file=main.jadpo expect=semantic.auth_method_duplicate
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
        providers: []
        providers: []
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
error: `providers:` is declared twice
 --> main.jadpo:14:9
   |
13 |         providers: []
   |         --------- first declared here
14 |         providers: []
   |         ^^^^^^^^^ declared again
  reason: Each way to check a person is one block, written once, so its settings are in one place.
  next: Move the settings into the first `providers:`, and remove this one (agent)
  rule: semantic.auth_method_duplicate
  help: https://jadpo.dev/docs/errors/semantic.auth_method_duplicate

check failed: 1 error, 2 files
```

## How to fix it

**If you want one sign-in method**, keep one block with its settings:

```jadpo fragment
authentication {
    browser_session: {
        cookie: "session"
    }
}
```

```jadpo check project=e-auth-method-duplicate-fix hidden
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

There is now one browser session, in the one cookie it names.

## Related

- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
