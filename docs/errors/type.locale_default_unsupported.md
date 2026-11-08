# `type.locale_default_unsupported`

The `default:` of `locales { }` is a locale its `supported:` list doesn't have.

## Why Jadpo refuses it

The default locale is the one a request gets when it asks for no locale
the app supports, and formatting and translations accept only the
supported locales. A default outside the list would be a language the app
promises to fall back to but never writes. Jadpo won't add it to the list
or pick another default, because which languages the app speaks is your
choice.

## What it looks like

app.jadpo:

```jadpo check project=e-locale-default file=app.jadpo expect=type.locale_default_unsupported
application {}

locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb]
}
```

```text output
error: The default `Locale.fr_fr` isn't a supported locale
 --> app.jadpo:4:14
  |
4 |     default: Locale.fr_fr
  |              ^^^^^^^^^^^^ not in `supported:`
5 |     supported: [Locale.en_gb]
  |                -------------- the supported locales
  reason: The default is what a request with no usable `Accept-Language` is given, and formatting and translations accept only the supported locales, so the default has to be one.
  next: Add `Locale.fr_fr` to `supported:`, or choose a default that is listed (agent)
  rule: type.locale_default_unsupported
  help: https://jadpo.dev/docs/errors/type.locale_default_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the app speaks the default's language.

**If it does**, add the default to `supported:`:

```jadpo fragment
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-locale-default-fix-1 file=app.jadpo hidden
application {}

locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

The app now supports French as well, so its messages need French
wording too.

**If it doesn't**, choose a default from the list:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-locale-default-fix-2 file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

A request that asks for no supported locale is now answered in British
English.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
