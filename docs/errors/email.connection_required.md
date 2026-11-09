# `email.connection_required`

An action emits an email, or a feature sends one, but the application names no email connection.

## Why Jadpo refuses it

An email goes out through the connection `application { email: }` names, once
the action that emits it commits. If there is no such connection, the email
has no way to leave, and the person waiting for it would never know why. The
same goes for magic links and for a data request's address check, which
send their own emails.

## What it looks like

`welcome` emits an email, and `app.jadpo` has no `email:` setting.

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

`domains/accounts/actions/welcome.jadpo`:

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
  help: https://jadpo.dev/docs/errors/email.connection_required

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the application should send email**, declare a connection and name it
in `app.jadpo`:

```jadpo check project=e-email-connection-required-fix file=app.jadpo
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

```jadpo check project=e-email-connection-required-fix file=domains/accounts/types/display_name.jadpo hidden
type DisplayName = Text {
    max_length: 80
}
```

```jadpo check project=e-email-connection-required-fix file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-connection-required-fix file=domains/accounts/emails/welcome_email.jadpo hidden
use accounts/types { DisplayName }

/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: DisplayName
}
```

```jadpo check project=e-email-connection-required-fix file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.
```

```jadpo check project=e-email-connection-required-fix file=domains/accounts/actions/welcome.jadpo hidden
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

`WelcomeEmail` now goes out through `mail` once `welcome` commits.

## Related

- [Email](../language/16-built-in-services.md#email)
- [Sending](../language/16-built-in-services.md#sending)
