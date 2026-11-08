# `syntax.locales_duplicate_option`

An option of `locales { }` is given twice.

## Why Jadpo refuses it

`locales` says each of `default:` and `supported:` once. Two of them could
be a slip or a change of mind, and when they disagree only you know which is
meant.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.locales_duplicate_option

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the first one is right**, remove the second:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-locales-twice-fix-1 hidden
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

British English is now the app's default.

**If the second one is right**, remove the first:

```jadpo fragment
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-locales-twice-fix-2 hidden
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

French is now the app's default.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
