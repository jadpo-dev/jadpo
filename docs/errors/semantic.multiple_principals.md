# `semantic.multiple_principals`

```jadpo check project=e-multiple-principals expect=semantic.multiple_principals
principal {
    user: {
        subject: Member.handle
    }
}

principal {
    user: {
        subject: Member.handle
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
error: `principal { }` is declared twice
 --> main.jadpo:7:1
  |
1 | principal {
  | --------- first declared here
7 | principal {
  | ^^^^^^^^^ declared again
  reason: The caller is declared once, in `app.jadpo`: one `user:` and at most one `service:`, so every route reads the same caller.
  next: Move what this `principal` holds into the first, and remove it (agent)
  rule: semantic.multiple_principals
  help: https://jadpo.dev/docs/errors/semantic.multiple_principals

check failed: 1 error, 1 file
```
