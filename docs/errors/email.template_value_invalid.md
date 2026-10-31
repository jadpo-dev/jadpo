# `email.template_value_invalid`

An email template writes out a value that isn't text, a number or an enum with wording, such as a date.

## Why Jadpo refuses it

A template writes out only text, numbers and enums whose values have
wording, because each of those has one way to be shown. A date, an
amount of money or a list has many: a date needs a time zone, a
language and a format before a person can read it. Jadpo won't pick
them for you, so the template says how to show the value, and the error
says how to do it for the type you wrote.

## What it looks like

The welcome email writes out when the person joined, an `Instant`.

```jadpo check project=e-email-template-value file=app.jadpo hidden
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

```jadpo check project=e-email-template-value file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

domains/accounts/emails/welcome_email.jadpo:

```jadpo check project=e-email-template-value file=domains/accounts/emails/welcome_email.jadpo
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    joined_at: Instant
}
```

domains/accounts/emails/welcome_email.html:

```jadpo check project=e-email-template-value file=domains/accounts/emails/welcome_email.html expect=email.template_value_invalid
<p>Welcome, {{ name }}. You joined on {{ joined_at }}.</p>
```

```text output
error: `{{ joined_at }}` can't be written out
 --> domains/accounts/emails/welcome_email.html:1:42
  |
1 | <p>Welcome, {{ name }}. You joined on {{ joined_at }}.</p>
  |                                          ^^^^^^^^^ this is `Instant`
  reason: A template writes out text (`Text`, `PresentationText`, or a type built on one), a number (`Int` or `Decimal`), or an enum whose values have wording, and `Html` as it is; `joined_at` is `Instant`.
  next: Format it with `text.format(...)` (area 19), which writes it in the email's locale (agent)
  rule: email.template_value_invalid
  help: https://jadpo.dev/docs/errors/email.template_value_invalid

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value is a date, a time or an amount**, format it with
`text.format`, which writes it in the email's locale. A time also needs
the zone to show it in, which the email is given as a field, and a
format. In `domains/accounts/emails/welcome_email.jadpo`:

```jadpo fragment
    zone: Zone
```

```jadpo check project=e-email-template-value-fix-1 file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent once someone signs up.
type WelcomeEmail = EmailMessage {
    name: Text
    joined_at: Instant
    zone: Zone
}
```

Then the template formats the time. In
`domains/accounts/emails/welcome_email.html`:

```jadpo fragment
<p>Welcome, {{ name }}. You joined on {{ text.format(joined_at, zone: zone, format: TimeFormat.date_long) }}.</p>
```

```jadpo check project=e-email-template-value-fix-1 file=domains/accounts/emails/welcome_email.html hidden
<p>Welcome, {{ name }}. You joined on {{ text.format(joined_at, zone: zone, format: TimeFormat.date_long) }}.</p>
```

```jadpo check project=e-email-template-value-fix-1 file=domains/accounts/emails/welcome_email.txt hidden
Welcome, {{ name }}. You joined on {{ text.format(joined_at, zone: zone, format: TimeFormat.date_long) }}.
```

```jadpo check project=e-email-template-value-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-email-template-value-fix-1 file=domains/accounts/translations/en_gb.jadpo hidden
translations {
    welcome_email: "Welcome, {name}"
}
```

The email shows the date in the reader's zone, the way its locale
writes it, such as "27 September 2026" in `Locale.en_gb`.

Other values have their own way to be shown. Use a `Bool` as the
condition of an `{{ if }}`, walk a list with `{{ for }}`, write an
object's fields out one by one, and give an enum's values wording.

## Related

- [What a template writes out](../language/16-built-in-services.md#what-a-template-writes-out)
- [Showing dates and times to people](../language/10-configuration-and-time.md#showing-dates-and-times-to-people)
- [Enums with wording](../language/10-configuration-and-time.md#enums-with-wording)
