# `email.template_value_invalid`

A template writes out text, a number or an enum whose values have
wording, so what a reader sees is decided by a type that says how it is
shown; anything else is formatted first, and the error says how:

```jadpo check project=e-email-template-value file=app.jadpo hidden
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

```jadpo check project=e-email-template-value file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-template-value file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    joined_at: Instant
}
```

```jadpo check project=e-email-template-value file=domains/accounts/emails/welcome_email.html expect=email.template_value_invalid
<p>Welcome, {{ name }}. You joined on {{ joined_at }}.</p>
```

```text output
error: `{{ joined_at }}` can't be written out
 --> domains/accounts/emails/welcome_email.html:1:42
  |
1 | <p>Welcome, {{ name }}. You joined on {{ joined_at }}.</p>
  |                                          ^^^^^^^^^ this is `Instant`
  reason: A template writes out text (`Text`, `PresentationText`, or a type built on one), a number (`Int` or `Decimal`), or an enum whose values have wording, and `Html` as it is; `joined_at` is `Instant`.
  next: Format it with `text.format(...)` (area 19), which writes it in the email's locale (agent)
  rule: email.template_value_invalid
  help: https://jadpo.dev/docs/errors/email.template_value_invalid

check failed: 1 error, 3 files
```
