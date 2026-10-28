# `syntax.locales_unknown_option`

There is no `unsupported:` setting (see "Locales"). A misspelt
option (`suported:`) is reported the same way, with the nearest name:

```jadpo check project=e-locales-unknown expect=syntax.locales_unknown_option
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
    unsupported: Locale.fr_fr
}
```

```text output
error: `unsupported` isn't an option of `locales`
 --> main.jadpo:4:5
  |
4 |     unsupported: Locale.fr_fr
  |     ^^^^^^^^^^^ unknown option
  reason: There is no `unsupported:` setting. A request field typed `Locale` is decoded like any enum, so a tag the application doesn't support is an `invalid_value` failure, and choosing a language from an `Accept-Language` header falls back to `default:`.
  next: Remove `unsupported:` (agent)
  rule: syntax.locales_unknown_option
  help: docs/language/10-configuration-and-time.html#syntax-locales-unknown-option

check failed: 1 error, 1 file
```
