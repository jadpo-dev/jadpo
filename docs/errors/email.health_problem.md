# `email.health_problem`

An email's template fails a check of the mail health score.

## Why Jadpo refuses it

Mail filters judge a message by things in its template: a missing plain-text
part, a link that isn't HTTPS or goes through a shortener, broken HTML,
images with no text, a subject in capitals, or more than Gmail shows before
it clips a mail at 102 KB. Jadpo runs those checks on a template as written
wherever its text answers them. What a template writes from its fields is
scored once it is rendered, in the dev inbox. The score is an early
indication, so a mail can break a rule on purpose if you say why.

## What it looks like

The HTML links to a plain `http://` address.

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

`domains/accounts/emails/welcome_email.html`:

```jadpo check project=e-email-health-problem file=domains/accounts/emails/welcome_email.html
<p>Hello, {{ name }}.</p>
<p><a href="http://notes.example/start">Start writing</a></p>
```

`domains/accounts/emails/welcome_email.jadpo`:

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
  help: https://jadpo.dev/docs/errors/email.health_problem

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `.html` template with no `.txt` beside it fails another of the checks:

```jadpo check project=e-email-health-problem-text file=app.jadpo hidden
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

```jadpo check project=e-email-health-problem-text file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-health-problem-text file=domains/accounts/emails/welcome_email.html hidden
<p>Hello, {{ name }}.</p>
<p><a href="https://notes.example/start">Start writing</a></p>
```

`domains/accounts/emails/welcome_email.jadpo`:

```jadpo check project=e-email-health-problem-text file=domains/accounts/emails/welcome_email.jadpo expect=email.health_problem
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```text output
error: `WelcomeEmail` has no plain-text part
 --> domains/accounts/emails/welcome_email.jadpo:2:6
  |
2 | type WelcomeEmail = EmailMessage {
  |      ^^^^^^^^^^^^ HTML only
  reason: Filters score mail with HTML and no plain text as spam, and some readers show only text; `welcome_email.html` has no `welcome_email.txt` beside it.
  next: Add `domains/accounts/emails/welcome_email.txt` with the same words as plain text, or write `decline email.health_problem: "<why>"` above it if this is deliberate (agent)
  rule: email.health_problem
  help: https://jadpo.dev/docs/errors/email.health_problem

check failed: 1 error, 3 files
```

## How to fix it

**If the link should be secure**, write it with `https://`. In `domains/accounts/emails/welcome_email.html`:

```jadpo check project=e-email-health-problem-fix file=domains/accounts/emails/welcome_email.html
<p>Hello, {{ name }}.</p>
<p><a href="https://notes.example/start">Start writing</a></p>
```

```jadpo check project=e-email-health-problem-fix file=app.jadpo hidden
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

```jadpo check project=e-email-health-problem-fix file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-health-problem-fix file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.

Start writing: https://notes.example/start
```

```jadpo check project=e-email-health-problem-fix file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

The link is encrypted, and mail filters no longer count it against the
message.

**If the mail breaks the rule on purpose**, keep it and say why, with a
`decline` line above the email's type. In `domains/accounts/emails/welcome_email.jadpo`:

```jadpo check project=e-email-health-problem-decline file=domains/accounts/emails/welcome_email.jadpo
/// Sent once someone signs up.
decline email.health_problem: "The intranet this links to has no HTTPS."
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```jadpo check project=e-email-health-problem-decline file=app.jadpo hidden
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

```jadpo check project=e-email-health-problem-decline file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-health-problem-decline file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.

Start writing: https://notes.example/start
```

```jadpo check project=e-email-health-problem-decline file=domains/accounts/emails/welcome_email.html hidden
<p>Hello, {{ name }}.</p>
<p><a href="http://notes.example/start">Start writing</a></p>
```

The reason is kept with the type for a reviewer to read, and the check
passes.

## Related

- [The mail health score](../language/16-built-in-services.md#the-mail-health-score)
- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)
