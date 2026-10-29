# `email.sent_context`

`email.sent` is what a test sent, caught in place of sending it, so only
a test reads it; outside a test an email goes out once its action
commits, and nothing keeps it to read:

```jadpo check project=e-email-sent-context file=app.jadpo hidden
application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: "smtp.notes.example"
        }
    }
    email: {
        connection: mail
        from: "hello@notes.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-sent-context file=domains/accounts/actions/stop_after_sending.jadpo expect=email.sent_context
/// Stops early once an email has gone out.
action stop_after_sending() -> Unit {
    if (list.count(email.sent) > 0) {
        return
    }
}
```

```text output
error: `email.sent` is read outside a test
 --> domains/accounts/actions/stop_after_sending.jadpo:3:20
  |
3 |     if (list.count(email.sent) > 0) {
  |                    ^^^^^^^^^^ not in a test
  reason: `email.sent` lists the emails a test sent, which `jadpo test` catches in place of sending them; outside a test an email goes out once its action commits, and nothing keeps it to read.
  next: Read `email.sent` in a test, after the action that sends the email (agent)
  rule: email.sent_context
  help: docs/errors/email.sent_context.md

check failed: 1 error, 2 files
```
