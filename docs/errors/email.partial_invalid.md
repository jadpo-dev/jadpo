# `email.partial_invalid`

A template includes a partial by its name, giving each of its fields once
by name, so what a shared piece needs is checked where it is used; a
partial that doesn't exist, can't be reached from the template, lacks the
template it is included from, is given the wrong fields or includes
itself is refused:

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

```jadpo check project=e-email-partial-invalid file=shared/emails/button.jadpo hidden
/// A link set as a button.
type Button = EmailPartial {
    label: Text
    url: Url
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
