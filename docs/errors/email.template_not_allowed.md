# `email.template_not_allowed`

An email template calls a function, does arithmetic or builds a value, when it may only read its fields, translate and format.

## Why Jadpo refuses it

A template reads its email's fields, calls translations and
`text.format`, compares values in `{{ if }}` and walks lists with
`{{ for }}`. Anything else an email says is worked out in the code that
emits it, and given to the email as a field. That keeps the email's
words in one place, where a reviewer reads them, rather than spread
between the code and its templates.

## What it looks like

The welcome email's heading changes its field's value with a call.

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

domains/accounts/emails/welcome_email.html:

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
  help: https://jadpo.dev/docs/errors/email.template_not_allowed

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field can be written as it is**, write it out without the call.
In `domains/accounts/emails/welcome_email.html`:

```jadpo fragment
<h1>Welcome, {{ name }}</h1>
```

```jadpo check project=e-email-template-not-allowed-fix-1 file=domains/accounts/emails/welcome_email.html hidden
<h1>Welcome, {{ name }}</h1>
```

```jadpo check project=e-email-template-not-allowed-fix-1 file=domains/accounts/emails/welcome_email.txt hidden
Welcome, {{ name }}
```

```jadpo check project=e-email-template-not-allowed-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-email-template-not-allowed-fix-1 file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-template-not-allowed-fix-1 file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

The heading writes the name as the email was given it.

**If the email needs a value worked out**, give the email a field for
it, and work the value out in the code that emits the email. In
`domains/accounts/emails/welcome_email.jadpo`:

```jadpo fragment
    heading_name: Text
```

```jadpo check project=e-email-template-not-allowed-fix-2 file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    heading_name: Text
}
```

Then the template writes the field out. In
`domains/accounts/emails/welcome_email.html`:

```jadpo fragment
<h1>WELCOME, {{ heading_name }}</h1>
```

```jadpo check project=e-email-template-not-allowed-fix-2 file=domains/accounts/emails/welcome_email.html hidden
<h1>WELCOME, {{ heading_name }}</h1>
```

```jadpo check project=e-email-template-not-allowed-fix-2 file=domains/accounts/emails/welcome_email.txt hidden
WELCOME, {{ heading_name }}
```

```jadpo check project=e-email-template-not-allowed-fix-2 file=app.jadpo hidden
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

```jadpo check project=e-email-template-not-allowed-fix-2 file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

Every place that emits `WelcomeEmail` now gives `heading_name`, so the
value is worked out in code a reviewer reads, and the template only
writes it.

## Related

- [What a template may not do](../language/16-built-in-services.md#what-a-template-may-not-do)
- [What a template may do](../language/16-built-in-services.md#what-a-template-may-do)
- [Templates](../language/16-built-in-services.md#templates)
