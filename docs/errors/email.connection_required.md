# `email.connection_required`

An email is sent through the connection `application { email: }` names,
once the action that emits it commits, so emitting one needs it:

```jadpo check project=e-email-connection-required file=app.jadpo hidden
application {
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-connection-required file=domains/accounts/types/display_name.jadpo hidden
type DisplayName = Text {
    max_length: 80
}
```

```jadpo check project=e-email-connection-required file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-connection-required file=domains/accounts/emails/welcome_email.jadpo hidden
use accounts/types { DisplayName }

/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: DisplayName
}
```

```jadpo check project=e-email-connection-required file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.
```

```jadpo check project=e-email-connection-required file=domains/accounts/actions/welcome.jadpo expect=email.connection_required
use accounts/types { DisplayName }
use accounts/emails { WelcomeEmail }

/// Welcomes someone who has just signed up.
action welcome(address: Email, name: DisplayName) -> Unit {
    emit WelcomeEmail {
        to: address
        locale: Locale.en_gb
        name: name
    }
}
```

```text output
error: `WelcomeEmail` is emitted, but nothing sends email
 --> domains/accounts/actions/welcome.jadpo:6:5
  |
6 |     emit WelcomeEmail {
  |     ^^^^^^^^^^^^^^^^^^^ no email connection
  reason: An email is sent once the action that emits it commits, through the connection `application { email: }` names, from the address its `from:` gives; `app.jadpo` names none.
  next: Declare a connection with `driver: Driver.smtp` in `connections:`, and add `email: { connection: <its name>  from: config.mail_sender }` to `application { }` in `app.jadpo` (agent)
  rule: email.connection_required
  help: docs/errors/email.connection_required.md

check failed: 1 error, 5 files
```
