# `syntax.locales_duplicate_option`

```jadpo check project=e-locales-twice expect=syntax.locales_duplicate_option
locales {
    default: Locale.en_gb
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```text output
error: `default:` is given twice
 --> main.jadpo:3:5
  |
2 |     default: Locale.en_gb
  |              ------------ first given here
3 |     default: Locale.fr_fr
  |     ^^^^^^^ given again
  reason: `locales` says each of `default:` and `supported:` once.
  next: Keep one `default:` and remove the other (agent)
  rule: syntax.locales_duplicate_option
  help: docs/errors/syntax.locales_duplicate_option.md

check failed: 1 error, 1 file
```
