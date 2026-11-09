# `email.settings_invalid`

An application's `email:` or `display_name:` setting is invalid.

## Why Jadpo refuses it

`email:` names the connection every email goes through, one that provides
`email`, and the address they are from, and `display_name:` is the
application's name as its readers see it. If either is wrong, every email
would fail in the same way when it was sent, so Jadpo stops at the setting
instead.

## What it looks like

`email:` names `main`, the database, which sends no email.

`app.jadpo`:

```jadpo check project=e-email-settings-invalid file=app.jadpo expect=email.settings_invalid
application {
    email: {
        connection: main
        from: "hello@notes.example"
    }
}
```

```text output
error: `connection: main` sends no email
 --> app.jadpo:3:21
  |
3 |         connection: main
  |                     ^^^^ not an email connection
  reason: `email: { connection: }` names a connection that provides `email`: one with `driver: Driver.smtp`, or to a mail package that maps `send` in its `provides/email.jadpo`; `main` is a connection that sends no email, and `app.jadpo` declares none with `driver: Driver.smtp`.
  next: Declare a connection with `driver: Driver.smtp` in `connections:`, and name it here (agent)
  rule: email.settings_invalid
  help: https://jadpo.dev/docs/errors/email.settings_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`{{ app.display_name }}` needs the name to write:

```jadpo check project=e-email-settings-invalid-name file=app.jadpo hidden
application {
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

```jadpo check project=e-email-settings-invalid-name file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-settings-invalid-name file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

`domains/accounts/emails/welcome_email.txt`:

```jadpo check project=e-email-settings-invalid-name file=domains/accounts/emails/welcome_email.txt expect=email.settings_invalid
Hello {{ name }}, from {{ app.display_name }}.
```

```text output
error: `app.display_name` is written, but the application has no `display_name:`
 --> domains/accounts/emails/welcome_email.txt:1:27
  |
1 | Hello {{ name }}, from {{ app.display_name }}.
  |                           ^^^^^^^^^^^^^^^^ no name to write
  reason: `{{ app.display_name }}` writes `application { display_name: }`, the application's name as its readers see it, which `app.jadpo` doesn't give.
  next: Add `display_name: "<the application's name>"` to `application { }` in `app.jadpo` (agent)
  rule: email.settings_invalid
  help: https://jadpo.dev/docs/errors/email.settings_invalid

check failed: 1 error, 3 files
```

An `email:` that isn't a block, has an option it doesn't take or leaves one
out, a `from:` that isn't an address, and a `display_name:` that isn't the
name written out are all refused the same way.

## How to fix it

**If the connection doesn't send email**, declare one that does and name it.
In `app.jadpo`:

```jadpo check project=e-email-settings-invalid-fix file=app.jadpo
application {
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
}
```

Every email now goes through `mail`, from `hello@notes.example`.

**If `app.display_name` has no name to write**, give the application one, in
`app.jadpo`.

```jadpo check project=e-email-settings-invalid-name-fix file=app.jadpo
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

```jadpo check project=e-email-settings-invalid-name-fix file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-settings-invalid-name-fix file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
}
```

```jadpo check project=e-email-settings-invalid-name-fix file=domains/accounts/emails/welcome_email.txt hidden
Hello {{ name }}, from {{ app.display_name }}.
```

The email writes "Notes" where the name stands.

## Related

- [Sending](../language/16-built-in-services.md#sending)
- [The application's name](../language/16-built-in-services.md#the-applications-name)
- [Email](../language/16-built-in-services.md#email)
