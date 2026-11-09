# `translation.locales_required`

A project has translations or an email, but `app.jadpo` declares no locales.

## Why Jadpo refuses it

Each language file is for one of the locales the application supports, and
each of those locales words every message. Until `locales { }` says which
they are, there is nothing to check a file against, and no way to tell
whether a language is finished.

## What it looks like

A language file is written, but `app.jadpo` says nothing about languages.

```jadpo check project=e-tr-locales-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-tr-locales-required file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

`domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=e-tr-locales-required file=domains/shipping/translations/en_gb.jadpo expect=translation.locales_required
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: Translations need the application's locales
 --> domains/shipping/translations/en_gb.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ no locales declared
  reason: Each language file is for one of the locales the application supports, and each supported locale words every message. None is declared, so there is nothing to check the wording against.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: translation.locales_required
  help: https://jadpo.dev/docs/errors/translation.locales_required

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An email needs them too, for its subject: the subject is the email's member in
its domain's language files, so there is nothing to send it with until a
locale words it.

```jadpo check project=e-tr-locales-required-email file=app.jadpo hidden
application {}
```

```jadpo check project=e-tr-locales-required-email file=domains/accounts/emails/welcome_email.txt hidden
Welcome to Acme, {{ display_name }}.
```

`domains/accounts/emails/welcome_email.jadpo`:

```jadpo check project=e-tr-locales-required-email file=domains/accounts/emails/welcome_email.jadpo expect=translation.locales_required
/// Sent when someone signs up.
type WelcomeEmail = EmailMessage {
    display_name: Text
}
```

```text output
error: Translations need the application's locales
 --> domains/accounts/emails/welcome_email.jadpo:2:6
  |
2 | type WelcomeEmail = EmailMessage {
  |      ^^^^^^^^^^^^ no locales declared
  reason: Each language file is for one of the locales the application supports, and each supported locale words every message. None is declared, so there is nothing to check the wording against.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: translation.locales_required
  help: https://jadpo.dev/docs/errors/translation.locales_required

check failed: 1 error, 2 files
```

## How to fix it

**If the application has languages to word**, declare them in `app.jadpo`:

```jadpo check project=e-tr-locales-required-fix file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-locales-required-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locales-required-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

Jadpo checks each language file against the locales you listed, and an
email's subject has a language to be worded in.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [Email subjects](../language/10-configuration-and-time.md#email-subjects)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
