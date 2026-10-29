# `email.template_not_allowed`

A template reads its email's fields, translates and formats; whatever
else an email says is worked out where it is emitted, and given to it as
a field, so its words are reviewed in one place:

```jadpo check project=e-email-template-not-allowed file=app.jadpo hidden
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

```jadpo check project=e-email-template-not-allowed file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-template-not-allowed file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```jadpo check project=e-email-template-not-allowed file=domains/accounts/emails/welcome_email.html expect=email.template_not_allowed
<h1>WELCOME, {{ text.upper(name) }}</h1>
```

```text output
error: A template only reads its fields, translates and formats
 --> domains/accounts/emails/welcome_email.html:1:17
  |
1 | <h1>WELCOME, {{ text.upper(name) }}</h1>
  |                 ^^^^^^^^^^^^^^^^ a call
  reason: Templates are strict: they read their email's fields, call translations and `text.format`, compare values in `{{ if }}` and walk lists with `{{ for }}`, so what an email says is decided where it is emitted, and a reviewer reads its words in one place.
  next: Call only `translations.<domain>.<message>(...)` and `text.format(...)`; work anything else out where the email is emitted, and give it to the email as a field (agent)
  rule: email.template_not_allowed
  help: docs/errors/email.template_not_allowed.md

check failed: 1 error, 3 files
```
