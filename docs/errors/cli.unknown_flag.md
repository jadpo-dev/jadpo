# `cli.unknown_flag`

A `jadpo` command line has a flag the command doesn't take.

## Why Jadpo refuses it

A flag that isn't one could be a typo for another, and running the command without it would do something you didn't ask for, such as printing text where JSON was expected. So nothing runs, and the nearest flag is named.

## What it looks like

The flag is misspelt:

```jadpo check project=e-unknown-flag file=app.jadpo hidden
application {}
```

```sh run project=e-unknown-flag expect=cli.unknown_flag
jadpo check --jsn
```

```text output
error: `--jsn` isn't a flag of `jadpo check`
  reason: `jadpo check` has no flag with this name. Did you mean `--json`?
  next: Use `--json` instead of `--jsn` (agent)
  rule: cli.unknown_flag
  help: https://jadpo.dev/docs/errors/cli.unknown_flag
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A flag no near one matches is refused with where to find them:

```sh run project=e-unknown-flag expect=cli.unknown_flag
jadpo check --watch
```

```text output
error: `--watch` isn't a flag of `jadpo check`
  reason: `jadpo check` has no flag with this name.
  next: Run `jadpo check --help` to see every flag (agent)
  rule: cli.unknown_flag
  help: https://jadpo.dev/docs/errors/cli.unknown_flag
```

## How to fix it

**If you meant the flag it suggests**, use it:

```sh
jadpo check --json
```

The command runs with that flag.

**If none is near**, see the command's flags:

```sh
jadpo check --help
```

Each flag is listed with what it does.

## Related

- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
- [Error messages](../language/14-tooling.md#error-messages)
