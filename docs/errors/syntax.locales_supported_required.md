# `syntax.locales_supported_required`

```jadpo check project=e-locales-supported expect=syntax.locales_supported_required
locales {
    default: Locale.en_gb
}
```

```text output
error: `locales` has no `supported:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `supported:`
  reason: `supported:` lists every locale formatting and translations accept; a locale not listed is a compile error where it is written and an invalid value where it is read.
  next: Add `supported: [Locale.en_gb]`, listing every locale the application speaks (agent)
  rule: syntax.locales_supported_required
  help: docs/errors/syntax.locales_supported_required.md

check failed: 1 error, 1 file
```
