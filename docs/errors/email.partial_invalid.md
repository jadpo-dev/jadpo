# `email.partial_invalid`

An email template includes a partial that doesn't exist, can't be reached from it, or is given the wrong fields.

## Why Jadpo refuses it

A partial is a piece several emails share, such as a button, declared as
a type with its own fields. A template includes it with
`{{ include name(field: value, ...) }}`, giving each field once by name,
so what the piece needs is checked where it is used. An include that
names a field the partial doesn't have, or a partial the template can't
reach, would send an email with a broken piece in it.

Jadpo won't guess which field a wrong name meant, so it refuses the
include when you check, rather than when the email is sent.

## What it looks like

The welcome email includes the shared button, which has a `label` and a
`url`.

shared/emails/button.jadpo:

```jadpo check project=e-email-partial-invalid file=shared/emails/button.jadpo
/// A link set as a button.
type Button = EmailPartial {
    label: Text
    url: Url
}
```

```jadpo check project=e-email-partial-invalid file=app.jadpo hidden
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

```jadpo check project=e-email-partial-invalid file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-partial-invalid file=shared/emails/button.html hidden
<a href="{{ url }}">{{ label }}</a>
```

```jadpo check project=e-email-partial-invalid file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    notes_link: Url
}
```

domains/accounts/emails/welcome_email.html:

```jadpo check project=e-email-partial-invalid file=domains/accounts/emails/welcome_email.html expect=email.partial_invalid
<p>Welcome, {{ name }}.</p>
{{ include button(label: name, link: notes_link) }}
```

```text output
error: `button` has no field `link`
 --> domains/accounts/emails/welcome_email.html:2:32
  |
2 | {{ include button(label: name, link: notes_link) }}
  |                                ^^^^ not one of its fields
  reason: A partial is a `type Name = EmailPartial { ... }` in `shared/emails/` or the domain's own `emails/`, its `.html` and `.txt` beside it; a template includes it as `{{ include name(field: value, ...) }}`, giving each of its fields once by name, and its locale goes with it.
  next: Remove `link:` (agent)
  rule: email.partial_invalid
  help: https://jadpo.dev/docs/errors/email.partial_invalid

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field has another name in the partial**, give it by that name.
`Button` calls the link `url`, so in
`domains/accounts/emails/welcome_email.html`:

```text
{{ include button(label: name, url: notes_link) }}
```

```jadpo check project=e-email-partial-invalid-fix-1 file=domains/accounts/emails/welcome_email.html hidden
<p>Welcome, {{ name }}.</p>
{{ include button(label: name, url: notes_link) }}
```

```jadpo check project=e-email-partial-invalid-fix-1 file=domains/accounts/emails/welcome_email.txt hidden
Welcome, {{ name }}.
{{ include button(label: name, url: notes_link) }}
```

```jadpo check project=e-email-partial-invalid-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-email-partial-invalid-fix-1 file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

```jadpo check project=e-email-partial-invalid-fix-1 file=shared/emails/button.jadpo hidden
/// A link set as a button.
type Button = EmailPartial {
    label: Text
    url: Url
}
```

```jadpo check project=e-email-partial-invalid-fix-1 file=shared/emails/button.html hidden
<a href="{{ url }}">{{ label }}</a>
```

```jadpo check project=e-email-partial-invalid-fix-1 file=shared/emails/button.txt hidden
{{ label }}: {{ url }}
```

```jadpo check project=e-email-partial-invalid-fix-1 file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    notes_link: Url
}
```

The welcome email's button now links to `notes_link`.

A partial in another domain's `emails/` can't be reached from this one.
Move it to `shared/emails/`, where every domain's emails may include it.

## Related

- [Partials: pieces several emails share](../language/16-built-in-services.md#partials-pieces-several-emails-share)
- [Which partials a template reaches](../language/16-built-in-services.md#which-partials-a-template-reaches)
- [What a template may not do](../language/16-built-in-services.md#what-a-template-may-not-do)
