# `semantic.multiple_authentication`

A project declares more than one `authentication { }`.

## Why Jadpo refuses it

The ways a caller signs in are listed once, so one place says how anyone
gets into the app. A second block could add a way in that someone reading
the first would never see.

## What it looks like

main.jadpo:

```jadpo check project=e-multiple-authentication expect=semantic.multiple_authentication
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

authentication {}

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
error: `authentication { }` is declared twice
 --> main.jadpo:13:1
   |
 7 | authentication {
   | -------------- first declared here
13 | authentication {}
   | ^^^^^^^^^^^^^^ declared again
  reason: The ways a caller signs in are listed once, in `authentication.jadpo`, so one place says how anyone gets in.
  next: Move what this `authentication` holds into the first, and remove it (agent)
  rule: semantic.multiple_authentication
  help: https://jadpo.dev/docs/errors/semantic.multiple_authentication

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both blocks declare a way in you want**, move them into one:

```jadpo fragment
authentication {
    browser_session: {
        cookie: "session"
    }
}
```

```jadpo check project=e-multiple-authentication-fix hidden
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

Every way of signing in is now listed in one `authentication { }`.

## Related

- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
