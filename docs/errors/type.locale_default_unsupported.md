# `type.locale_default_unsupported`

```jadpo check project=e-locale-default expect=type.locale_default_unsupported
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb]
}
```

```text output
error: The default `Locale.fr_fr` isn't a supported locale
 --> main.jadpo:2:14
  |
2 |     default: Locale.fr_fr
  |              ^^^^^^^^^^^^ not in `supported:`
3 |     supported: [Locale.en_gb]
  |                -------------- the supported locales
  reason: The default is what a request with no usable `Accept-Language` is given, and formatting and translations accept only the supported locales, so the default has to be one.
  next: Add `Locale.fr_fr` to `supported:`, or choose a default that is listed (agent)
  rule: type.locale_default_unsupported
  help: https://jadpo.dev/docs/errors/type.locale_default_unsupported

check failed: 1 error, 1 file
```
