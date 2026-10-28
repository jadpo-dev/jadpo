# `email.template_missing`

An email's words live beside its type, in an `.html` template, a `.txt`
one or both, named after it; an email with neither has nothing to send,
and a template no email is named after is sent by nothing:

```jadpo check project=e-email-template-missing file=app.jadpo hidden
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

```jadpo check project=e-email-template-missing file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-template-missing file=domains/accounts/emails/welcome_email.jadpo expect=email.template_missing
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```text output
error: `WelcomeEmail` has no template
 --> domains/accounts/emails/welcome_email.jadpo:2:6
  |
2 | type WelcomeEmail = EmailMessage {
  |      ^^^^^^^^^^^^ nothing to send
  reason: An email's words live beside its type: `welcome_email.html`, its HTML, and `welcome_email.txt`, its plain text, each reading its fields inside `{{ }}`. It needs at least one.
  next: Create `domains/accounts/emails/welcome_email.html` and `domains/accounts/emails/welcome_email.txt` beside `welcome_email.jadpo` (agent)
  rule: email.template_missing
  help: docs/language/16-built-in-services.html#email-template-missing

check failed: 1 error, 3 files
```
