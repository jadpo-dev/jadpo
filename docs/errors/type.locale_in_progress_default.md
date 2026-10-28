# `type.locale_in_progress_default`

An in-progress locale falls back to the default where wording is missing, so the default is worded in full.

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
  help: docs/errors/type.locale_in_progress_default.md

check failed: 1 error, 1 file
```
