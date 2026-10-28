# `email.settings_invalid`

`email: { connection: from: }` names the connection every email goes
through, one that provides `email`, and the address they are from;
`display_name:` is the application's name, written out:

```jadpo check project=e-email-settings-invalid file=app.jadpo expect=email.settings_invalid
application {
    email: {
        connection: main
        from: "hello@notes.example"
    }
}
```

```text output
error: `connection: main` sends no email
 --> app.jadpo:3:21
  |
3 |         connection: main
  |                     ^^^^ not an email connection
  reason: `email: { connection: }` names a connection that provides `email`: one with `driver: Driver.smtp`, or to a mail package that maps `send` in its `provides/email.jadpo`; `main` is a connection that sends no email, and `app.jadpo` declares none with `driver: Driver.smtp`.
  next: Declare a connection with `driver: Driver.smtp` in `connections:`, and name it here (agent)
  rule: email.settings_invalid
  help: docs/language/16-built-in-services.html#email-settings-invalid

check failed: 1 error, 1 file
```
