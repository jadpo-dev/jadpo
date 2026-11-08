# `type.locale_in_progress_default`

The `default:` locale of `locales { }` is also listed in `in_progress:`, as a locale whose wording isn't finished.

## Why Jadpo refuses it

A locale in `in_progress:` falls back to the default wherever its wording
is missing, so the default is the last place a missing wording can come
from. If the default were unfinished too, a message could have no wording
in any language. So the default is always worded in full.

## What it looks like

app.jadpo:

```jadpo check project=e-locale-in-progress-default file=app.jadpo expect=type.locale_in_progress_default
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.en_gb]
}
```

```text output
error: The default `Locale.en_gb` can't be in progress
 --> app.jadpo:6:19
  |
4 |     default: Locale.en_gb
  |              ------------ the default
6 |     in_progress: [Locale.en_gb]
  |                   ^^^^^^^^^^^^ the default locale
  reason: An in-progress locale falls back to the default where wording is missing, so the default is worded in full, and the last place a missing wording can come from.
  next: Remove `Locale.en_gb` from `in_progress:`, or choose another default (agent)
  rule: type.locale_in_progress_default
  help: https://jadpo.dev/docs/errors/type.locale_in_progress_default

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is which locale isn't finished.

**If another locale is the unfinished one**, list that one instead:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

```jadpo check project=e-locale-in-progress-default-fix-1 file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

French falls back to British English where its wording is missing, and
every British English wording must be there.

**If the default's own wording isn't finished**, make a finished locale
the default:

```jadpo fragment
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.en_gb]
}
```

```jadpo check project=e-locale-in-progress-default-fix-2 file=app.jadpo hidden
application {}

locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.en_gb]
}
```

French is now the default, worded in full, and British English falls back
to it where its wording is missing.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [Locales still being translated: `in_progress:`](../language/10-configuration-and-time.md#locales-still-being-translated-in_progress)
