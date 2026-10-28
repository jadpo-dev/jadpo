# `type.locale_in_progress_unsupported`

`in_progress:` names supported locales whose wording isn't finished; a locale that isn't supported isn't written at all.

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
  help: docs/errors/type.locale_in_progress_unsupported.md

check failed: 1 error, 1 file
```
