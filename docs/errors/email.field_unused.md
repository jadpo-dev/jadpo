# `email.field_unused`

An email's fields are what its templates and its subject read, so a
field none of them reads is sent for nothing, or is a value a template
meant to show. A judgement call: a `decline` line above the field keeps
one deliberately:

```jadpo check project=e-email-field-unused file=app.jadpo hidden
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

```jadpo check project=e-email-field-unused file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-field-unused file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.
```

```jadpo check project=e-email-field-unused file=domains/accounts/emails/welcome_email.jadpo expect=email.field_unused
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    referral_code: Text
}
```

```text output
error: `WelcomeEmail.referral_code` is written nowhere
 --> domains/accounts/emails/welcome_email.jadpo:4:5
  |
4 |     referral_code: Text
  |     ^^^^^^^^^^^^^ no template writes it
  reason: An email's fields are what its templates and its subject read, so a field none of them reads is sent for nothing, or is a value a template was meant to show; no template of `WelcomeEmail` reads `referral_code`, and no wording of its subject writes `{referral_code}`.
  next: Read `referral_code` in a template or the subject, or remove it from `WelcomeEmail`, or write `decline email.field_unused: "<why>"` above it if this is deliberate (agent)
  rule: email.field_unused
  help: https://jadpo.dev/docs/errors/email.field_unused

check failed: 1 error, 3 files
```
