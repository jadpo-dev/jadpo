# `semantic.multiple_locale_declarations`

```jadpo check project=e-locales-multiple expect=semantic.multiple_locale_declarations
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

locales {
    default: Locale.en_us
    supported: [Locale.en_us]
}
```

```text output
error: `locales { }` is declared twice
 --> main.jadpo:6:1
  |
1 | locales {
  | ------- first declared here
6 | locales {
  | ^^^^^^^ declared again
  reason: An application lists the locales it supports once, in `app.jadpo`, so formatting and translations all accept the same ones.
  next: Move what this `locales` lists into the first, and remove it (agent)
  rule: semantic.multiple_locale_declarations
  help: docs/language/10-configuration-and-time.html#semantic-multiple-locale-declarations

check failed: 1 error, 1 file
```
