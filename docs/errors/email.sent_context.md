# `email.sent_context`

Code outside a test reads `email.sent`.

## Why Jadpo refuses it

`email.sent` lists the emails a test sent, which `jadpo test` catches in place
of sending them. Outside a test an email goes out once its action commits,
and nothing keeps it to read. Code that reads `email.sent` there would have
nothing to read and no truthful answer to give.

## What it looks like

An action tries to find out whether an email has gone out.

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

`domains/accounts/actions/stop_after_sending.jadpo`:

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
  help: https://jadpo.dev/docs/errors/email.sent_context

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you want to check what was sent**, read `email.sent` in a test, beside
the action that sends the email. In `domains/accounts/actions/sign_up.test.jadpo`:

```jadpo check project=e-email-sent-context-fix file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once a user has signed up.
type WelcomeEmail = EmailMessage {
    display_name: Text
}
```

```jadpo check project=e-email-sent-context-fix file=domains/accounts/emails/welcome_email.txt hidden
Hello {{ display_name }}, thanks for signing up.
```

```jadpo check project=e-email-sent-context-fix file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {display_name}"
}
```

```jadpo check project=e-email-sent-context-fix file=domains/accounts/entities/user.jadpo hidden
/// Someone with an account.
entity User {
    id: Uuid
    email: Email
    display_name: Text {
        min_length: 1
        max_length: 80
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-email-sent-context-fix file=domains/accounts/actions/sign_up.jadpo hidden
use accounts/entities { User }
use accounts/emails { WelcomeEmail }

/// Signs someone up, and welcomes them once it is done.
action sign_up(email: User.email, display_name: User.display_name) -> User {
    var user: User = User.create {
        set: {
            email: email
            display_name: display_name
        }
    }
    emit WelcomeEmail {
        to: user.email
        locale: Locale.en_gb
        display_name: user.display_name
    }
    return user
}
```

```jadpo check project=e-email-sent-context-fix file=app.jadpo hidden
application {
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-sent-context-fix file=config.jadpo hidden
config {
    mail_sender: Email {
        default: "hello@notes.example"
    }
    smtp_host: Hostname
}
```

```jadpo check project=e-email-sent-context-fix file=domains/accounts/actions/sign_up.test.jadpo migrate=initial
use accounts/entities { User }

test "signing up sends a welcome" {
    var user: User = sign_up("ada@notes.example", "Ada")
    assert user.display_name == "Ada"
    var sent: List<SentEmail> = email.sent
    assert list.any(sent, item -> item.to == "ada@notes.example" and item.subject == "Welcome, Ada")
}
```

The test sends nothing, and `email.sent` holds what the action would have
sent.

## Related

- [Sending](../language/16-built-in-services.md#sending)
- [Email](../language/16-built-in-services.md#email)
