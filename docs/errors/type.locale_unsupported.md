# `type.locale_unsupported`

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
  help: docs/language/10-configuration-and-time.html#type-locale-unsupported

check failed: 1 error, 1 file
```
