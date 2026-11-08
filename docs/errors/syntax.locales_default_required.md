# `syntax.locales_default_required`

A `locales { }` declaration has no `default:`.

## Why Jadpo refuses it

The default is the locale a choice from an `Accept-Language` header falls
back to, so every request ends up with a language the application supports.
Jadpo won't pick it for you, because the default is the language someone
gets when nothing else matches.

## What it looks like

main.jadpo:

```jadpo check project=e-locales-default expect=syntax.locales_default_required
locales {
    supported: [Locale.en_gb]
}
```

```text output
error: `locales` has no `default:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `default:`
  reason: The default is the locale a choice from an `Accept-Language` header falls back to, so every request ends up with a language the application supports.
  next: Add `default: Locale.en_gb` (agent)
  rule: syntax.locales_default_required
  help: https://jadpo.dev/docs/errors/syntax.locales_default_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the app has one language to fall back to**, add `default:`:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-locales-default-fix hidden
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

A request whose `Accept-Language` matches none of the supported locales now
gets British English.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
