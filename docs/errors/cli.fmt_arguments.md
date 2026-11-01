# `cli.fmt_arguments`

A `jadpo fmt` command names a path that isn't there or isn't a `.jadpo` file.

## Why Jadpo refuses it

`jadpo fmt` formats a folder or one `.jadpo` file. A path that isn't there, or a file of another kind, would leave nothing formatted, and saying it was would be wrong.

## What it looks like

The path is misspelt:

```jadpo check project=e-fmt-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-fmt-arguments expect=cli.fmt_arguments
jadpo fmt app.jadp
```

```text output
error: `app.jadp` doesn't exist
  reason: There is no file or folder at this path.
  next: Give the path of a `.jadpo` file or a folder (agent)
  rule: cli.fmt_arguments
  help: https://jadpo.dev/docs/errors/cli.fmt_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant the whole project**, leave the path out:

```sh
jadpo fmt
```

Every `.jadpo` file in the project is formatted.

## Related

- [`jadpo fmt`](../language/14-tooling.md#jadpo-fmt)
- [Formatting](../language/14-tooling.md#formatting)
