# `syntax.locales_default_required`

```jadpo check project=e-locales-default expect=syntax.locales_default_required
locales {
    supported: [Locale.en_gb]
}
```

```text output
error: `locales` has no `default:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `default:`
  reason: The default is the locale a choice from an `Accept-Language` header falls back to, so every request ends up with a language the application supports.
  next: Add `default: Locale.en_gb` (agent)
  rule: syntax.locales_default_required
  help: docs/errors/syntax.locales_default_required.md

check failed: 1 error, 1 file
```
