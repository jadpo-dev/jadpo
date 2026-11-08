# `type.locale_in_progress_unsupported`

A locale listed in `in_progress:` isn't in the `supported:` list of `locales { }`.

## Why Jadpo refuses it

`in_progress:` names supported locales whose wording isn't finished yet,
so they may fall back to the default where it is missing. A locale that
isn't supported isn't written at all, so it has no wording to finish.
Listing one there usually means it was meant to be supported, or was
removed from `supported:` and left behind.

## What it looks like

app.jadpo:

```jadpo check project=e-locale-in-progress-unsupported file=app.jadpo expect=type.locale_in_progress_unsupported
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.de_de]
}
```

```text output
error: `Locale.de_de` is in progress but isn't supported
 --> app.jadpo:6:19
  |
5 |     supported: [Locale.en_gb, Locale.fr_fr]
  |                ---------------------------- the supported locales
6 |     in_progress: [Locale.de_de]
  |                   ^^^^^^^^^^^^ not in `supported:`
  reason: `in_progress:` names supported locales whose wording isn't finished: it lets them fall back where wording is missing. A locale that isn't supported isn't written at all.
  next: Add `Locale.de_de` to `supported:`, or remove it from `in_progress:` (agent)
  rule: type.locale_in_progress_unsupported
  help: https://jadpo.dev/docs/errors/type.locale_in_progress_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the app is starting to support the locale.

**If it is**, add it to `supported:` as well:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr, Locale.de_de]
    in_progress: [Locale.de_de]
}
```

```jadpo check project=e-locale-in-progress-unsupported-fix-1 file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr, Locale.de_de]
    in_progress: [Locale.de_de]
}
```

The app now speaks German, falling back to British English where a German
wording is missing.

**If it isn't**, remove it from `in_progress:`, and the line once it is
empty:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-locale-in-progress-unsupported-fix-2 file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

Every supported locale is now finished, so a missing wording in either is
an error.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [Locales still being translated: `in_progress:`](../language/10-configuration-and-time.md#locales-still-being-translated-in_progress)
