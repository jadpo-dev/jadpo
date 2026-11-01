# `cli.unknown_command`

A `jadpo` command line starts with a command Jadpo doesn't have, or asks `jadpo help` about a command or rule it doesn't have.

## Why Jadpo refuses it

A command that isn't one could be a typo for another, and running the nearest one could change the project in a way you didn't ask for. So Jadpo runs nothing, and names the nearest command instead.

## What it looks like

The command is misspelt:

```sh run project=e-unknown-command expect=cli.unknown_command
jadpo chek
```

```text output
error: `chek` isn't a jadpo command
  reason: Jadpo has no command with this name. Did you mean `check`?
  next: Use `check` instead of `chek` (agent)
  rule: cli.unknown_command
  help: https://jadpo.dev/docs/errors/cli.unknown_command
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

`jadpo help` names the nearest rule the same way, for a word with a `.`:

```sh run project=e-unknown-command expect=cli.unknown_command
jadpo help impot.unused
```

```text output
error: `impot.unused` isn't a jadpo command or rule
  reason: `jadpo help` explains a command, or a rule by the id on an error's `rule:` line. Did you mean `import.unused`?
  next: Use `import.unused` instead of `impot.unused` (agent)
  rule: cli.unknown_command
  help: https://jadpo.dev/docs/errors/cli.unknown_command
```

## How to fix it

**If you meant the command it suggests**, use it:

```sh
jadpo check
```

The command runs as written.

**If none is near**, list every command:

```sh
jadpo help
```

Each command is listed with what it does.

## Related

- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
- [Commands for debugging, editors and help](../language/14-tooling.md#commands-for-debugging-editors-and-help)
