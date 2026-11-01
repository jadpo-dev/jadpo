# `cli.translations_arguments`

A `jadpo translations` command names no action, one it doesn't have, or a locale the project doesn't translate into.

## Why Jadpo refuses it

`jadpo translations` lists what is left to translate, records the wording each translation was made from, and exchanges it with a translation service. Accepting a locale's wording is a person's decision about one locale, so it needs to know which.

## What it looks like

A project that supports only its default locale, asked to accept French:

```jadpo check project=e-translations-arguments file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```sh run project=e-translations-arguments expect=cli.translations_arguments
jadpo translations accept fr
```

```text output
error: `fr` isn't a locale with translations
  reason: This project supports no locale besides its default, which is what the others are translated from.
  next: Add a locale to `supported:` in `locales { }` (agent)
  rule: cli.translations_arguments
  help: https://jadpo.dev/docs/errors/cli.translations_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-translations-arguments expect=cli.translations_arguments
jadpo translations sync
```

```text output
error: `sync` isn't something `jadpo translations` does
  reason: `jadpo translations` can `missing`, `unused`, `update`, `accept`, `builtin`, `export` or `import`.
  next: Run `jadpo translations missing` to see what is left to translate (agent)
  rule: cli.translations_arguments
  help: https://jadpo.dev/docs/errors/cli.translations_arguments
```

## How to fix it

**If the app should be translated into French**, add it to `supported:` in `locales { }`, then accept its wording:

```sh
jadpo translations accept fr
```

It records the wording each French message was made from.

## Related

- [`jadpo translations`](../language/14-tooling.md#jadpo-translations)
- [Locales](../language/10-configuration-and-time.md#locales)
