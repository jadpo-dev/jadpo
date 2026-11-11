# `cli.files_arguments`

A `jadpo files` command names an action it doesn't have, a store the app doesn't keep files in, a file the store doesn't hold, or a local file it can't use.

## Why Jadpo refuses it

`jadpo files` reaches a store of files by its connection in `app.jadpo`,
and a file by the key the runtime made for it. It never acts on another
store or file for a name it doesn't know, never writes over a local file,
and never puts bytes under a key that already holds a file, since a file
never changes once it is kept.

## What it looks like

A bakery keeps its price sheets in a store of files:

```jadpo check project=e-files-arguments file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

A store named a letter short is refused with the one nearest:

```sh run project=e-files-arguments expect=cli.files_arguments
jadpo files ls sheet --dev
```

```text output
error: `sheet` isn't a store of files
  reason: A store is named by its connection in `app.jadpo`: `sheets`.
  next: Use `sheets` (agent)
  rule: cli.files_arguments
  help: https://jadpo.dev/docs/errors/cli.files_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-files-arguments expect=cli.files_arguments
jadpo files list sheets --dev
```

```text output
error: `list` isn't something `jadpo files` does
  reason: `jadpo files` lists the app's stores of files, and `ls`, `show`, `get`, `put` and `copy` act on one.
  next: Run `jadpo files` to list the stores (agent)
  rule: cli.files_arguments
  help: https://jadpo.dev/docs/errors/cli.files_arguments
```

## How to fix it

**If the store or the action is misspelt**, use the one the command
names:

```sh
jadpo files ls sheets --dev
```

The command lists the files `jadpo dev`'s copy of `sheets` keeps.

**If you don't know a store's name or a file's key**, list them first:

```sh
jadpo files
jadpo files ls sheets --dev
```

The first lists the app's stores, and the second the keys one keeps.

**If a local file is in the way**, name a file that isn't there yet:

```sh
jadpo files get sheets 3f/a2/3fa2c1d0-0000-4000-8000-000000000001 sheet-copy.csv --dev
```

The command writes the new file and leaves the other one alone.

## Related

- [`jadpo files`](../language/14-tooling.md#jadpo-files)
- [Files](../language/16-built-in-services.md#files)
