# `type.locale_unsupported`

Your code writes out a locale, such as `Locale.fr_fr`, that isn't in the app's list of supported locales.

## Why Jadpo refuses it

Your app lists the locales it supports, once, in `locales { }`.
Formatting and translations work only in those, so a deployable carries
the language data of those locales and no others. A locale written out in
code that the list doesn't have would format text in a language the app
was never set up for.

Jadpo won't add the locale to the list for you when you could equally have
meant one that is listed. The server's own locale is never used either,
so the code says which one it means.

## What it looks like

main.jadpo:

```jadpo check project=e-locale-unsupported expect=type.locale_unsupported
type OrderedAt = Instant {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

function placed_label(ordered_at: OrderedAt) -> PresentationText {
    return text.format(
        ordered_at,
        zone: Zone.utc,
        locale: Locale.fr_fr,
        format: TimeFormat.date_long
    )
}
```

```text output
error: `Locale.fr_fr` isn't a locale this application supports
 --> main.jadpo:12:17
   |
12 |         locale: Locale.fr_fr,
   |                 ^^^^^^^^^^^^ not in `locales`
  reason: `text.format` writes text in the locales listed in `locales { supported: }`, in `app.jadpo`; `Locale.fr_fr` is not one of them, and the server never guesses a language.
  next: Add `Locale.fr_fr` to `supported:` in `app.jadpo`, or use a locale that is listed (agent)
  rule: type.locale_unsupported
  help: https://jadpo.dev/docs/errors/type.locale_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the app should support French**, add it to `supported:`:

```jadpo fragment
    supported: [Locale.en_gb, Locale.fr_fr]
```

```jadpo check project=e-locale-unsupported-add hidden
type OrderedAt = Instant {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}

function placed_label(ordered_at: OrderedAt) -> PresentationText {
    return text.format(
        ordered_at,
        zone: Zone.utc,
        locale: Locale.fr_fr,
        format: TimeFormat.date_long
    )
}
```

The date is written in French, and a deployable carries French's data.
An app with translations then needs a French wording for each message.

**If the text should be in the reader's language**, take their locale as
a parameter instead of writing one out:

```jadpo fragment
function placed_label(ordered_at: OrderedAt, reader_locale: Locale) -> PresentationText {
        locale: reader_locale,
```

```jadpo check project=e-locale-unsupported-reader hidden
type OrderedAt = Instant {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

function placed_label(ordered_at: OrderedAt, reader_locale: Locale) -> PresentationText {
    return text.format(
        ordered_at,
        zone: Zone.utc,
        locale: reader_locale,
        format: TimeFormat.date_long
    )
}
```

The date is written in the locale the caller passes. A locale the app
doesn't support that still arrives while the app runs is written in a
supported locale of its language, or else in `default:`.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [A locale the app doesn't support](../language/10-configuration-and-time.md#a-locale-the-app-doesnt-support)
- [`zone:` and `locale:`](../language/10-configuration-and-time.md#zone-and-locale)
