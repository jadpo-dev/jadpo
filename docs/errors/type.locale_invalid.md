# `type.locale_invalid`

A locale in `locales { }` is written as something other than one of `Locale`'s names, such as a tag in quotes or a misspelt `Locale.en_bg`.

## Why Jadpo refuses it

The locales an app can support are a closed list, so each is a name in
`Locale`, written `Locale.<tag>` with the tag in lower case and `_` for
`-`: `Locale.en_gb` is en-GB. A name is checked while compiling, so a
misspelt locale is caught here rather than when a request asks for it.
Text in quotes, such as `"fr-FR"`, is the common slip.

## What it looks like

app.jadpo:

```jadpo check project=e-locale-invalid file=app.jadpo expect=type.locale_invalid
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, "fr-FR"]
}
```

```text output
error: `"fr-FR"` isn't written as a locale
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, "fr-FR"]
  |                               ^^^^^^^ not a `Locale`
  reason: A locale is a name in `Locale`, written `Locale.<tag>` with the tag in lower case and `_` for `-`: `Locale.en_gb` is en-GB. Closed choices are names, not text, so a misspelt one is caught while compiling.
  next: Write `Locale.fr_fr` instead (agent)
  rule: type.locale_invalid
  help: https://jadpo.dev/docs/errors/type.locale_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name that `Locale` doesn't have is the same error, and the `next:` line
gives the nearest one:

app.jadpo:

```jadpo check project=e-locale-invalid-name file=app.jadpo expect=type.locale_invalid
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fx]
}
```

```text output
error: `Locale.fr_fx` isn't a locale
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, Locale.fr_fx]
  |                               ^^^^^^^^^^^^ no such locale
  reason: `Locale` has no variant called `fr_fx`; a variant is the locale's tag in lower case with `_` for `-`, such as `en_gb` for en-GB. Did you mean `fr_fr`?
  next: Use `Locale.fr_fr` instead (agent)
  rule: type.locale_invalid
  help: https://jadpo.dev/docs/errors/type.locale_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If you meant a locale**, write it as a name in `Locale`:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-locale-invalid-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

The app supports the French of France as well as British English.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [`Zone`, `Weekday` and `Locale`](../language/10-configuration-and-time.md#zone-weekday-and-locale)
