# `email.template_missing`

An email has no template, or a template has no email.

## Why Jadpo refuses it

An email's words live beside its type, in an `.html` template, a `.txt` one
or both, named after it. An email with neither has nothing to send, and a
template no email is named after is sent by nothing, so one of the two is a
mistake.

## What it looks like

`WelcomeEmail` has a type, and nothing beside it to send.

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

`domains/accounts/emails/welcome_email.jadpo`:

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
  help: https://jadpo.dev/docs/errors/email.template_missing

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The other way round, a template with no type beside it is refused too:

```jadpo check project=e-email-template-orphan file=app.jadpo hidden
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

`domains/accounts/emails/reset_notice.txt`:

```jadpo check project=e-email-template-orphan file=domains/accounts/emails/reset_notice.txt expect=email.template_missing
Your password was reset.
```

```text output
error: `reset_notice.txt` belongs to no email
 --> domains/accounts/emails/reset_notice.txt:1:1
  |
1 | Your password was reset.
  | ^^^^^^^^^^^^^^^^^^^^^^^^ no type beside it
  reason: An email's templates sit beside its `type X = EmailMessage { ... }`, named after it, so nothing sends `reset_notice.txt` until a `ResetNotice` is declared in `domains/accounts/emails/reset_notice.jadpo`.
  next: Declare `type ResetNotice = EmailMessage { ... }` beside it, or remove the file (agent)
  rule: email.template_missing
  help: https://jadpo.dev/docs/errors/email.template_missing

check failed: 1 error, 1 file
```

## How to fix it

**If the email is new**, write its template beside the type. In
`domains/accounts/emails/welcome_email.txt`:

```jadpo check project=e-email-template-missing-fix file=domains/accounts/emails/welcome_email.txt
Hello, {{ name }}.
```

```jadpo check project=e-email-template-missing-fix file=app.jadpo hidden
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

```jadpo check project=e-email-template-missing-fix file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-template-missing-fix file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

The email now has words to send.

**If the template belongs to an email that doesn't exist yet**, declare one
named after it, in `domains/accounts/emails/reset_notice.jadpo`:

```jadpo check project=e-email-template-orphan-fix file=domains/accounts/emails/reset_notice.jadpo
/// Sent once a password has been reset.
type ResetNotice = EmailMessage {}
```

```jadpo check project=e-email-template-orphan-fix file=app.jadpo hidden
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

```jadpo check project=e-email-template-orphan-fix file=domains/accounts/emails/reset_notice.txt hidden
Your password was reset.
```

```jadpo check project=e-email-template-orphan-fix file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    reset_notice: "Your password was reset"
}
```

The template now has an email to send it.

## Related

- [Templates](../language/16-built-in-services.md#templates)
- [Email](../language/16-built-in-services.md#email)
