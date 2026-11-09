# `email.field_unused`

An email or partial has a field no template and no subject writes.

## Why Jadpo refuses it

An email's fields are what its templates and its subject read, so a field
none of them reads is sent for nothing, or is a value a template was meant
to show. It might be either, so it is a judgement call: the check stops, and
you say which. A partial's fields are checked the same way.

## What it looks like

`WelcomeEmail` has a `referral_code`, and its template never writes it.

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

`domains/accounts/emails/welcome_email.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the template should show it**, write it. In `domains/accounts/emails/welcome_email.txt`:

```jadpo check project=e-email-field-unused-write file=domains/accounts/emails/welcome_email.txt
Hello, {{ name }}.

Your referral code: {{ referral_code }}
```

```jadpo check project=e-email-field-unused-write file=app.jadpo hidden
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

```jadpo check project=e-email-field-unused-write file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-field-unused-write file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    referral_code: Text
}
```

The email now tells the reader their code.

**If the field isn't needed**, remove it. In `domains/accounts/emails/welcome_email.jadpo`:

```jadpo check project=e-email-field-unused-remove file=domains/accounts/emails/welcome_email.jadpo
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```jadpo check project=e-email-field-unused-remove file=app.jadpo hidden
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

```jadpo check project=e-email-field-unused-remove file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-field-unused-remove file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.
```

The email takes only what it writes, and each `emit` stops passing it.

**If it is needed for something else**, keep it and say why, with a
`decline` line above it. In `domains/accounts/emails/welcome_email.jadpo`:

```jadpo check project=e-email-field-unused-decline file=domains/accounts/emails/welcome_email.jadpo
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    decline email.field_unused: "Read by the billing system from the sent mail."
    referral_code: Text
}
```

```jadpo check project=e-email-field-unused-decline file=app.jadpo hidden
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

```jadpo check project=e-email-field-unused-decline file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-field-unused-decline file=domains/accounts/emails/welcome_email.txt hidden
Hello, {{ name }}.
```

The reason stays in the code for a reviewer to read, and the check passes.

## Related

- [Templates](../language/16-built-in-services.md#templates)
- [Partials: pieces several emails share](../language/16-built-in-services.md#partials-pieces-several-emails-share)
- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)
