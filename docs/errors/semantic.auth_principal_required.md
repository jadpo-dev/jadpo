# `semantic.auth_principal_required`

```jadpo check project=e-auth-principal-required expect=semantic.auth_principal_required
authentication {
    browser_session: {
        cookie: "session"
    }
}
```

```text output
error: `browser_session:` signs in users, but no `user:` is declared
 --> main.jadpo:2:5
  |
2 |     browser_session: {
  |     ^^^^^^^^^^^^^^^ nobody to sign in
  reason: A browser session signs in a user, looked up through the principal's `user:` in `app.jadpo`, which names the entity and the unique field that identify one.
  next: Declare `principal { user: { subject: <Entity>.<field> } }` in `app.jadpo` (agent)
  rule: semantic.auth_principal_required
  help: docs/errors/semantic.auth_principal_required.md

check failed: 1 error, 1 file
```
