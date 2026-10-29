# `semantic.multiple_authentication`

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
  help: docs/errors/semantic.multiple_authentication.md

check failed: 1 error, 1 file
```
