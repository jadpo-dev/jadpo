# `type.locale_invalid`

A locale is a name in `Locale`, not text; a tag in quotes is the common
slip, and the fix is one edit:

```jadpo check project=e-locale-invalid expect=type.locale_invalid
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, "fr-FR"]
}
```

```text output
error: `"fr-FR"` isn't written as a locale
 --> main.jadpo:3:31
  |
3 |     supported: [Locale.en_gb, "fr-FR"]
  |                               ^^^^^^^ not a `Locale`
  reason: A locale is a name in `Locale`, written `Locale.<tag>` with the tag in lower case and `_` for `-`: `Locale.en_gb` is en-GB. Closed choices are names, not text, so a misspelt one is caught while compiling.
  next: Write `Locale.fr_fr` instead (agent)
  rule: type.locale_invalid
  help: https://jadpo.dev/docs/errors/type.locale_invalid

check failed: 1 error, 1 file
```
