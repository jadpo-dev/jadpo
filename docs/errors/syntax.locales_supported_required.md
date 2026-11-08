# `syntax.locales_supported_required`

A `locales { }` declaration has no `supported:`.

## Why Jadpo refuses it

`supported:` lists every locale formatting and translations accept, and
Jadpo can't work out which languages your application speaks. A locale not
listed is a compile error where it is written and an invalid value where it
is read.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.locales_supported_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the app speaks only its default**, list that one:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-locales-supported-fix-1 hidden
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

The app now supports British English alone.

**If the app speaks others too**, list each of them:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-locales-supported-fix-2 hidden
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

The app now supports British English and French, so its messages need French
wording too.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
