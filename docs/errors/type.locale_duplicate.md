# `type.locale_duplicate`

```jadpo check project=e-locale-duplicate expect=type.locale_duplicate
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_gb]
}
```

```text output
error: `Locale.en_gb` is listed twice
 --> main.jadpo:3:31
  |
3 |     supported: [Locale.en_gb, Locale.en_gb]
  |                               ^^^^^^^^^^^^ listed again
  |                 ------------ first listed here
  reason: A locale is listed or it is not; listing it twice says nothing more.
  next: Remove the second `Locale.en_gb` (agent)
  rule: type.locale_duplicate
  help: https://jadpo.dev/docs/errors/type.locale_duplicate

check failed: 1 error, 1 file
```
