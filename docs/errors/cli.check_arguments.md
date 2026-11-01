# `cli.check_arguments`

A `jadpo check` command names a path that isn't there or isn't a `.jadpo` file, or a `--before` git doesn't know.

## Why Jadpo refuses it

`jadpo check` checks a project, a folder or one `.jadpo` file. A path that isn't there, or a file of another kind, would leave nothing checked, and saying it passed would be wrong.

## What it looks like

The path is misspelt:

```jadpo check project=e-check-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-check-arguments expect=cli.check_arguments
jadpo check app.jadp
```

```text output
error: `app.jadp` doesn't exist
  reason: There is no file or folder at this path.
  next: Give the path of a project, a `.jadpo` file or a folder (agent)
  rule: cli.check_arguments
  help: https://jadpo.dev/docs/errors/cli.check_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A file that isn't a `.jadpo` file is refused too:

```sh run project=e-check-arguments expect=cli.check_arguments
touch notes.txt
jadpo check notes.txt
```

```text output
error: `notes.txt` isn't a Jadpo file
  reason: `jadpo check` reads files whose names end in `.jadpo`.
  next: Give a `.jadpo` file, or a folder to check every one in it (agent)
  rule: cli.check_arguments
  help: https://jadpo.dev/docs/errors/cli.check_arguments
```

## How to fix it

**If you meant the whole project**, leave the path out:

```sh
jadpo check
```

It finds the project by walking up to `app.jadpo` and checks every file in it.

**If you meant one file**, give its path as it is:

```sh
jadpo check app.jadpo
```

Only that file's errors are printed.

## Related

- [`jadpo check`](../language/14-tooling.md#jadpo-check)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
