# `email.health_problem`

The mail health score's checks, run on a template as written wherever its
own text answers them: an `.html` with no `.txt` beside it, a link written
out that isn't absolute HTTPS or goes through a shortener, HTML that
doesn't parse cleanly, images with no text, a subject worded in capitals,
or a template over Gmail's 102 KB clip. What a template writes from its
fields is scored once it is rendered, in the dev inbox. The score is an
early indication, so a `decline` line above the email's or partial's type
keeps a mail that breaks a rule on purpose:

```jadpo check project=e-email-health-problem file=app.jadpo hidden
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

```jadpo check project=e-email-health-problem file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-health-problem file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.

Start writing: https://notes.example/start
```

```jadpo check project=e-email-health-problem file=domains/accounts/emails/welcome_email.html
<p>Hello, {{ name }}.</p>
<p><a href="http://notes.example/start">Start writing</a></p>
```

```jadpo check project=e-email-health-problem file=domains/accounts/emails/welcome_email.jadpo expect=email.health_problem
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```text output
error: `WelcomeEmail` has a link that isn't absolute HTTPS
 --> domains/accounts/emails/welcome_email.jadpo:2:6
  |
2 | type WelcomeEmail = EmailMessage {
  |      ^^^^^^^^^^^^ link in welcome_email.html
  reason: `welcome_email.html` links to `http://notes.example/start`; filters distrust plain `http://`, and the reader's click goes unencrypted.
  next: Link to a full `https://` address, or write `decline email.health_problem: "<why>"` above it if this is deliberate (agent)
  rule: email.health_problem
  help: docs/language/16-built-in-services.html#email-health-problem

check failed: 1 error, 3 files
```
