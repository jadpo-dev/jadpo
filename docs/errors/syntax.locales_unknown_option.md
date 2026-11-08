# `syntax.locales_unknown_option`

A `locales { }` declaration sets an option that doesn't exist.

## Why Jadpo refuses it

`locales` takes `default:`, `supported:` and `in_progress:`, and nothing
else. There is no `unsupported:` setting, because a locale you don't list is
already refused: a request field typed `Locale` is decoded like any enum, so
a tag the application doesn't support is an `invalid_value` failure, and
choosing a language from an `Accept-Language` header falls back to
`default:`.

## What it looks like

main.jadpo:

```jadpo check project=e-locales-unknown expect=syntax.locales_unknown_option
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
    unsupported: Locale.fr_fr
}
```

```text output
error: `unsupported` isn't an option of `locales`
 --> main.jadpo:4:5
  |
4 |     unsupported: Locale.fr_fr
  |     ^^^^^^^^^^^ unknown option
  reason: There is no `unsupported:` setting. A request field typed `Locale` is decoded like any enum, so a tag the application doesn't support is an `invalid_value` failure, and choosing a language from an `Accept-Language` header falls back to `default:`.
  next: Remove `unsupported:` (agent)
  rule: syntax.locales_unknown_option
  help: https://jadpo.dev/docs/errors/syntax.locales_unknown_option

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A misspelt option is this error too, and its `reason:` names the nearest
option:

main.jadpo:

```jadpo check project=e-locales-unknown-2 expect=syntax.locales_unknown_option
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progres: [Locale.fr_fr]
}
```

```text output
error: `in_progres` isn't an option of `locales`
 --> main.jadpo:4:5
  |
4 |     in_progres: [Locale.fr_fr]
  |     ^^^^^^^^^^ unknown option
  reason: `locales` takes `default:`, `supported:` and `in_progress:`, and nothing else. Did you mean `in_progress`?
  next: Use `in_progress` instead of `in_progres` (agent)
  rule: syntax.locales_unknown_option
  help: https://jadpo.dev/docs/errors/syntax.locales_unknown_option

check failed: 1 error, 1 file
```

## How to fix it

**If you meant to refuse a locale**, remove the line:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-locales-unknown-fix-1 hidden
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

French stays unsupported: it isn't listed, and a request field holding it
fails with `invalid_value`.

**If you misspelt an option**, write its name:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

```jadpo check project=e-locales-unknown-fix-2 hidden
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

French is now listed as a locale still being translated.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [A locale the app doesn't support](../language/10-configuration-and-time.md#a-locale-the-app-doesnt-support)
