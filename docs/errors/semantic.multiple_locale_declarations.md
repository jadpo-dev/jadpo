# `semantic.multiple_locale_declarations`

A project declares more than one `locales { }`.

## Why Jadpo refuses it

An application lists the locales it supports once, so formatting and
translations all accept the same ones. With two lists, the same call could
accept a locale in one place and refuse it in another.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.multiple_locale_declarations

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both lists hold locales the app supports**, put them in one block:

```jadpo check project=e-locales-multiple-fix
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us]
}
```

The app now supports both locales, and defaults to `en_gb`.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
